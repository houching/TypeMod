pub mod app_state;
pub mod domain;
pub mod ui;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use gpui::{
    App, Application, Bounds, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size,
};

use app_state::{AppState, SystemFontState};
use domain::builder::{BuildProgressEvent, find_fontforge, run_build_and_apply};
use domain::elevation::is_admin;
use domain::registry::{are_clones_installed, check_system_state};
use domain::resizer::get_current_metrics;
use ui::shell::ShellView;

fn parse_args() -> (Option<PathBuf>, bool) {
    let mut state_arg = None;
    let mut cli_build = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--state" {
            state_arg = args.next().map(PathBuf::from);
        } else if arg == "--cli-build" {
            cli_build = true;
        }
    }
    (state_arg, cli_build)
}

fn run_cli_build(state_arg: Option<PathBuf>) -> anyhow::Result<()> {
    if !is_admin() {
        anyhow::bail!("--cli-build requires Administrator privileges");
    }
    let state = AppState::new(state_arg.as_deref());
    if !state.config.latin.has_regular() {
        anyhow::bail!("No Primary English / Latin Regular font configured");
    }
    let sys_state = check_system_state();
    if sys_state != SystemFontState::Original {
        anyhow::bail!("System state is {:?}; restore and reboot first", sys_state);
    }
    let ff_exe = find_fontforge().ok_or_else(|| anyhow::anyhow!("FontForge ffpython.exe not found"))?;
    let cancel = AtomicBool::new(false);
    run_build_and_apply(&state.config, &ff_exe, &cancel, |ev| match ev {
        BuildProgressEvent::Step { progress, status } => {
            println!("[{:.0}%] {}", progress * 100.0, status);
        }
        BuildProgressEvent::Log(line) => println!("{line}"),
    })
}

fn main() {
    let (state_arg, cli_build) = parse_args();
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
            s.is_admin = is_admin();
            s.system_state = check_system_state();
            s.clones_installed = are_clones_installed();
            s.fontforge_path = s
                .config
                .custom_fontforge_path
                .clone()
                .filter(|p| p.exists())
                .or_else(find_fontforge);
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
                    title: Some(format!("SegoeUI Changer v{}", env!("CARGO_PKG_VERSION")).into()),
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
