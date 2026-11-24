use std::fs;
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ENGINE_CORE_PY: &str = include_str!("../../scripts/engine_core.py");
const VARIABLE_SLICER_PY: &str = include_str!("../../scripts/variable_slicer.py");

use read_fonts::{FontRef, TableProvider};
use serde::Serialize;

use crate::app_state::{AppState, FontFamilySelection, MergingMode, PersistedConfig};
use crate::domain::backup_cloner::{clone_original_segoe, ensure_backup};
use crate::domain::elevation::is_admin;
use crate::domain::font_discovery::is_variable_font;
use crate::domain::registry::{
    SEGOE_FONT_SPECS, SegoeFontSpec, install_mod_fonts_from_bytes, windows_dir, windows_fonts_dir,
};

#[derive(Debug, Clone)]
pub enum BuildProgressEvent {
    Step { progress: f32, status: String },
    Log(String),
}

fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn is_under_program_files(canon_path: &Path) -> bool {
    let s = strip_verbatim_prefix(canon_path)
        .to_string_lossy()
        .to_ascii_lowercase();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(val) = std::env::var_os(var) {
            let mut base = PathBuf::from(val).to_string_lossy().to_ascii_lowercase();
            if !base.is_empty() {
                if !base.ends_with('\\') {
                    base.push('\\');
                }
                if s.starts_with(&base) {
                    return true;
                }
            }
        }
    }
    false
}

/// Validates a FontForge `ffpython*.exe` path (rejecting UNC paths, symlinks, and non-ProgramFiles paths when elevated).
pub fn is_trusted_ffpython_path(path: &Path) -> bool {
    if path.to_string_lossy().starts_with(r"\\") {
        return false;
    }
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.file_type().is_file() {
        return false;
    }
    let Ok(canon) = path.canonicalize() else {
        return false;
    };
    let clean = strip_verbatim_prefix(&canon);
    if clean.to_string_lossy().starts_with(r"\\") {
        return false;
    }
    let Some(fname) = clean
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };
    if !fname.starts_with("ffpython") || !fname.ends_with(".exe") {
        return false;
    }
    if is_admin() && !is_under_program_files(&clean) {
        return false;
    }
    true
}

pub fn resolve_trusted_fontforge(custom: Option<&Path>) -> Option<PathBuf> {
    custom
        .filter(|p| is_trusted_ffpython_path(p))
        .map(strip_verbatim_prefix)
        .or_else(find_fontforge)
}

fn find_exe_in_path(exe_name: &str, require_program_files: bool) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let mut fallback: Option<PathBuf> = None;
    for dir in std::env::split_paths(&path_var) {
        if !dir.is_absolute() {
            continue;
        }
        let dir_lc = dir.to_string_lossy().to_ascii_lowercase();
        if dir_lc.starts_with(r"\\") || dir_lc.contains(r"\windowsapps\") {
            continue;
        }
        let candidate = dir.join(exe_name);
        if fs::symlink_metadata(&candidate).is_ok_and(|m| m.file_type().is_file()) {
            let Ok(canon) = candidate.canonicalize() else {
                continue;
            };
            let clean = strip_verbatim_prefix(&canon);
            if clean.to_string_lossy().starts_with(r"\\") {
                continue;
            }
            if is_under_program_files(&clean) {
                return Some(clean);
            }
            if !require_program_files && fallback.is_none() {
                fallback = Some(clean);
            }
        }
    }
    fallback
}

pub fn find_fontforge() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(val) = std::env::var_os(var) {
            roots.push(PathBuf::from(val));
        }
    }
    if !is_admin()
        && let Some(local) = std::env::var_os("LOCALAPPDATA")
    {
        let local_path = PathBuf::from(local);
        roots.push(local_path.clone());
        roots.push(local_path.join("Programs"));
    }
    if let Some(sys_drive) = std::env::var_os("SystemDrive") {
        let drive = sys_drive.to_string_lossy();
        roots.push(PathBuf::from(format!(r"{drive}\Program Files")));
        roots.push(PathBuf::from(format!(r"{drive}\Program Files (x86)")));
    }
    roots.push(PathBuf::from(r"C:\Program Files"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)"));

    for root in roots {
        let candidate = root.join(r"FontForgeBuilds\bin\ffpython.exe");
        if is_trusted_ffpython_path(&candidate) {
            return Some(strip_verbatim_prefix(
                &candidate.canonicalize().unwrap_or(candidate),
            ));
        }
    }

    find_exe_in_path("ffpython.exe", is_admin())
}

