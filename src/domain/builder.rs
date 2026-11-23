use std::fs;
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use serde::Serialize;

use crate::app_state::{AppState, FontFamilySelection, MergingMode, PersistedConfig};
use crate::domain::backup_cloner::{clone_original_segoe, ensure_backup};
use crate::domain::registry::{set_font_registry_entries, windows_fonts_dir};

#[derive(Debug, Clone)]
pub enum BuildProgressEvent {
    Step { progress: f32, status: String },
    Log(String),
}

pub fn find_fontforge() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files\FontForgeBuilds\bin\ffpython.exe",
        r"C:\Program Files (x86)\FontForgeBuilds\bin\ffpython.exe",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(out) = Command::new("where")
        .arg("ffpython")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        && out.status.success()
    {
        let first = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if !first.is_empty() {
            return Some(PathBuf::from(first));
        }
    }
    None
}

pub fn install_fontforge_via_winget() -> anyhow::Result<()> {
    let status = Command::new("winget")
        .args([
            "install",
            "--id",
            "FontForge.FontForge",
            "-e",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .status()?;
    if !status.success() {
        anyhow::bail!("winget install FontForge.FontForge failed with {status}");
    }
    Ok(())
}

pub fn scripts_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let near_exe = exe_dir.join("scripts");
        if near_exe.exists() {
            return near_exe;
        }
    }
    let cwd_scripts = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("scripts");
    cwd_scripts.canonicalize().unwrap_or(cwd_scripts)
}

#[derive(Serialize)]
struct SlicerReq<'a> {
    base_path: &'a str,
    is_var: bool,
    reg: &'a str,
    light: &'a str,
    semilight: &'a str,
    semibold: &'a str,
    bold: &'a str,
    black: &'a str,
    prefix: &'a str,
    extra_ranges: &'a [(u32, u32)],
}

fn path_or_none(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_else(|| "NONE".to_string())
}

pub fn ensure_fonttools_python(
    ff_exe: &Path,
    mut on_log: impl FnMut(String),
) -> anyhow::Result<PathBuf> {
    let has_fonttools = |exe: &Path| -> bool {
        Command::new(exe)
            .args(["-c", "import fontTools"])
            .output()
            .is_ok_and(|o| o.status.success())
    };

    let sys_python = PathBuf::from("python");
    if has_fonttools(&sys_python) {
        return Ok(sys_python);
    }

    let ff_python = ff_exe.with_file_name("ffpython.exe");
    if ff_python.exists() && has_fonttools(&ff_python) {
        return Ok(ff_python);
    }

    let install_with = |py_exe: &Path, on_log: &mut dyn FnMut(String)| -> bool {
        let Ok(mut child) = Command::new(py_exe)
            .args(["-u", "-m", "pip", "install", "fonttools"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        else {
            return false;
        };

        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                on_log(line);
            }
        }
        child.wait().is_ok_and(|s| s.success()) && has_fonttools(py_exe)
    };

    on_log("[Setup] Installing Python 'fonttools' via pip...".to_string());
    if install_with(&sys_python, &mut on_log) {
        return Ok(sys_python);
    }
    if ff_python.exists() && install_with(&ff_python, &mut on_log) {
        return Ok(ff_python);
    }

    anyhow::bail!("Python package 'fonttools' is required for variable font slicing and could not be installed.")
}

fn check_cancelled(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Build cancelled by user.");
    }
    Ok(())
}

/// Runs `cmd` while watching `cancel`; if cancelled, terminates the child process immediately.
fn wait_child_with_cancel(
    mut child: Child,
    cancel: &AtomicBool,
    mut on_line: impl FnMut(bool, String),
) -> anyhow::Result<std::process::ExitStatus> {
    if cancel.load(Ordering::Relaxed) {
        let _ = child.kill();
        anyhow::bail!("Build cancelled by user.");
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<(bool, String)>();

    let out_handle = stdout.map(|out| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send((false, line));
            }
        })
    });
    let err_handle = stderr.map(|err| {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx.send((true, line));
            }
        })
    });
    drop(tx);

    let status = loop {
        while let Ok((is_err, line)) = rx.try_recv() {
            on_line(is_err, line);
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(h) = out_handle {
                let _ = h.join();
            }
            if let Some(h) = err_handle {
                let _ = h.join();
            }
            anyhow::bail!("Build cancelled by user.");
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };

    if let Some(h) = out_handle {
        let _ = h.join();
    }
    if let Some(h) = err_handle {
        let _ = h.join();
    }
    while let Ok((is_err, line)) = rx.try_recv() {
        on_line(is_err, line);
    }

    Ok(status)
}

