#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod app_state;
pub mod domain;
pub mod ui;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size,
};

use app_state::{AppState, SystemFontState};
use domain::backup_cloner::import_and_apply_package;
use domain::builder::{
    BuildProgressEvent, apply_built_fonts_from_dir, built_fonts_dir, has_built_fonts,
    resolve_trusted_fontforge, run_build_and_apply,
};
use domain::elevation::is_admin;
use domain::registry::{are_clones_installed, check_system_state, restore_original_fonts};
use domain::resizer::get_current_metrics;
use ui::shell::ShellView;

enum HelperAction {
    Gui,
    CliBuild,
    ApplyBuilt { work_dir: Option<PathBuf> },
    Restore { delete_clones: bool },
    ImportZip { zip_path: PathBuf },
}

fn parse_flag_value(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    flag: &str,
) -> anyhow::Result<PathBuf> {
    let Some(val) = args.next() else {
        anyhow::bail!("Missing path value after {flag}");
    };
    let val_str = val.to_string_lossy();
    if val_str.is_empty() || val_str.starts_with("--") {
        anyhow::bail!("Invalid path value after {flag}: {val_str}");
    }
    Ok(PathBuf::from(val))
}

fn parse_args() -> anyhow::Result<(Option<PathBuf>, HelperAction)> {
    let mut state_arg: Option<PathBuf> = None;
    let mut action = HelperAction::Gui;
    let mut work_dir: Option<PathBuf> = None;
    let mut delete_clones = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let s = arg.to_string_lossy();
        if s == "--state" {
            state_arg = Some(parse_flag_value(&mut args, "--state")?);
        } else if let Some(val) = s.strip_prefix("--state=") {
            if val.is_empty() || val.starts_with("--") {
                anyhow::bail!("Invalid path value in --state=: {val}");
            }
            state_arg = Some(PathBuf::from(val));
        } else if s == "--work-dir" {
            work_dir = Some(parse_flag_value(&mut args, "--work-dir")?);
        } else if s == "--cli-build" {
            action = HelperAction::CliBuild;
        } else if s == "--apply-built" {
            action = HelperAction::ApplyBuilt { work_dir: None };
        } else if s == "--restore" {
            action = HelperAction::Restore {
                delete_clones: false,
            };
        } else if s == "--delete-clones" {
            delete_clones = true;
        } else if s == "--import-zip" {
            let zip_path = parse_flag_value(&mut args, "--import-zip")?;
            action = HelperAction::ImportZip { zip_path };
        } else {
            anyhow::bail!("Unrecognized argument: {s}");
        }
    }
    if let HelperAction::ApplyBuilt { work_dir: wd } = &mut action {
        *wd = work_dir;
    }
    if let HelperAction::Restore { delete_clones: dc } = &mut action {
        *dc = delete_clones;
    }
    Ok((state_arg, action))
}

fn is_valid_temp_state_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(digits) = name
        .strip_prefix("temp_state_")
        .and_then(|rest| rest.strip_suffix(".json"))
    else {
        return false;
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        return false;
    }
    let Ok(canon_parent) = path.parent().map(std::fs::canonicalize).transpose() else {
        return false;
    };
    let Some(canon_parent) = canon_parent else {
        return false;
    };
    let matches_temp = std::env::temp_dir()
        .canonicalize()
        .is_ok_and(|t| t == canon_parent);
    let matches_docs = AppState::app_docs_dir()
        .canonicalize()
        .is_ok_and(|d| d == canon_parent);
    matches_temp || matches_docs
}

fn run_cli_build(state_arg: Option<PathBuf>) -> anyhow::Result<()> {
    if !is_admin() {
        anyhow::bail!("--cli-build requires Administrator privileges");
    }
    let config = match state_arg.as_deref() {
        Some(p) => AppState::load_config_from(p)?,
        None => AppState::load_default_config().unwrap_or_default(),
    };
    let state = AppState::from_config(config);
    if !state.config.latin.has_regular() {
        anyhow::bail!("No Primary English / Latin Regular font configured");
    }
    let sys_state = check_system_state();
    if sys_state != SystemFontState::Original {
        anyhow::bail!("System state is {:?}; restore and reboot first", sys_state);
    }
    let ff_exe = resolve_trusted_fontforge(state.config.custom_fontforge_path.as_deref())
        .ok_or_else(|| anyhow::anyhow!("FontForge ffpython.exe not found"))?;
    let cancel = AtomicBool::new(false);
    run_build_and_apply(&state.config, &ff_exe, &cancel, |ev| match ev {
        BuildProgressEvent::Step { progress, status } => {
            println!("[{:.0}%] {}", progress * 100.0, status);
        }
        BuildProgressEvent::Log(line) => println!("{line}"),
    })
}

fn run_helper_action(state_arg: Option<PathBuf>, action: HelperAction) -> anyhow::Result<()> {
    if !is_admin() {
        anyhow::bail!("Elevated helper requires Administrator privileges");
    }
    match action {
        HelperAction::Gui => Ok(()),
        HelperAction::CliBuild => run_cli_build(state_arg),
        HelperAction::ApplyBuilt { work_dir } => {
            let config = match state_arg.as_deref() {
                Some(p) => AppState::load_config_from(p)?,
                None => AppState::load_default_config().unwrap_or_default(),
            };
            let dir = work_dir.unwrap_or_else(built_fonts_dir);
            apply_built_fonts_from_dir(&config, None, &dir, |_| {})?;
            Ok(())
        }
        HelperAction::Restore { delete_clones } => restore_original_fonts(delete_clones),
        HelperAction::ImportZip { zip_path } => {
            import_and_apply_package(&zip_path)?;
            Ok(())
        }
    }
}

fn main() {
    let (state_arg, action) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Argument error: {e}");
            std::process::exit(2);
        }
    };
    if !matches!(action, HelperAction::Gui) {
        if let Err(e) = run_helper_action(state_arg, action) {
            eprintln!("Operation failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    Application::new()
        .with_assets(ui::icons::EmbeddedAssets)
        .run(move |cx: &mut App| {
        let state = cx.new(|_| {
            let s = AppState::new(state_arg.as_deref());
            if let Some(ref temp_path) = state_arg
                && is_valid_temp_state_file(temp_path)
            {
                let _ = std::fs::remove_file(temp_path);
            }
            s
        });

        let custom_ff = state.read(cx).config.custom_fontforge_path.clone();
        let state_bg = state.clone();
        cx.spawn(async move |cx| {
            let (admin, sys_state, clones, built, ff_path, metrics) = cx
                .background_executor()
                .spawn(async move {
                    (
                        is_admin(),
                        check_system_state(),
                        are_clones_installed(),
                        has_built_fonts(),
                        resolve_trusted_fontforge(custom_ff.as_deref()),
                        get_current_metrics().ok(),
                    )
                })
                .await;
            let _ = state_bg.update(cx, |s, cx| {
                s.is_admin = admin;
                s.system_state = sys_state;
                s.clones_installed = clones;
                s.has_built_fonts = built;
                s.fontforge_path = ff_path;
                if let Some(m) = metrics {
                    s.uniform_pt = m.message;
                    s.sizes = m;
                }
                cx.notify();
            });
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1140.0), px(780.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("TypeMod v{}", env!("CARGO_PKG_VERSION")).into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| cx.new(|cx| ShellView::new(state.clone(), cx)),
        )
        .expect("failed to open main window");

        cx.activate(true);
    });
}