pub fn install_fontforge_via_winget() -> anyhow::Result<()> {
    let winget = std::env::var_os("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join(r"Microsoft\WindowsApps\winget.exe"))
        .filter(|p| p.exists())
        .or_else(|| find_exe_in_path("winget.exe", false))
        .ok_or_else(|| anyhow::anyhow!("winget.exe not found on this Windows installation"))?;

    let out = Command::new(winget)
        .args([
            "install",
            "--id",
            "FontForge.FontForge",
            "-e",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = if err.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).to_string()
        } else {
            err.to_string()
        };
        anyhow::bail!(
            "winget install FontForge.FontForge failed with {}: {}",
            out.status,
            msg.trim()
        );
    }
    Ok(())
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
    cancel: &AtomicBool,
    mut on_log: impl FnMut(String),
) -> anyhow::Result<PathBuf> {
    let has_fonttools = |exe: &Path| -> bool {
        Command::new(exe)
            .args(["-c", "import fontTools"])
            .env("PYTHONSAFEPATH", "1")
            .env("PYTHONUTF8", "1")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|o| o.status.success())
    };

    let sys_python = find_exe_in_path("python.exe", false);
    if let Some(ref py) = sys_python
        && has_fonttools(py)
    {
        return Ok(py.clone());
    }

    if ff_exe.exists() && has_fonttools(ff_exe) {
        return Ok(ff_exe.to_path_buf());
    }

    let install_with = |py_exe: &Path, on_log: &mut dyn FnMut(String)| -> anyhow::Result<bool> {
        let Ok(child) = Command::new(py_exe)
            .args([
                "-u",
                "-m",
                "pip",
                "install",
                "--no-input",
                "--retries",
                "1",
                "--timeout",
                "10",
                "fonttools>=4.50,<5",
            ])
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8:replace")
            .env("PYTHONSAFEPATH", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
        else {
            return Ok(false);
        };

        let no_abort = AtomicBool::new(false);
        let status = wait_child_with_cancel(child, cancel, &no_abort, |_is_err, line| on_log(line))?;
        Ok(status.success() && has_fonttools(py_exe))
    };

    on_log("[Setup] Installing Python 'fonttools' via pip...".to_string());
    if let Some(ref py) = sys_python
        && install_with(py, &mut on_log)?
    {
        return Ok(py.clone());
    }
    if ff_exe.exists() && install_with(ff_exe, &mut on_log)? {
        return Ok(ff_exe.to_path_buf());
    }

    anyhow::bail!("Python package 'fonttools' is required for variable font slicing and could not be installed.")
}

fn check_cancelled(cancel: &AtomicBool) -> anyhow::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Build cancelled by user.");
    }
    Ok(())
}

fn read_lossy_lines<R: std::io::Read + Send + 'static>(
    stream: R,
    is_err: bool,
    tx: std::sync::mpsc::Sender<(bool, String)>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::with_capacity(256);
        while reader.read_until(b'\n', &mut buf).unwrap_or(0) > 0 {
            let line = String::from_utf8_lossy(&buf)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            buf.clear();
            let _ = tx.send((is_err, line));
        }
    })
}