struct SliceRequest<'a> {
    work_dir: &'a Path,
    sel: &'a FontFamilySelection,
    prefix: &'a str,
    slicer_script: &'a Path,
    ff_exe: &'a Path,
    base_progress: f32,
    progress_span: f32,
    extra_ranges: &'a [(u32, u32)],
    cancel: &'a AtomicBool,
}

fn resolve_family_weights(
    req_ctx: SliceRequest<'_>,
    on_event: &mut impl FnMut(BuildProgressEvent),
) -> anyhow::Result<(Vec<String>, Vec<PathBuf>)> {
    let SliceRequest {
        work_dir,
        sel,
        prefix,
        slicer_script,
        ff_exe,
        base_progress,
        progress_span,
        extra_ranges,
        cancel,
    } = req_ctx;
    check_cancelled(cancel)?;
    if !sel.has_regular() {
        return Ok((vec!["NONE".to_string(); 6], Vec::new()));
    }

    if !sel.is_variable {
        let r = path_or_none(&sel.regular);
        let lt = sel
            .light
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| r.clone());
        let sl = sel
            .semilight
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| r.clone());
        let b = sel
            .bold
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| r.clone());
        let sb = sel
            .semibold
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| b.clone());
        let blk = sel
            .black
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| b.clone());
        return Ok((vec![lt, sl, r, sb, b, blk], Vec::new()));
    }

    let python_exe = ensure_fonttools_python(ff_exe, |line| {
        on_event(BuildProgressEvent::Log(line));
    })?;
    check_cancelled(cancel)?;

    let req = SlicerReq {
        base_path: &work_dir.to_string_lossy(),
        is_var: sel.is_variable,
        reg: &path_or_none(&sel.regular),
        light: &path_or_none(&sel.light),
        semilight: &path_or_none(&sel.semilight),
        semibold: &path_or_none(&sel.semibold),
        bold: &path_or_none(&sel.bold),
        black: &path_or_none(&sel.black),
        prefix,
        extra_ranges,
    };

    let req_path = work_dir.join(format!("slicer_{prefix}.json"));
    fs::write(&req_path, serde_json::to_string(&req)?)?;

    let child = Command::new(&python_exe)
        .arg("-u")
        .arg(slicer_script)
        .arg(&req_path)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut parsed_paths: Option<Vec<String>> = None;
    let mut stderr_buf = String::new();
    let status_res = wait_child_with_cancel(child, cancel, |is_err, line| {
        if is_err {
            stderr_buf.push_str(&line);
            stderr_buf.push('\n');
            on_event(BuildProgressEvent::Log(line));
        } else if let Some(rest) = line.strip_prefix("SLICING_PROGRESS ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 3
                && let Some((cur, total)) = parts[0].split_once('/')
                && let (Ok(c), Ok(t)) = (cur.parse::<f32>(), total.parse::<f32>())
            {
                let pct = base_progress + progress_span * (c / t.max(1.0));
                on_event(BuildProgressEvent::Step {
                    progress: pct,
                    status: format!("Slicing {prefix} weight {} ({}/{})...", parts[2], cur, total),
                });
            }
            on_event(BuildProgressEvent::Log(format!("[Slicer] {rest}")));
        } else if let Some(json_str) = line.strip_prefix("SLICER_RESULT ") {
            parsed_paths = serde_json::from_str(json_str).ok();
        } else {
            on_event(BuildProgressEvent::Log(line));
        }
    });

    let _ = fs::remove_file(&req_path);
    let status = status_res?;
    if !status.success() {
        anyhow::bail!("variable_slicer failed for {prefix}: {stderr_buf}");
    }

    let paths = parsed_paths
        .ok_or_else(|| anyhow::anyhow!("variable_slicer produced no result for {prefix}"))?;
    let temps = paths.iter().map(PathBuf::from).collect();
    Ok((paths, temps))
}

