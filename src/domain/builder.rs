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
    python_exe: Option<&'a Path>,
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
        python_exe,
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

    let python_exe = python_exe.ok_or_else(|| {
        anyhow::anyhow!("Python 'fonttools' executable was not resolved for variable font slicing.")
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

    let child = Command::new(python_exe)
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

    let extra_ranges_vec = config.active_script_ranges();
    let extra_ranges = extra_ranges_vec.as_slice();
    let active_scripts = config.active_scripts();
    let extra_scripts: Vec<_> = active_scripts.iter().copied().skip(1).collect();
    let khmer_same_as_latin =
        config.khmer.has_regular() && config.khmer.regular == config.latin.regular;
    let italic_enabled = config.enable_italic && config.latin_italic.has_regular();
    let any_extra_var = extra_scripts.iter().any(|&sc| {
        config
            .script_font(sc)
            .is_some_and(|f| f.has_regular() && f.is_variable && f.regular != config.latin.regular)
            || (italic_enabled
                && config
                    .script_italic_font(sc)
                    .is_some_and(|f| f.has_regular() && f.is_variable))
    });
    let any_variable = (config.latin.has_regular() && config.latin.is_variable)
        || (!khmer_same_as_latin && config.khmer.has_regular() && config.khmer.is_variable)
        || (italic_enabled
            && ((config.latin_italic.has_regular() && config.latin_italic.is_variable)
                || (config.khmer_italic.has_regular() && config.khmer_italic.is_variable)))
        || any_extra_var;

    let slicer_python = if any_variable {
        Some(ensure_fonttools_python(ff_exe, |line| {
            on_event(BuildProgressEvent::Log(line));
        })?)
    } else {
        None
    };

    if khmer_same_as_latin {
        on_event(BuildProgressEvent::Log(
            "[Slicer] Secondary font matches Latin font; using single-font build mode.".to_string(),
        ));
    }

    let (l_res, k_res, li_res, ki_res) = std::thread::scope(|s| {
        let (tx, rx) = std::sync::mpsc::channel::<BuildProgressEvent>();

        let run_slice = |sel: &FontFamilySelection,
                         prefix: &str,
                         base_progress: f32,
                         progress_span: f32,
                         tx: std::sync::mpsc::Sender<BuildProgressEvent>| {
            resolve_family_weights(
                SliceRequest {
                    work_dir: &work_dir,
                    sel,
                    prefix,
                    slicer_script: &slicer_script,
                    python_exe: slicer_python.as_deref(),
                    base_progress,
                    progress_span,
                    extra_ranges,
                    cancel,
                },
                &mut |ev| {
                    let _ = tx.send(ev);
                },
            )
        };

        let tx1 = tx.clone();
        let h_lat = s.spawn(move || run_slice(&config.latin, "lat", 0.12, 0.08, tx1));

        let tx2 = tx.clone();
        let h_khm = (!khmer_same_as_latin)
            .then(|| s.spawn(move || run_slice(&config.khmer, "khm", 0.20, 0.08, tx2)));

        let tx3 = tx.clone();
        let h_lati = italic_enabled
            .then(|| s.spawn(move || run_slice(&config.latin_italic, "lati", 0.28, 0.01, tx3)));

        let tx4 = tx.clone();
        let h_khmi = italic_enabled
            .then(|| s.spawn(move || run_slice(&config.khmer_italic, "khmi", 0.29, 0.01, tx4)));

        drop(tx);
        for ev in rx {
            on_event(ev);
        }

        let l_res = h_lat
            .join()
            .unwrap_or_else(|_| Err(anyhow::anyhow!("Latin slicer thread panicked")));
        let k_res = match h_khm {
            Some(h) => h
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Secondary slicer thread panicked"))),
            None => Ok((vec!["NONE".to_string(); 6], Vec::new())),
        };
        let li_res = match h_lati {
            Some(h) => h
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Latin italic slicer thread panicked"))),
            None => Ok((Vec::new(), Vec::new())),
        };
        let ki_res = match h_khmi {
            Some(h) => h
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Secondary italic slicer thread panicked"))),
            None => Ok((Vec::new(), Vec::new())),
        };
        (l_res, k_res, li_res, ki_res)
    });

    let (l_paths, t1) = l_res?;
    let (k_paths, t2) = k_res?;
    let (li_paths, t3) = li_res?;
    let (ki_paths, t4) = ki_res?;
    temp_files.extend(t1);
    temp_files.extend(t2);
    temp_files.extend(t3);
    temp_files.extend(t4);

    let mut extra_normal_map: std::collections::HashMap<&'static str, Vec<String>> =
        std::collections::HashMap::new();
    let mut extra_italic_map: std::collections::HashMap<&'static str, Vec<String>> =
        std::collections::HashMap::new();

    for sc in &extra_scripts {
        if let Some(sel) = config.script_font(*sc)
            && sel.has_regular()
            && sel.regular != config.latin.regular
        {
            let prefix = format!("ext_{}", sc.label().to_ascii_lowercase());
            let (paths, temps) = resolve_family_weights(
                SliceRequest {
                    work_dir: &work_dir,
                    sel,
                    prefix: &prefix,
                    slicer_script: &slicer_script,
                    python_exe: slicer_python.as_deref(),
                    base_progress: 0.28,
                    progress_span: 0.01,
                    extra_ranges,
                    cancel,
                },
                &mut on_event,
            )?;
            temp_files.extend(temps);
            for (i, (w, _)) in NORMAL_WEIGHT_SPECS.iter().enumerate() {
                if let Some(p) = paths.get(i)
                    && p != "NONE"
                {
                    extra_normal_map.entry(*w).or_default().push(p.clone());
                }
            }
        }
        if italic_enabled
            && let Some(sel) = config.script_italic_font(*sc)
            && sel.has_regular()
            && sel.regular != config.latin_italic.regular
        {
            let prefix = format!("exti_{}", sc.label().to_ascii_lowercase());
            let (paths, temps) = resolve_family_weights(
                SliceRequest {
                    work_dir: &work_dir,
                    sel,
                    prefix: &prefix,
                    slicer_script: &slicer_script,
                    python_exe: slicer_python.as_deref(),
                    base_progress: 0.29,
                    progress_span: 0.01,
                    extra_ranges,
                    cancel,
                },
                &mut on_event,
            )?;
            temp_files.extend(temps);
            for (i, (w, _)) in ITALIC_WEIGHT_SPECS.iter().enumerate() {
                if let Some(p) = paths.get(i)
                    && p != "NONE"
                {
                    extra_italic_map.entry(*w).or_default().push(p.clone());
                }
            }
        }
    }
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

    let core_script = scripts.join("engine_core.py");
    run_fontforge_engine(
        EngineInvocation {
            ff_exe,
            script: &core_script,
            work_dir: &work_dir,
            latin_paths: &l_paths,
            khmer_paths: &k_paths,
            mode: config.merging_mode,
            italic: false,
            base_progress: 0.30,
            progress_span: 0.60,
            extra_ranges,
            extra_font_map: &extra_normal_map,
            cancel,
        },
        &mut full_log,
        &mut on_event,
    )?;

    if italic_enabled {
        check_cancelled(cancel)?;
        on_event(BuildProgressEvent::Step {
            progress: 0.90,
            status: "Building Italic weights...".to_string(),
        });
        run_fontforge_engine(
            EngineInvocation {
                ff_exe,
                script: &core_script,
                work_dir: &work_dir,
                latin_paths: &li_paths,
                khmer_paths: &ki_paths,
                mode: config.merging_mode,
                italic: true,
                base_progress: 0.90,
                progress_span: 0.05,
                extra_ranges,
                extra_font_map: &extra_italic_map,
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

const NORMAL_WEIGHT_SPECS: [(&str, &str); 6] = [
    ("Light", "segoeuil.ttf"),
    ("Semilight", "segoeuisl.ttf"),
    ("Regular", "segoeui.ttf"),
    ("Semibold", "seguisb.ttf"),
    ("Bold", "segoeuib.ttf"),
    ("Black", "seguibl.ttf"),
];

const ITALIC_WEIGHT_SPECS: [(&str, &str); 6] = [
    ("Light Italic", "seguili.ttf"),
    ("Semilight Italic", "seguisli.ttf"),
    ("Italic", "segoeuii.ttf"),
    ("Semibold Italic", "seguisbi.ttf"),
    ("Bold Italic", "segoeuiz.ttf"),
    ("Black Italic", "seguibli.ttf"),
];

struct EngineInvocation<'a> {
    ff_exe: &'a Path,
    script: &'a Path,
    work_dir: &'a Path,
    latin_paths: &'a [String],
    khmer_paths: &'a [String],
    mode: MergingMode,
    italic: bool,
    base_progress: f32,
    progress_span: f32,
    extra_ranges: &'a [(u32, u32)],
    extra_font_map: &'a std::collections::HashMap<&'static str, Vec<String>>,
    cancel: &'a AtomicBool,
}

enum WorkerMsg {
    Log { weight: &'static str, line: String },
    Done { weight: &'static str },
    Err(String),
}

fn run_fontforge_engine(
    inv: EngineInvocation<'_>,
    full_log: &mut String,
    on_event: &mut impl FnMut(BuildProgressEvent),
) -> anyhow::Result<()> {
    let specs = if inv.italic {
        &ITALIC_WEIGHT_SPECS
    } else {
        &NORMAL_WEIGHT_SPECS
    };
    let jobs: Vec<(&'static str, &str, &str, &'static str)> = specs
        .iter()
        .enumerate()
        .filter_map(|(i, (weight, sys_ttf))| {
            let lat = inv.latin_paths.get(i).map(String::as_str).unwrap_or("NONE");
            let ara = inv.khmer_paths.get(i).map(String::as_str).unwrap_or("NONE");
            (lat != "NONE").then_some((*weight, lat, ara, *sys_ttf))
        })
        .collect();

    if jobs.is_empty() {
        return Ok(());
    }

    let total_jobs = jobs.len();
    let worker_count = total_jobs
        .min(std::thread::available_parallelism().map_or(2, |n| n.get()))
        .clamp(1, 4);
    let ranges_json = serde_json::to_string(inv.extra_ranges).unwrap_or_else(|_| "[]".to_string());
    let extra_fonts_json =
        serde_json::to_string(inv.extra_font_map).unwrap_or_else(|_| "{}".to_string());
    let mode_arg = inv.mode.as_arg();
    let queue = std::sync::Mutex::new(jobs.into_iter().collect::<std::collections::VecDeque<_>>());
    let (tx, rx) = std::sync::mpsc::channel::<WorkerMsg>();

    on_event(BuildProgressEvent::Log(format!(
        "[Engine] Building {total_jobs} weights across {worker_count} parallel FontForge worker(s)..."
    )));

    let mut suppressed_subtable_warnings = 0usize;
    let mut completed = 0usize;
    let mut first_error: Option<String> = None;

    std::thread::scope(|s| {
        for _ in 0..worker_count {
            let queue = &queue;
            let tx = tx.clone();
            let ranges_json = &ranges_json;
            let extra_fonts_json = &extra_fonts_json;
            s.spawn(move || {
                loop {
                    if inv.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let job = queue.lock().ok().and_then(|mut q| q.pop_front());
                    let Some((weight, lat, ara, sys_ttf)) = job else {
                        break;
                    };

                    let mut cmd = Command::new(inv.ff_exe);
                    cmd.arg("-u")
                        .arg(inv.script)
                        .args(["--single", weight, lat, ara, sys_ttf, mode_arg])
                        .env("SEGOEUI_EXTRA_RANGES", ranges_json)
                        .env("SEGOEUI_EXTRA_FONT_MAP", extra_fonts_json)
                        .current_dir(inv.work_dir)
                        .creation_flags(CREATE_NO_WINDOW)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());

                    let child = match cmd.spawn() {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx.send(WorkerMsg::Err(format!(
                                "Failed to spawn FontForge for {weight}: {e}"
                            )));
                            break;
                        }
                    };

                    let res = wait_child_with_cancel(child, inv.cancel, |_is_err, line| {
                        let _ = tx.send(WorkerMsg::Log { weight, line });
                    });

                    match res {
                        Ok(st) if st.success() => {
                            let _ = tx.send(WorkerMsg::Done { weight });
                        }
                        Ok(st) => {
                            let _ = tx.send(WorkerMsg::Err(format!(
                                "FontForge worker for {weight} exited with status {st}"
                            )));
                            break;
                        }
                        Err(e) => {
                            let _ = tx.send(WorkerMsg::Err(e.to_string()));
                            break;
                        }
                    }
                }
            });
        }
        drop(tx);

        for msg in rx {
            match msg {
                WorkerMsg::Log { weight, line } => {
                    full_log.push_str(&format!("[{weight}] {line}\n"));
                    if line.contains("making the whole subtable invalid") {
                        suppressed_subtable_warnings += 1;
                        continue;
                    }
                    on_event(BuildProgressEvent::Log(format!("[{weight}] {line}")));
                }
                WorkerMsg::Done { weight } => {
                    completed += 1;
                    let frac = completed as f32 / total_jobs as f32;
                    on_event(BuildProgressEvent::Step {
                        progress: inv.base_progress + inv.progress_span * frac,
                        status: format!("Built {weight} ({completed}/{total_jobs})"),
                    });
                }
                WorkerMsg::Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }
    });

    if suppressed_subtable_warnings > 0 {
        on_event(BuildProgressEvent::Log(format!(
            "[Engine] Suppressed {suppressed_subtable_warnings} unused-subtable warnings (see build log file for full details)."
        )));
    }

    check_cancelled(inv.cancel)?;
    if let Some(err) = first_error {
        anyhow::bail!("{err}");
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