/// Runs `cmd` while watching `cancel` and `abort`; if either triggers, kills the child process immediately.
fn wait_child_with_cancel(
    mut child: Child,
    cancel: &AtomicBool,
    abort: &AtomicBool,
    mut on_line: impl FnMut(bool, String),
) -> anyhow::Result<std::process::ExitStatus> {
    if cancel.load(Ordering::Relaxed) || abort.load(Ordering::Relaxed) {
        let _ = child.kill();
        anyhow::bail!("Build cancelled by user.");
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<(bool, String)>();

    let out_handle = stdout.map(|out| read_lossy_lines(out, false, tx.clone()));
    let err_handle = stderr.map(|err| read_lossy_lines(err, true, tx.clone()));
    drop(tx);

    let status = loop {
        while let Ok((is_err, line)) = rx.try_recv() {
            on_line(is_err, line);
        }
        if cancel.load(Ordering::Relaxed) || abort.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            if let Some(h) = out_handle {
                let _ = h.join();
            }
            if let Some(h) = err_handle {
                let _ = h.join();
            }
            anyhow::bail!("Build cancelled or aborted.");
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

fn existing_path_str(p: &Option<PathBuf>) -> Option<String> {
    p.as_ref()
        .filter(|v| v.exists())
        .map(|v| v.to_string_lossy().to_string())
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
    let Some(reg_path) = sel.regular.as_ref().filter(|p| p.exists()) else {
        return Ok((vec!["NONE".to_string(); 6], Vec::new()));
    };

    let is_var = is_variable_font(reg_path);
    if !is_var {
        let r = reg_path.to_string_lossy().to_string();
        let lt = existing_path_str(&sel.light).unwrap_or_else(|| r.clone());
        let sl = existing_path_str(&sel.semilight).unwrap_or_else(|| r.clone());
        let b = existing_path_str(&sel.bold).unwrap_or_else(|| r.clone());
        let sb = existing_path_str(&sel.semibold).unwrap_or_else(|| b.clone());
        let blk = existing_path_str(&sel.black).unwrap_or_else(|| b.clone());
        return Ok((vec![lt, sl, r, sb, b, blk], Vec::new()));
    }

    let python_exe = python_exe.ok_or_else(|| {
        anyhow::anyhow!("Python 'fonttools' executable was not resolved for variable font slicing.")
    })?;
    check_cancelled(cancel)?;

    let req = SlicerReq {
        base_path: &work_dir.to_string_lossy(),
        is_var,
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

    let windir = windows_dir();
    let child = Command::new(python_exe)
        .arg("-u")
        .arg(slicer_script)
        .arg(&req_path)
        .env("PYTHONUTF8", "1")
        .env("PYTHONIOENCODING", "utf-8:replace")
        .env("PYTHONSAFEPATH", "1")
        .env("SEGOEUI_WINDIR", &windir)
        .current_dir(work_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut parsed_paths: Option<Vec<String>> = None;
    let mut stderr_buf = String::new();
    let no_abort = AtomicBool::new(false);
    let status_res = wait_child_with_cancel(child, cancel, &no_abort, |is_err, line| {
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

    let raw_paths = parsed_paths
        .ok_or_else(|| anyhow::anyhow!("variable_slicer produced no result for {prefix}"))?;
    let canon_work = work_dir.canonicalize()?;
    let mut verified_paths = Vec::with_capacity(raw_paths.len());
    let mut temps = Vec::with_capacity(raw_paths.len());
    for raw in raw_paths {
        let p = PathBuf::from(&raw);
        if !fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_file()) {
            anyhow::bail!("Invalid sliced font path returned: {}", p.display());
        }
        let canon = p.canonicalize()?;
        let fname = canon
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if canon.parent() != Some(canon_work.as_path())
            || !fname.starts_with("temp_")
            || !fname.ends_with(".ttf")
        {
            anyhow::bail!("Untrusted sliced output outside work directory: {}", p.display());
        }
        let clean = strip_verbatim_prefix(&canon);
        verified_paths.push(clean.to_string_lossy().to_string());
        temps.push(clean);
    }
    Ok((verified_paths, temps))
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

struct BuildDirGuard(PathBuf);

impl Drop for BuildDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn create_private_build_dir() -> anyhow::Result<BuildDirGuard> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!("segoeui_build_{}_{}", std::process::id(), ts);
    let primary = if is_admin() {
        windows_dir().join("Temp").join(&name)
    } else {
        std::env::temp_dir().join(&name)
    };
    if fs::create_dir_all(&primary).is_ok() {
        return Ok(BuildDirGuard(strip_verbatim_prefix(&primary)));
    }
    let fallback = std::env::temp_dir().join(&name);
    fs::create_dir_all(&fallback)?;
    Ok(BuildDirGuard(strip_verbatim_prefix(&fallback)))
}

pub fn built_fonts_dir() -> PathBuf {
    AppState::app_docs_dir().join("work")
}

pub fn has_built_fonts() -> bool {
    built_fonts_dir().join("segoeui_system_mod.ttf").is_file()
}

/// Step 1: Slices variable fonts, builds replacement `.ttf` files via FontForge, validates them,
/// and writes them into `<app>_data/work` for previewing or applying. Does not require Admin.
pub fn run_build_only(
    config: &PersistedConfig,
    ff_exe: &Path,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(BuildProgressEvent),
) -> anyhow::Result<usize> {
    let build_dir_guard = create_private_build_dir()?;
    let work_dir = build_dir_guard.0.clone();
    let slicer_script = work_dir.join("variable_slicer.py");
    let core_script = work_dir.join("engine_core.py");
    fs::write(&slicer_script, VARIABLE_SLICER_PY)?;
    fs::write(&core_script, ENGINE_CORE_PY)?;

    let mut temp_files: Vec<PathBuf> = Vec::new();
    let mut full_log = String::new();

    on_event(BuildProgressEvent::Step {
        progress: 0.08,
        status: "Resolving & slicing font weights...".to_string(),
    });

    let extra_ranges_vec = config.active_script_ranges();
    let extra_ranges = extra_ranges_vec.as_slice();
    let active_scripts = config.active_scripts();
    let primary_script = active_scripts[0];
    let primary_sel = config.script_font(primary_script).cloned().unwrap_or_default();
    let primary_italic_sel = config
        .script_italic_font(primary_script)
        .cloned()
        .unwrap_or_default();
    let extra_scripts: Vec<_> = active_scripts.iter().copied().skip(1).collect();
    let khmer_same_as_latin =
        primary_sel.has_regular() && primary_sel.regular == config.latin.regular;
    let italic_enabled = config.enable_italic && config.latin_italic.has_regular();
    let khmi_same_as_latin_italic = italic_enabled
        && primary_italic_sel.has_regular()
        && primary_italic_sel.regular == config.latin_italic.regular;
    let is_sel_var = |s: &FontFamilySelection| {
        s.regular
            .as_ref()
            .filter(|p| p.exists())
            .is_some_and(|p| is_variable_font(p))
    };
    let any_extra_var = extra_scripts.iter().any(|&sc| {
        config
            .script_font(sc)
            .is_some_and(|f| f.regular != config.latin.regular && is_sel_var(f))
            || (italic_enabled
                && config
                    .script_italic_font(sc)
                    .is_some_and(|f| f.regular != config.latin_italic.regular && is_sel_var(f)))
    });
    let any_variable = is_sel_var(&config.latin)
        || (!khmer_same_as_latin && is_sel_var(&primary_sel))
        || (italic_enabled
            && (is_sel_var(&config.latin_italic)
                || (!khmi_same_as_latin_italic && is_sel_var(&primary_italic_sel))))
        || any_extra_var;

    let slicer_python = if any_variable {
        Some(ensure_fonttools_python(ff_exe, cancel, |line| {
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

    let (l_res, k_res, li_res, ki_res, extra_normal_res, extra_italic_res) =
        std::thread::scope(|s| {
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
                .then(|| s.spawn(move || run_slice(&primary_sel, "khm", 0.20, 0.08, tx2)));

            let tx3 = tx.clone();
            let h_lati = italic_enabled
                .then(|| s.spawn(move || run_slice(&config.latin_italic, "lati", 0.28, 0.01, tx3)));

            let tx4 = tx.clone();
            let h_khmi = (italic_enabled && !khmi_same_as_latin_italic)
                .then(|| s.spawn(move || run_slice(&primary_italic_sel, "khmi", 0.29, 0.01, tx4)));

            let mut h_ext_norm = Vec::new();
            let mut h_ext_ital = Vec::new();
            for &sc in &extra_scripts {
                if let Some(sel) = config.script_font(sc)
                    && sel.has_regular()
                    && sel.regular != config.latin.regular
                {
                    let prefix = format!("ext_{}", sc.label().to_ascii_lowercase());
                    let tx_e = tx.clone();
                    h_ext_norm.push((
                        sc,
                        s.spawn(move || run_slice(sel, &prefix, 0.28, 0.01, tx_e)),
                    ));
                }
                if italic_enabled
                    && let Some(sel) = config.script_italic_font(sc)
                    && sel.has_regular()
                    && sel.regular != config.latin_italic.regular
                {
                    let prefix = format!("exti_{}", sc.label().to_ascii_lowercase());
                    let tx_e = tx.clone();
                    h_ext_ital.push((
                        sc,
                        s.spawn(move || run_slice(sel, &prefix, 0.29, 0.01, tx_e)),
                    ));
                }
            }

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
            let extra_normal_res: Vec<_> = h_ext_norm
                .into_iter()
                .map(|(sc, h)| {
                    (
                        sc,
                        h.join().unwrap_or_else(|_| {
                            Err(anyhow::anyhow!("Extra script slicer thread panicked"))
                        }),
                    )
                })
                .collect();
            let extra_italic_res: Vec<_> = h_ext_ital
                .into_iter()
                .map(|(sc, h)| {
                    (
                        sc,
                        h.join().unwrap_or_else(|_| {
                            Err(anyhow::anyhow!("Extra script italic slicer thread panicked"))
                        }),
                    )
                })
                .collect();
            (l_res, k_res, li_res, ki_res, extra_normal_res, extra_italic_res)
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

    for (_, res) in extra_normal_res {
        let (paths, temps) = res?;
        temp_files.extend(temps);
        for (i, spec) in SEGOE_FONT_SPECS
            .iter()
            .filter(|s| !s.is_italic && !s.is_variable)
            .enumerate()
        {
            if let Some(p) = paths.get(i)
                && p != "NONE"
            {
                extra_normal_map
                    .entry(spec.weight_name)
                    .or_default()
                    .push(p.clone());
            }
        }
    }
    for (_, res) in extra_italic_res {
        let (paths, temps) = res?;
        temp_files.extend(temps);
        for (i, spec) in SEGOE_FONT_SPECS.iter().filter(|s| s.is_italic).enumerate() {
            if let Some(p) = paths.get(i)
                && p != "NONE"
            {
                extra_italic_map
                    .entry(spec.weight_name)
                    .or_default()
                    .push(p.clone());
            }
        }
    }
    check_cancelled(cancel)?;

    let fonts_dir = windows_fonts_dir();
    let has_seguivar = fonts_dir.join("SegUIVar.ttf").exists();
    if has_seguivar && let Some(reg) = &config.latin.regular {
        let is_truetype_glyf = fs::read(reg)
            .ok()
            .and_then(|b| FontRef::new(&b).ok().map(|f| f.glyf().is_ok()))
            .unwrap_or(false);
        if is_truetype_glyf {
            let spoof_dst = work_dir.join("SegUIVar_system_mod.ttf");
            if let Err(e) = crate::domain::backup_cloner::patch_font_name_table(
                reg,
                &spoof_dst,
                |name_id, _| match name_id {
                    6 => Some("SegoeUIVariable".to_string()),
                    1 | 4 | 16 => Some("Segoe UI Variable".to_string()),
                    _ => None,
                },
            ) {
                on_event(BuildProgressEvent::Log(format!(
                    "[Engine] Warning: skipped SegUIVar spoof ({e})"
                )));
            }
        } else {
            on_event(BuildProgressEvent::Log(
                "[Engine] Notice: primary font is not TrueType glyf; skipping SegUIVar_system_mod.ttf."
                    .to_string(),
            ));
        }
    }

    on_event(BuildProgressEvent::Step {
        progress: 0.30,
        status: "Building replacement fonts...".to_string(),
    });

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
        status: "Validating & saving built fonts...".to_string(),
    });

    let generated_specs: Vec<&SegoeFontSpec> = SEGOE_FONT_SPECS
        .iter()
        .filter(|s| (italic_enabled || !s.is_italic) && (!s.is_variable || has_seguivar))
        .collect();

    let mut built_items: Vec<(&SegoeFontSpec, Vec<u8>)> = Vec::new();
    for spec in &generated_specs {
        let src = work_dir.join(spec.mod_file);
        if fs::symlink_metadata(&src).is_ok_and(|m| m.file_type().is_file()) {
            let bytes = fs::read(&src)?;
            if bytes.is_empty() || FontRef::new(&bytes).is_err() {
                anyhow::bail!("FontForge produced invalid font file: {}", spec.mod_file);
            }
            built_items.push((*spec, bytes));
        }
    }

    if !built_items
        .iter()
        .any(|(spec, _)| spec.mod_file == "segoeui_system_mod.ttf")
    {
        anyhow::bail!(
            "FontForge finished but segoeui_system_mod.ttf was not produced in {}",
            work_dir.display()
        );
    }

    let docs_dir = AppState::app_docs_dir();
    let app_work = built_fonts_dir();
    fs::create_dir_all(&app_work)?;
    if let Ok(entries) = fs::read_dir(&app_work) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with("_system_mod.ttf") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    for (spec, bytes) in &built_items {
        write_regular_file_checked(&app_work.join(spec.mod_file), bytes)?;
    }

    if config.save_log
        && let Err(e) = write_regular_file_checked(
            &docs_dir.join("TypeMod_Build_Log.txt"),
            full_log.as_bytes(),
        )
    {
        on_event(BuildProgressEvent::Log(format!(
            "[Warning] Failed to save build log: {e}"
        )));
    }

    if config.save_fonts
        && let Err(e) = (|| -> anyhow::Result<()> {
            let out_dir = docs_dir.join("Outputs");
            fs::create_dir_all(&out_dir)?;
            for (spec, bytes) in &built_items {
                write_regular_file_checked(&out_dir.join(spec.mod_file), bytes)?;
            }
            Ok(())
        })()
    {
        on_event(BuildProgressEvent::Log(format!(
            "[Warning] Failed to export generated fonts: {e}"
        )));
    }

    let count = built_items.len();
    let _ = temp_files;

    on_event(BuildProgressEvent::Step {
        progress: 1.0,
        status: format!("Built {count} fonts! Preview or click Apply to Windows."),
    });
    Ok(count)
}

/// Step 2: Backs up stock Segoe UI, validates built fonts from `work_dir`,
/// installs them to `%WINDIR%\Fonts`, updates HKLM Registry, and optionally clones stock Segoe UI.
pub fn apply_built_fonts_from_dir(
    config: &PersistedConfig,
    ff_exe: Option<&Path>,
    work_dir: &Path,
    mut on_event: impl FnMut(BuildProgressEvent),
) -> anyhow::Result<usize> {
    on_event(BuildProgressEvent::Step {
        progress: 0.15,
        status: "Backing up stock Segoe UI...".to_string(),
    });
    let backup_path = ensure_backup()?;

    on_event(BuildProgressEvent::Step {
        progress: 0.45,
        status: "Validating built fonts...".to_string(),
    });
    let has_seguivar = windows_fonts_dir().join("SegUIVar.ttf").exists();
    let mut built_items: Vec<(&SegoeFontSpec, Vec<u8>)> = Vec::new();
    for spec in SEGOE_FONT_SPECS {
        if spec.is_variable && !has_seguivar {
            continue;
        }
        let src = work_dir.join(spec.mod_file);
        if fs::symlink_metadata(&src).is_ok_and(|m| m.file_type().is_file()) {
            let bytes = fs::read(&src)?;
            if bytes.is_empty() || FontRef::new(&bytes).is_err() {
                anyhow::bail!("Built font '{}' is invalid or corrupt", spec.mod_file);
            }
            built_items.push((spec, bytes));
        }
    }

    on_event(BuildProgressEvent::Step {
        progress: 0.70,
        status: "Installing fonts & updating Windows Registry...".to_string(),
    });
    let count = install_mod_fonts_from_bytes(&built_items)?;

    if config.clone_segoe
        && let Err(e) = clone_original_segoe(ff_exe.unwrap_or_else(|| Path::new("")), &backup_path)
    {
        on_event(BuildProgressEvent::Log(format!(
            "[Warning] Failed to clone original Segoe UI: {e}"
        )));
    }

    on_event(BuildProgressEvent::Step {
        progress: 1.0,
        status: "Applied! Reboot required to apply changes.".to_string(),
    });
    Ok(count)
}

pub fn apply_built_fonts(
    config: &PersistedConfig,
    ff_exe: Option<&Path>,
    on_event: impl FnMut(BuildProgressEvent),
) -> anyhow::Result<usize> {
    apply_built_fonts_from_dir(config, ff_exe, &built_fonts_dir(), on_event)
}

/// Runs both Step 1 (build) and Step 2 (apply) sequentially (used by `--cli-build`).
pub fn run_build_and_apply(
    config: &PersistedConfig,
    ff_exe: &Path,
    cancel: &AtomicBool,
    mut on_event: impl FnMut(BuildProgressEvent),
) -> anyhow::Result<()> {
    run_build_only(config, ff_exe, cancel, &mut on_event)?;
    check_cancelled(cancel)?;
    apply_built_fonts(config, Some(ff_exe), &mut on_event)?;
    Ok(())
}

fn write_regular_file_checked(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.file_type().is_symlink()
    {
        anyhow::bail!("Refusing to overwrite symlink at {}", path.display());
    }
    fs::write(path, bytes)?;
    Ok(())
}

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
    let jobs: Vec<(&'static str, &str, &str, &'static str)> = SEGOE_FONT_SPECS
        .iter()
        .filter(|s| !s.is_variable && s.is_italic == inv.italic)
        .enumerate()
        .filter_map(|(i, spec)| {
            let lat = inv.latin_paths.get(i).map(String::as_str).unwrap_or("NONE");
            let ara = inv.khmer_paths.get(i).map(String::as_str).unwrap_or("NONE");
            (lat != "NONE").then_some((spec.weight_name, lat, ara, spec.stock_file))
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
    let abort_workers = AtomicBool::new(false);
    let windir = windows_dir();

    std::thread::scope(|s| {
        for _ in 0..worker_count {
            let queue = &queue;
            let tx = tx.clone();
            let ranges_json = &ranges_json;
            let extra_fonts_json = &extra_fonts_json;
            let abort_workers = &abort_workers;
            let windir = &windir;
            s.spawn(move || {
                loop {
                    if inv.cancel.load(Ordering::Relaxed) || abort_workers.load(Ordering::Relaxed) {
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
                        .env("SEGOEUI_WINDIR", windir)
                        .env("PYTHONUTF8", "1")
                        .env("PYTHONIOENCODING", "utf-8:replace")
                        .env("PYTHONNOUSERSITE", "1")
                        .env("PYTHONSAFEPATH", "1")
                        .env("PYTHONPATH", "")
                        .current_dir(inv.work_dir)
                        .creation_flags(CREATE_NO_WINDOW)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());

                    let child = match cmd.spawn() {
                        Ok(c) => c,
                        Err(e) => {
                            abort_workers.store(true, Ordering::Relaxed);
                            let _ = tx.send(WorkerMsg::Err(format!(
                                "Failed to spawn FontForge for {weight}: {e}"
                            )));
                            break;
                        }
                    };

                    let res = wait_child_with_cancel(child, inv.cancel, abort_workers, |_is_err, line| {
                        let _ = tx.send(WorkerMsg::Log { weight, line });
                    });

                    match res {
                        Ok(st) if st.success() => {
                            let _ = tx.send(WorkerMsg::Done { weight });
                        }
                        Ok(st) => {
                            abort_workers.store(true, Ordering::Relaxed);
                            let _ = tx.send(WorkerMsg::Err(format!(
                                "FontForge worker for {weight} exited with status {st}"
                            )));
                            break;
                        }
                        Err(e) => {
                            abort_workers.store(true, Ordering::Relaxed);
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
                    abort_workers.store(true, Ordering::Relaxed);
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

    if let Some(err) = first_error && !inv.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("{err}");
    }
    check_cancelled(inv.cancel)?;
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