pub fn parse_engine_progress(line: &str, khmer_enabled: bool) -> Option<(f32, String)> {
    let lower = line.to_ascii_lowercase();
    if let Some(pos) = lower.find("processing ") {
        let rest = &line[pos + "processing ".len()..];
        let weight = rest.split_whitespace().next().unwrap_or("...");
        let pct = match weight.to_ascii_lowercase().as_str() {
            "light" => 0.35,
            "semilight" => 0.45,
            "regular" => 0.55,
            "semibold" => 0.65,
            "bold" => 0.75,
            "black" => 0.85,
            _ => 0.60,
        };
        return Some((pct, format!("Building {weight} weight...")));
    }
    if line.contains("Assembling") {
        let msg = if khmer_enabled {
            "Merging with Khmer...".to_string()
        } else {
            "Assembling final font...".to_string()
        };
        return Some((0.88, msg));
    }
    if line.contains("All system replacement fonts built successfully") {
        return Some((0.92, "Finalizing...".to_string()));
    }
    None
}

pub fn run_build_and_apply(
    config: &PersistedConfig,
    ff_exe: &Path,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(BuildProgressEvent),
) -> anyhow::Result<()> {
    let scripts = scripts_dir();
    let slicer_script = scripts.join("variable_slicer.py");
    let work_dir = AppState::app_docs_dir().join("work");
    fs::create_dir_all(&work_dir)?;

    let mut temp_files: Vec<PathBuf> = Vec::new();
    let mut full_log = String::new();

    on_event(BuildProgressEvent::Step {
        progress: 0.05,
        status: "Backing up stock Segoe UI...".to_string(),
    });
    let backup_path = ensure_backup()?;

    on_event(BuildProgressEvent::Step {
        progress: 0.12,
        status: "Resolving & slicing font weights...".to_string(),
    });

    let extra_ranges = config.secondary_script.ranges();
    let (l_paths, t1) = resolve_family_weights(
        SliceRequest {
            work_dir: &work_dir,
            sel: &config.latin,
            prefix: "lat",
            slicer_script: &slicer_script,
            ff_exe,
            base_progress: 0.12,
            progress_span: 0.08,
            extra_ranges,
            cancel,
        },
        &mut on_event,
    )?;
    let (k_paths, t2) = if config.khmer.has_regular()
        && config.khmer.regular == config.latin.regular
    {
        on_event(BuildProgressEvent::Log(
            "[Slicer] Secondary font matches Latin font; using single-font build mode.".to_string(),
        ));
        (vec!["NONE".to_string(); 6], Vec::new())
    } else {
        resolve_family_weights(
            SliceRequest {
                work_dir: &work_dir,
                sel: &config.khmer,
                prefix: "khm",
                slicer_script: &slicer_script,
                ff_exe,
                base_progress: 0.20,
                progress_span: 0.08,
                extra_ranges,
                cancel,
            },
            &mut on_event,
        )?
    };
    temp_files.extend(t1);
    temp_files.extend(t2);

    let khmer_enabled = k_paths.iter().any(|p| p != "NONE");
    let italic_enabled = config.enable_italic && config.latin_italic.has_regular();

    let (li_paths, ki_paths) = if italic_enabled {
        let (li, t3) = resolve_family_weights(
            SliceRequest {
                work_dir: &work_dir,
                sel: &config.latin_italic,
                prefix: "lati",
                slicer_script: &slicer_script,
                ff_exe,
                base_progress: 0.28,
                progress_span: 0.01,
                extra_ranges,
                cancel,
            },
            &mut on_event,
        )?;
        let (ki, t4) = resolve_family_weights(
            SliceRequest {
                work_dir: &work_dir,
                sel: &config.khmer_italic,
                prefix: "khmi",
                slicer_script: &slicer_script,
                ff_exe,
                base_progress: 0.29,
                progress_span: 0.01,
                extra_ranges,
                cancel,
            },
            &mut on_event,
        )?;
        temp_files.extend(t3);
        temp_files.extend(t4);
        (li, ki)
    } else {
        (Vec::new(), Vec::new())
    };
    check_cancelled(cancel)?;

    if let Some(reg) = &config.latin.regular {
        let spoof_dst = work_dir.join("SegUIVar_system_mod.ttf");
        let spoofed = crate::domain::backup_cloner::patch_font_name_table(
            reg,
            &spoof_dst,
            |name_id, _| match name_id {
                6 => Some("SegoeUIVariable".to_string()),
                1 | 4 | 16 => Some("Segoe UI Variable".to_string()),
                _ => None,
            },
        )
        .is_ok();
        if !spoofed {
            fs::copy(reg, spoof_dst)?;
        }
    }

    on_event(BuildProgressEvent::Step {
        progress: 0.30,
        status: "Building replacement fonts...".to_string(),
    });

    let engine_name = match config.merging_mode {
        MergingMode::Visual => "engine.py",
        MergingMode::GridSync => "grid_sync_engine.py",
    };
    run_fontforge_engine(
        EngineInvocation {
            ff_exe,
            script: &scripts.join(engine_name),
            work_dir: &work_dir,
            latin_paths: &l_paths,
            khmer_paths: &k_paths,
            mode: config.merging_mode,
            khmer_enabled,
            extra_ranges,
            cancel,
        },
        &mut full_log,
        &mut on_event,
    )?;

    if italic_enabled {
        check_cancelled(cancel)?;
        on_event(BuildProgressEvent::Step {
            progress: 0.92,
            status: "Building Italic weights...".to_string(),
        });
        let ital_name = match config.merging_mode {
            MergingMode::Visual => "engine_italic.py",
            MergingMode::GridSync => "grid_sync_engine_italic.py",
        };
        run_fontforge_engine(
            EngineInvocation {
                ff_exe,
                script: &scripts.join(ital_name),
                work_dir: &work_dir,
                latin_paths: &li_paths,
                khmer_paths: &ki_paths,
                mode: config.merging_mode,
                khmer_enabled,
                extra_ranges,
                cancel,
            },
            &mut full_log,
            &mut on_event,
        )?;
    }

    check_cancelled(cancel)?;

    on_event(BuildProgressEvent::Step {
        progress: 0.95,
        status: "Installing fonts & updating Windows Registry...".to_string(),
    });

    let mut generated: Vec<(&str, &str)> = vec![
        ("segoeuil_system_mod.ttf", "Segoe UI Light (TrueType)"),
        ("segoeuisl_system_mod.ttf", "Segoe UI Semilight (TrueType)"),
        ("segoeui_system_mod.ttf", "Segoe UI (TrueType)"),
        ("seguisb_system_mod.ttf", "Segoe UI Semibold (TrueType)"),
        ("segoeuib_system_mod.ttf", "Segoe UI Bold (TrueType)"),
        ("seguibl_system_mod.ttf", "Segoe UI Black (TrueType)"),
        ("SegUIVar_system_mod.ttf", "Segoe UI Variable (TrueType)"),
    ];
    if italic_enabled {
        generated.extend_from_slice(&[
            ("seguili_system_mod.ttf", "Segoe UI Light Italic (TrueType)"),
            ("seguisli_system_mod.ttf", "Segoe UI Semilight Italic (TrueType)"),
            ("segoeuii_system_mod.ttf", "Segoe UI Italic (TrueType)"),
            ("seguisbi_system_mod.ttf", "Segoe UI Semibold Italic (TrueType)"),
            ("segoeuiz_system_mod.ttf", "Segoe UI Bold Italic (TrueType)"),
            ("seguibli_system_mod.ttf", "Segoe UI Black Italic (TrueType)"),
        ]);
    }

    let fonts_dir = windows_fonts_dir();
    let tag = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(1);
    let mut reg_updates: Vec<(&str, String)> = Vec::new();
    let mut has_base_segoe = false;
    for (file_name, reg_key) in &generated {
        let src = work_dir.join(file_name);
        if src.exists() {
            if *file_name == "segoeui_system_mod.ttf" {
                has_base_segoe = true;
            }
            let tagged_name = file_name.replace(".ttf", &format!("_{tag}.ttf"));
            let dst = fonts_dir.join(&tagged_name);
            fs::copy(&src, &dst).map_err(|e| {
                anyhow::anyhow!("Failed to install {} to {}: {e}", tagged_name, dst.display())
            })?;
            reg_updates.push((*reg_key, tagged_name));
        }
    }
    if !has_base_segoe {
        anyhow::bail!(
            "FontForge finished but segoeui_system_mod.ttf was not produced in {}",
            work_dir.display()
        );
    }
    let reg_refs: Vec<(&str, &str)> = reg_updates
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    set_font_registry_entries(&reg_refs)?;
    let _ = crate::domain::registry::cleanup_stale_mod_files();
    // Point Desktop/Explorer WindowMetrics from "Segoe UI Variable Text" to "Segoe UI"
    let _ = crate::domain::resizer::set_system_font_face("Segoe UI");

    if config.save_log {
        fs::write(
            AppState::app_docs_dir().join("SegoeUIChanger_Build_Log.txt"),
            &full_log,
        )?;
    }

    if config.save_fonts {
        let out_dir = AppState::app_docs_dir().join("Outputs");
        fs::create_dir_all(&out_dir)?;
        for (file_name, _) in &generated {
            let src = work_dir.join(file_name);
            if src.exists() {
                fs::copy(&src, out_dir.join(file_name))?;
            }
        }
    }

    if config.clone_segoe {
        clone_original_segoe(ff_exe, &backup_path)?;
    }

    for tmp in temp_files {
        let _ = fs::remove_file(tmp);
    }

    on_event(BuildProgressEvent::Step {
        progress: 1.0,
        status: "Success! Reboot required to apply changes.".to_string(),
    });
    Ok(())
}

