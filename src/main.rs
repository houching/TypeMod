pub mod app_state;
pub mod domain;
pub mod ui;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size,
};

use app_state::{AppState, SystemFontState};
use domain::builder::{
    BuildProgressEvent, has_built_fonts, resolve_trusted_fontforge, run_build_and_apply,
};
use domain::elevation::is_admin;
use domain::registry::{are_clones_installed, check_system_state};
use domain::resizer::get_current_metrics;
use ui::shell::ShellView;

fn parse_args() -> anyhow::Result<(Option<PathBuf>, bool)> {
    let mut state_arg: Option<PathBuf> = None;
    let mut cli_build = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let s = arg.to_string_lossy();
        if s == "--state" {
            let Some(val) = args.next() else {
                anyhow::bail!("Missing path value after --state");
            };
            let val_str = val.to_string_lossy();
            if val_str.is_empty() || val_str.starts_with("--") {
                anyhow::bail!("Invalid path value after --state: {val_str}");
            }
            state_arg = Some(PathBuf::from(val));
        } else if let Some(val) = s.strip_prefix("--state=") {
            if val.is_empty() || val.starts_with("--") {
                anyhow::bail!("Invalid path value in --state=: {val}");
            }
            state_arg = Some(PathBuf::from(val));
        } else if s == "--cli-build" {
            cli_build = true;
        } else {
            anyhow::bail!("Unrecognized argument: {s}");
        }
    }
    Ok((state_arg, cli_build))
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

fn main() {
    let (state_arg, cli_build) = match parse_args() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Argument error: {e}");
            std::process::exit(2);
        }
    };
    if cli_build {
        if let Err(e) = run_cli_build(state_arg) {
            eprintln!("CLI build failed: {e}");
            std::process::exit(1);
        }
        return;
    }

    Application::new()
        .with_assets(ui::icons::EmbeddedAssets)
        .run(move |cx: &mut App| {
        let state = cx.new(|_| {
            let mut s = AppState::new(state_arg.as_deref());
            if let Some(ref temp_path) = state_arg
                && is_valid_temp_state_file(temp_path)
            {
                let _ = std::fs::remove_file(temp_path);
            }
            s.is_admin = is_admin();
            s.system_state = check_system_state();
            s.clones_installed = are_clones_installed();
            s.has_built_fonts = has_built_fonts();
            s.fontforge_path =
                resolve_trusted_fontforge(s.config.custom_fontforge_path.as_deref());
            if let Ok(metrics) = get_current_metrics() {
                s.uniform_pt = metrics.message;
                s.sizes = metrics;
            }
            s
        });

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
