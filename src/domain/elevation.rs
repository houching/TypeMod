use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::Win32::UI::Shell::{IsUserAnAdmin, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Relaunches the current executable with UAC elevation (`runas`), passing `--state <path>`.
/// Returns `Ok(true)` if the elevation prompt was accepted and the new process launched.
pub fn relaunch_elevated(state_path: &Path) -> anyhow::Result<bool> {
    let exe = std::env::current_exe()?;
    let exe_w = to_wide_null(&exe.to_string_lossy());
    let params = format!("--state \"{}\"", state_path.to_string_lossy());
    let params_w = to_wide_null(&params);

    let hinst = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    Ok((hinst.0 as isize) > 32)
}