struct EngineInvocation<'a> {
    ff_exe: &'a Path,
    script: &'a Path,
    work_dir: &'a Path,
    latin_paths: &'a [String],
    khmer_paths: &'a [String],
    mode: MergingMode,
    khmer_enabled: bool,
    extra_ranges: &'a [(u32, u32)],
    cancel: &'a AtomicBool,
}

fn run_fontforge_engine(
    inv: EngineInvocation<'_>,
    full_log: &mut String,
    on_event: &mut impl FnMut(BuildProgressEvent),
) -> anyhow::Result<()> {
    let ranges_json = serde_json::to_string(inv.extra_ranges).unwrap_or_else(|_| "[]".to_string());
    let mut cmd = Command::new(inv.ff_exe);
    cmd.arg("-u")
        .arg(inv.script)
        .args(inv.latin_paths)
        .args(inv.khmer_paths)
        .env("SEGOEUI_EXTRA_RANGES", ranges_json)
        .current_dir(inv.work_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if inv
        .script
        .file_name()
        .is_some_and(|n| n == "engine.py" || n == "engine_italic.py")
    {
        cmd.arg(inv.mode.as_arg());
    }

    let child = cmd.spawn()?;
    let mut suppressed_subtable_warnings = 0usize;
    let status = wait_child_with_cancel(child, inv.cancel, |is_err, line| {
        full_log.push_str(&line);
        full_log.push('\n');
        if !is_err
            && let Some((progress, status)) = parse_engine_progress(&line, inv.khmer_enabled)
        {
            on_event(BuildProgressEvent::Step { progress, status });
        }
        if line.contains("making the whole subtable invalid") {
            suppressed_subtable_warnings += 1;
            return;
        }
        on_event(BuildProgressEvent::Log(line));
    })?;
    if suppressed_subtable_warnings > 0 {
        on_event(BuildProgressEvent::Log(format!(
            "[Engine] Suppressed {suppressed_subtable_warnings} unused-subtable warnings (see build log file for full details)."
        )));
    }

    if !status.success() {
        anyhow::bail!("FontForge engine failed with exit status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engine_progress_lines() {
        let (p, s) = parse_engine_progress("[Engine] Processing Bold weight...", true).unwrap();
        assert!((p - 0.75).abs() < f32::EPSILON);
        assert_eq!(s, "Building Bold weight...");

        let (p2, s2) = parse_engine_progress("Assembling final font...", true).unwrap();
        assert!((p2 - 0.88).abs() < f32::EPSILON);
        assert_eq!(s2, "Merging with Khmer...");
    }
}
