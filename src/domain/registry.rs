use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
    RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::core::{PCWSTR, PWSTR, w};

use crate::app_state::SystemFontState;

const FONTS_REG_PATH: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts");
const FONT_SUBSTITUTES_REG_PATH: PCWSTR =
    w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontSubstitutes");

#[derive(Clone, Copy, Debug)]
pub struct SegoeFontSpec {
    pub weight_name: &'static str,
    pub stock_file: &'static str,
    pub mod_file: &'static str,
    pub clone_file: &'static str,
    pub reg_key: &'static str,
    pub clone_reg_key: &'static str,
    pub is_italic: bool,
    pub is_variable: bool,
}

/// Canonical Single Source of Truth for all 13 Windows Segoe UI font weights & registry mappings.
pub const SEGOE_FONT_SPECS: &[SegoeFontSpec] = &[
    SegoeFontSpec {
        weight_name: "Light",
        stock_file: "segoeuil.ttf",
        mod_file: "segoeuil_system_mod.ttf",
        clone_file: "clone_segoeuil.ttf",
        reg_key: "Segoe UI Light (TrueType)",
        clone_reg_key: "Segoe UI Clone Light (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Semilight",
        stock_file: "segoeuisl.ttf",
        mod_file: "segoeuisl_system_mod.ttf",
        clone_file: "clone_segoeuisl.ttf",
        reg_key: "Segoe UI Semilight (TrueType)",
        clone_reg_key: "Segoe UI Clone Semilight (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Regular",
        stock_file: "segoeui.ttf",
        mod_file: "segoeui_system_mod.ttf",
        clone_file: "clone_segoeui.ttf",
        reg_key: "Segoe UI (TrueType)",
        clone_reg_key: "Segoe UI Clone (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Semibold",
        stock_file: "seguisb.ttf",
        mod_file: "seguisb_system_mod.ttf",
        clone_file: "clone_seguisb.ttf",
        reg_key: "Segoe UI Semibold (TrueType)",
        clone_reg_key: "Segoe UI Clone Semibold (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Bold",
        stock_file: "segoeuib.ttf",
        mod_file: "segoeuib_system_mod.ttf",
        clone_file: "clone_segoeuib.ttf",
        reg_key: "Segoe UI Bold (TrueType)",
        clone_reg_key: "Segoe UI Clone Bold (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Black",
        stock_file: "seguibl.ttf",
        mod_file: "seguibl_system_mod.ttf",
        clone_file: "clone_seguibl.ttf",
        reg_key: "Segoe UI Black (TrueType)",
        clone_reg_key: "Segoe UI Clone Black (TrueType)",
        is_italic: false,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Variable",
        stock_file: "SegUIVar.ttf",
        mod_file: "SegUIVar_system_mod.ttf",
        clone_file: "",
        reg_key: "Segoe UI Variable (TrueType)",
        clone_reg_key: "",
        is_italic: false,
        is_variable: true,
    },
    SegoeFontSpec {
        weight_name: "Light Italic",
        stock_file: "seguili.ttf",
        mod_file: "seguili_system_mod.ttf",
        clone_file: "clone_seguili.ttf",
        reg_key: "Segoe UI Light Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Light Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Semilight Italic",
        stock_file: "seguisli.ttf",
        mod_file: "seguisli_system_mod.ttf",
        clone_file: "clone_seguisli.ttf",
        reg_key: "Segoe UI Semilight Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Semilight Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Italic",
        stock_file: "segoeuii.ttf",
        mod_file: "segoeuii_system_mod.ttf",
        clone_file: "clone_segoeuii.ttf",
        reg_key: "Segoe UI Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Semibold Italic",
        stock_file: "seguisbi.ttf",
        mod_file: "seguisbi_system_mod.ttf",
        clone_file: "clone_seguisbi.ttf",
        reg_key: "Segoe UI Semibold Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Semibold Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Bold Italic",
        stock_file: "segoeuiz.ttf",
        mod_file: "segoeuiz_system_mod.ttf",
        clone_file: "clone_segoeuiz.ttf",
        reg_key: "Segoe UI Bold Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Bold Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
    SegoeFontSpec {
        weight_name: "Black Italic",
        stock_file: "seguibli.ttf",
        mod_file: "seguibli_system_mod.ttf",
        clone_file: "clone_seguibli.ttf",
        reg_key: "Segoe UI Black Italic (TrueType)",
        clone_reg_key: "Segoe UI Clone Black Italic (TrueType)",
        is_italic: true,
        is_variable: false,
    },
];

pub fn windows_fonts_dir() -> PathBuf {
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windir.join("Fonts")
}

fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn list_installed_fonts() -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let fonts_dir = windows_fonts_dir();

    for root in [HKEY_LOCAL_MACHINE, windows::Win32::System::Registry::HKEY_CURRENT_USER] {
        let mut hkey = HKEY::default();
        let status = unsafe { RegOpenKeyExW(root, FONTS_REG_PATH, Some(0), KEY_READ, &mut hkey) };
        if status.is_err() {
            continue;
        }

        let mut idx = 0u32;
        let mut name_buf = vec![0u16; 512];
        let mut data_buf = vec![0u8; 1024];

        loop {
            let mut name_len = name_buf.len() as u32;
            let mut data_len = data_buf.len() as u32;
            let mut val_type = 0u32;

            let err = unsafe {
                RegEnumValueW(
                    hkey,
                    idx,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut val_type),
                    Some(data_buf.as_mut_ptr()),
                    Some(&mut data_len),
                )
            };

            if err == ERROR_NO_MORE_ITEMS {
                break;
            }
            idx += 1;

            if err.is_err() || val_type != REG_SZ.0 {
                continue;
            }

            let raw_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let u16_slice = unsafe {
                std::slice::from_raw_parts(
                    data_buf.as_ptr() as *const u16,
                    (data_len as usize) / 2,
                )
            };
            let raw_val = String::from_utf16_lossy(u16_slice)
                .trim_end_matches('\0')
                .to_string();

            let path = if Path::new(&raw_val).is_absolute() {
                PathBuf::from(&raw_val)
            } else {
                fonts_dir.join(&raw_val)
            };

            let is_outline = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"));

            if is_outline && path.exists() {
                let clean_name = clean_registry_font_name(&raw_name);
                if seen.insert((clean_name.to_lowercase(), path.clone())) {
                    result.push((clean_name, path));
                }
            }
        }

        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }

    result.sort_by_key(|a| a.0.to_lowercase());
    result
}

