use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, HANDLE, HWND};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY,
    TokenPrimary,
};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessWithTokenW, LOGON_WITH_PROFILE, OpenProcess,
    OpenProcessToken, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{IsUserAnAdmin, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::{
    GetShellWindow, GetWindowThreadProcessId, SW_SHOWNORMAL,
};
use windows::core::{PCWSTR, PWSTR, w};

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

fn to_wide_null(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Escapes a single argument following Windows `CommandLineToArgvW` rules.
pub fn quote_windows_arg(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for ch in arg.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..(backslashes * 2 + 1) {
                    out.push('\\');
                }
                out.push('"');
                backslashes = 0;
            }
            _ => {
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push(ch);
            }
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
    out
}

/// Relaunches the current executable with UAC elevation (`runas`), passing `--state <path>`.
/// Returns `Ok(true)` if accepted, `Ok(false)` if cancelled by user (`ERROR_CANCELLED`).
pub fn relaunch_elevated(state_path: &Path) -> anyhow::Result<bool> {
    let state_str = state_path.to_string_lossy();
    if state_str.chars().any(|c| c.is_control()) {
        anyhow::bail!("Invalid control character in temporary state path");
    }

    let exe = std::env::current_exe()?;
    let exe_w = to_wide_null(exe.as_os_str());
    let params = format!("--state {}", quote_windows_arg(&state_str));
    let params_w = to_wide_null(OsStr::new(&params));

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    match unsafe { ShellExecuteExW(&mut sei) } {
        Ok(()) => Ok(true),
        Err(e) if e.code() == ERROR_CANCELLED.into() => Ok(false),
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}

/// Opens a folder in Windows Explorer, de-elevating via the desktop shell token when running as Admin.
pub fn open_in_explorer(dir: &Path) {
    let explorer = crate::domain::registry::explorer_exe();
    if !is_admin() {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new(&explorer)
            .arg(dir)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        return;
    }

    if let Some(tok) = capture_desktop_shell_token()
        && unsafe { launch_with_token(&tok, &explorer, Some(dir)) }.is_ok()
    {
        return;
    }

    let _ = std::process::Command::new(&explorer).arg(dir).spawn();
}

pub struct ShellToken(HANDLE);

impl Drop for ShellToken {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// Captures a primary medium-integrity token from the current desktop shell (`explorer.exe`).
pub fn capture_desktop_shell_token() -> Option<ShellToken> {
    let shell_hwnd: HWND = unsafe { GetShellWindow() };
    if shell_hwnd.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(shell_hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    let h_proc = ShellToken(unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?
    });
    let mut raw_tok = HANDLE::default();
    unsafe {
        OpenProcessToken(h_proc.0, TOKEN_DUPLICATE | TOKEN_QUERY, &mut raw_tok).ok()?;
    }
    let h_tok = ShellToken(raw_tok);

    let mut raw_primary = HANDLE::default();
    unsafe {
        DuplicateTokenEx(
            h_tok.0,
            TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut raw_primary,
        )
        .ok()?;
    }
    Some(ShellToken(raw_primary))
}

/// Launches `explorer.exe` using a previously captured desktop shell token (for FontCache restart).
pub fn launch_shell_explorer(token: Option<&ShellToken>) {
    let explorer = crate::domain::registry::explorer_exe();
    if let Some(tok) = token
        && unsafe { launch_with_token(tok, &explorer, None) }.is_ok()
    {
        return;
    }
    if !is_admin() {
        let _ = std::process::Command::new(&explorer).spawn();
    }
}

unsafe fn launch_with_token(
    tok: &ShellToken,
    exe: &Path,
    arg_path: Option<&Path>,
) -> anyhow::Result<()> {
    let app_w = to_wide_null(exe.as_os_str());
    let mut cmd_w = arg_path.map(|p| {
        let cmd = format!(
            "{} {}",
            quote_windows_arg(&exe.to_string_lossy()),
            quote_windows_arg(&p.to_string_lossy())
        );
        to_wide_null(OsStr::new(&cmd))
    });
    let si = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut pi = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessWithTokenW(
            tok.0,
            LOGON_WITH_PROFILE,
            PCWSTR(app_w.as_ptr()),
            cmd_w.as_mut().map(|v| PWSTR(v.as_mut_ptr())),
            CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )?;
        let _ = CloseHandle(pi.hProcess);
        let _ = CloseHandle(pi.hThread);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::quote_windows_arg;

    #[test]
    fn quotes_windows_cmdline_args_safely() {
        assert_eq!(
            quote_windows_arg(r#"C:\Users\Test\temp_state_1.json"#),
            r#""C:\Users\Test\temp_state_1.json""#
        );
        assert_eq!(
            quote_windows_arg(r#"C:\Dir With Spaces\a\"b\"#),
            r#""C:\Dir With Spaces\a\\\"b\\""#
        );
    }
}