fn clean_registry_font_name(raw: &str) -> String {
    if let Some(pos) = raw.rfind(" (")
        && raw.ends_with(')')
    {
        return raw[..pos].trim().to_string();
    }
    raw.trim().to_string()
}

pub fn query_font_value(value_name: &str) -> Option<String> {
    let mut hkey = HKEY::default();
    let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_READ, &mut hkey) };
    if status.is_err() {
        return None;
    }

    let wide_name = to_wide_null(value_name);
    let mut data_buf = vec![0u8; 512];
    let mut data_len = data_buf.len() as u32;
    let mut val_type = REG_VALUE_TYPE::default();

    let err = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut val_type),
            Some(data_buf.as_mut_ptr()),
            Some(&mut data_len),
        )
    };

    unsafe {
        let _ = RegCloseKey(hkey);
    }

    if err.is_err() || val_type != REG_SZ {
        return None;
    }

    let u16_slice = unsafe {
        std::slice::from_raw_parts(data_buf.as_ptr() as *const u16, (data_len as usize) / 2)
    };
    Some(
        String::from_utf16_lossy(u16_slice)
            .trim_end_matches('\0')
            .to_string(),
    )
}

pub fn check_system_state() -> SystemFontState {
    let is_mod = ["Segoe UI (TrueType)", "Segoe UI Variable (TrueType)"]
        .into_iter()
        .any(|k| {
            query_font_value(k)
                .is_some_and(|v| v.to_ascii_lowercase().contains("system_mod"))
        });

    if is_mod {
        return SystemFontState::Modded;
    }

    if !crate::domain::elevation::is_admin() {
        return SystemFontState::Original;
    }
    if cleanup_stale_mod_files() {
        SystemFontState::Original
    } else {
        SystemFontState::PendingRestoreReboot
    }
}

/// Removes leftover `*_system_mod*.ttf` files from `%WINDIR%\Fonts` that are not
/// currently referenced by `HKLM\...\Fonts`. Returns `true` if no unreferenced mod files remain.
pub fn cleanup_stale_mod_files() -> bool {
    let active: std::collections::HashSet<String> = SEGOE_FONT_SPECS
        .iter()
        .filter_map(|s| query_font_value(s.reg_key))
        .map(|v| v.to_ascii_lowercase())
        .collect();

    let Ok(entries) = fs::read_dir(windows_fonts_dir()) else {
        return true;
    };
    let mut all_clean = true;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.contains("_system_mod")
            && name.ends_with(".ttf")
            && !active.contains(&name)
            && fs::remove_file(entry.path()).is_err()
        {
            all_clean = false;
        }
    }
    all_clean
}

pub fn are_clones_installed() -> bool {
    if SEGOE_FONT_SPECS
        .iter()
        .filter(|s| !s.clone_reg_key.is_empty())
        .any(|s| query_font_value(s.clone_reg_key).is_some())
    {
        return true;
    }
    if let Ok(entries) = fs::read_dir(windows_fonts_dir()) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("clone_segoe")
            {
                return true;
            }
        }
    }
    false
}

pub fn clear_segoe_font_substitutes() {
    let mut hkey = HKEY::default();
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            FONT_SUBSTITUTES_REG_PATH,
            Some(0),
            KEY_WRITE,
            &mut hkey,
        )
    }
    .is_ok()
    {
        let val = to_wide_null("Segoe UI");
        unsafe {
            let _ = RegDeleteValueW(hkey, PCWSTR(val.as_ptr()));
            let _ = RegCloseKey(hkey);
        }
    }
}

pub fn set_font_registry_entries(entries: &[(&str, &str)]) -> anyhow::Result<()> {
    clear_segoe_font_substitutes();
    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_WRITE, &mut hkey).ok()?;
    }

    let res = (|| -> anyhow::Result<()> {
        for (name, file) in entries {
            let name_w = to_wide_null(name);
            let file_w = to_wide_null(file);
            let bytes = unsafe {
                std::slice::from_raw_parts(file_w.as_ptr() as *const u8, file_w.len() * 2)
            };
            unsafe {
                RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), Some(0), REG_SZ, Some(bytes))
                    .ok()?;
            }
        }
        Ok(())
    })();

    unsafe {
        let _ = RegCloseKey(hkey);
    }
    res
}

pub fn restore_original_fonts(delete_clones: bool) -> anyhow::Result<()> {
    let stock_entries: Vec<(&str, &str)> = SEGOE_FONT_SPECS
        .iter()
        .map(|s| (s.reg_key, s.stock_file))
        .collect();
    set_font_registry_entries(&stock_entries)?;

    if delete_clones {
        let mut hkey = HKEY::default();
        if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_WRITE, &mut hkey) }
            .is_ok()
        {
            for spec in SEGOE_FONT_SPECS.iter().filter(|s| !s.clone_reg_key.is_empty()) {
                let k_w = to_wide_null(spec.clone_reg_key);
                unsafe {
                    let _ = RegDeleteValueW(hkey, PCWSTR(k_w.as_ptr()));
                }
            }
            unsafe {
                let _ = RegCloseKey(hkey);
            }
        }

        if let Ok(entries) = fs::read_dir(windows_fonts_dir()) {
            for entry in entries.flatten() {
                if entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .starts_with("clone_segoe")
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }

    Ok(())
}

/// Best-effort live refresh without reboot: updates GDI font resources, purges Windows FontCache,
/// broadcasts `WM_FONTCHANGE`, and restarts `explorer.exe`.
pub fn live_refresh_font_cache() {
    use std::os::windows::process::CommandExt;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{AddFontResourceW, RemoveFontResourceW};
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_FONTCHANGE,
    };

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let fonts_dir = windows_fonts_dir();

    // Remove modded fonts or re-add stock fonts in current GDI session
    let is_modded = check_system_state() == SystemFontState::Modded;
    for spec in SEGOE_FONT_SPECS {
        let active_mod = query_font_value(spec.reg_key)
            .filter(|v| v.to_ascii_lowercase().contains("system_mod"))
            .unwrap_or_else(|| spec.mod_file.to_string());
        let mod_path = to_wide_null(&fonts_dir.join(&active_mod).to_string_lossy());
        let stock_path = to_wide_null(&fonts_dir.join(spec.stock_file).to_string_lossy());
        unsafe {
            if is_modded {
                let _ = RemoveFontResourceW(PCWSTR(stock_path.as_ptr()));
                let _ = AddFontResourceW(PCWSTR(mod_path.as_ptr()));
            } else {
                let _ = RemoveFontResourceW(PCWSTR(mod_path.as_ptr()));
                let _ = AddFontResourceW(PCWSTR(stock_path.as_ptr()));
            }
        }
    }

    let ps = r#"
Stop-Service -Name FontCache -Force -ErrorAction SilentlyContinue
Remove-Item "$env:WINDIR\ServiceProfiles\LocalService\AppData\Local\FontCache\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "$env:WINDIR\System32\FNTCACHE.DAT" -Force -ErrorAction SilentlyContinue
Start-Service -Name FontCache -ErrorAction SilentlyContinue
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 400
if (-not (Get-Process -Name explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
"#;
    let _ = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    let _ = crate::domain::resizer::set_system_font_face("Segoe UI");

    unsafe {
        let mut res = 0usize;
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_FONTCHANGE,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            3000,
            Some(&mut res),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_installed_windows_fonts_and_system_state() {
        let fonts = list_installed_fonts();
        assert!(!fonts.is_empty(), "expected installed Windows fonts in HKLM");
        let _state = check_system_state();
        let _clones = are_clones_installed();
    }

    #[test]
    fn reads_live_win32_metrics_and_detects_segoe_weights() {
        let metrics = crate::domain::resizer::get_current_metrics()
            .expect("SystemParametersInfoW should succeed");
        assert!(metrics.message > 0 && metrics.icon > 0);

        let segoe = windows_fonts_dir().join("segoeui.ttf");
        if segoe.exists() {
            let fonts = list_installed_fonts();
            let detected = crate::domain::font_discovery::auto_detect_weights(
                &segoe,
                Some("Segoe UI"),
                &fonts,
            );
            assert!(detected.has_regular());
            assert!(detected.bold.is_some());
        }
    }
}
