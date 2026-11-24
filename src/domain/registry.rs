use std::collections::HashSet;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use read_fonts::FontRef;
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    RegCloseKey, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};
use windows::core::{PCWSTR, PWSTR, w};

use crate::app_state::SystemFontState;

const MAX_FONT_BYTES: usize = 64 * 1024 * 1024;

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

pub fn windows_dir() -> PathBuf {
    let mut buf = vec![0u16; 260];
    let len = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 {
        return PathBuf::from(r"C:\Windows");
    }
    if len > buf.len() {
        buf.resize(len, 0);
        let len2 = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
        if len2 == 0 || len2 > buf.len() {
            return PathBuf::from(r"C:\Windows");
        }
        return PathBuf::from(String::from_utf16_lossy(&buf[..len2]));
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..len]))
}

pub fn system32_dir() -> PathBuf {
    let mut buf = vec![0u16; 260];
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 {
        return windows_dir().join("System32");
    }
    if len > buf.len() {
        buf.resize(len, 0);
        let len2 = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
        if len2 == 0 || len2 > buf.len() {
            return windows_dir().join("System32");
        }
        return PathBuf::from(String::from_utf16_lossy(&buf[..len2]));
    }
    PathBuf::from(String::from_utf16_lossy(&buf[..len]))
}

pub fn windows_fonts_dir() -> PathBuf {
    windows_dir().join("Fonts")
}

pub fn powershell_exe() -> PathBuf {
    system32_dir().join(r"WindowsPowerShell\v1.0\powershell.exe")
}

pub fn shutdown_exe() -> PathBuf {
    system32_dir().join("shutdown.exe")
}

pub fn explorer_exe() -> PathBuf {
    windows_dir().join("explorer.exe")
}

fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn expand_env_string(raw: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }
    let src_w = to_wide_null(raw);
    let needed = unsafe { ExpandEnvironmentStringsW(PCWSTR(src_w.as_ptr()), None) } as usize;
    if needed == 0 {
        return raw.to_string();
    }
    let mut dst = vec![0u16; needed];
    let written = unsafe { ExpandEnvironmentStringsW(PCWSTR(src_w.as_ptr()), Some(&mut dst)) } as usize;
    if written == 0 || written > dst.len() {
        return raw.to_string();
    }
    String::from_utf16_lossy(&dst[..written])
        .trim_end_matches('\0')
        .to_string()
}

fn decode_reg_string(buf: &[u16], byte_len: u32, val_type: u32) -> String {
    let u16_len = ((byte_len as usize) / 2).min(buf.len());
    let raw = String::from_utf16_lossy(&buf[..u16_len])
        .trim_end_matches('\0')
        .to_string();
    if val_type == REG_EXPAND_SZ.0 {
        expand_env_string(&raw)
    } else {
        raw
    }
}

pub fn list_installed_fonts() -> Vec<(String, PathBuf)> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let fonts_dir = windows_fonts_dir();

    for root in [HKEY_LOCAL_MACHINE, windows::Win32::System::Registry::HKEY_CURRENT_USER] {
        let mut hkey = HKEY::default();
        let status = unsafe { RegOpenKeyExW(root, FONTS_REG_PATH, Some(0), KEY_READ, &mut hkey) };
        if status.is_err() {
            continue;
        }

        let mut idx = 0u32;
        let mut name_buf = vec![0u16; 512];
        let mut data_buf = vec![0u16; 512];

        loop {
            let mut name_len = name_buf.len() as u32;
            let mut data_bytes = (data_buf.len() * 2) as u32;
            let mut val_type = 0u32;

            let err = unsafe {
                RegEnumValueW(
                    hkey,
                    idx,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut val_type),
                    Some(data_buf.as_mut_ptr() as *mut u8),
                    Some(&mut data_bytes),
                )
            };

            if err == ERROR_NO_MORE_ITEMS {
                break;
            }
            if err == ERROR_MORE_DATA && name_buf.len() < 8192 && data_buf.len() < 8192 {
                name_buf.resize(name_buf.len() * 2, 0);
                data_buf.resize(data_buf.len() * 2, 0);
                continue;
            }
            idx += 1;

            if err.is_err() || (val_type != REG_SZ.0 && val_type != REG_EXPAND_SZ.0) {
                continue;
            }

            let raw_name = String::from_utf16_lossy(&name_buf[..(name_len as usize).min(name_buf.len())]);
            let raw_val = decode_reg_string(&data_buf, data_bytes, val_type);

            let path = if Path::new(&raw_val).is_absolute() {
                PathBuf::from(&raw_val)
            } else {
                fonts_dir.join(&raw_val)
            };

            let is_outline = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf"));

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
    query_font_value_raw(value_name).map(|(v, _)| v)
}

fn query_font_value_raw(value_name: &str) -> Option<(String, REG_VALUE_TYPE)> {
    let mut hkey = HKEY::default();
    let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_READ, &mut hkey) };
    if status.is_err() {
        return None;
    }

    let wide_name = to_wide_null(value_name);
    let mut data_buf = vec![0u16; 256];
    let mut data_bytes = (data_buf.len() * 2) as u32;
    let mut val_type = REG_VALUE_TYPE::default();

    let mut err = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(wide_name.as_ptr()),
            None,
            Some(&mut val_type),
            Some(data_buf.as_mut_ptr() as *mut u8),
            Some(&mut data_bytes),
        )
    };
    if err == ERROR_MORE_DATA && (data_bytes as usize) > data_buf.len() * 2 && data_bytes <= 16384 {
        data_buf.resize((data_bytes as usize).div_ceil(2), 0);
        data_bytes = (data_buf.len() * 2) as u32;
        err = unsafe {
            RegQueryValueExW(
                hkey,
                PCWSTR(wide_name.as_ptr()),
                None,
                Some(&mut val_type),
                Some(data_buf.as_mut_ptr() as *mut u8),
                Some(&mut data_bytes),
            )
        };
    }

    unsafe {
        let _ = RegCloseKey(hkey);
    }

    if err.is_err() || (val_type != REG_SZ && val_type != REG_EXPAND_SZ) {
        return None;
    }

    Some((decode_reg_string(&data_buf, data_bytes, val_type.0), val_type))
}

pub fn check_system_state() -> SystemFontState {
    let is_mod = SEGOE_FONT_SPECS.iter().any(|spec| {
        query_font_value(spec.reg_key)
            .is_some_and(|v| v.to_ascii_lowercase().contains("system_mod"))
    });

    if is_mod {
        return SystemFontState::Modded;
    }

    if has_unreferenced_mod_files() {
        SystemFontState::PendingRestoreReboot
    } else {
        SystemFontState::Original
    }
}

fn active_mod_filenames() -> HashSet<String> {
    SEGOE_FONT_SPECS
        .iter()
        .filter_map(|s| query_font_value(s.reg_key))
        .map(|v| {
            Path::new(&v)
                .file_name()
                .map(|f| f.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_else(|| v.to_ascii_lowercase())
        })
        .collect()
}

fn has_unreferenced_mod_files() -> bool {
    let active = active_mod_filenames();

    let Ok(entries) = fs::read_dir(windows_fonts_dir()) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        name.contains("_system_mod") && name.ends_with(".ttf") && !active.contains(&name)
    })
}

/// Removes leftover `*_system_mod*.ttf` files from `%WINDIR%\Fonts` that are not
/// currently referenced by `HKLM\...\Fonts`. Returns `true` if no unreferenced mod files remain.
pub fn cleanup_stale_mod_files() -> bool {
    let active = active_mod_filenames();

    let Ok(entries) = fs::read_dir(windows_fonts_dir()) else {
        return false;
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
    let previous: Vec<(&str, Option<(String, REG_VALUE_TYPE)>)> = entries
        .iter()
        .map(|(name, _)| (*name, query_font_value_raw(name)))
        .collect();

    clear_segoe_font_substitutes();
    let mut hkey = HKEY::default();
    unsafe {
        RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_WRITE, &mut hkey).ok()?;
    }

    let mut written_count = 0usize;
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
            written_count += 1;
        }
        Ok(())
    })();

    if res.is_err() && written_count > 0 {
        for (name, prev_opt) in previous.iter().take(written_count).rev() {
            let name_w = to_wide_null(name);
            match prev_opt {
                Some((prev_val, prev_type)) => {
                    let prev_w = to_wide_null(prev_val);
                    let bytes = unsafe {
                        std::slice::from_raw_parts(prev_w.as_ptr() as *const u8, prev_w.len() * 2)
                    };
                    unsafe {
                        let _ = RegSetValueExW(
                            hkey,
                            PCWSTR(name_w.as_ptr()),
                            Some(0),
                            *prev_type,
                            Some(bytes),
                        );
                    }
                }
                None => unsafe {
                    let _ = RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr()));
                },
            }
        }
    }

    unsafe {
        let _ = RegCloseKey(hkey);
    }
    res
}

fn delete_font_registry_values(keys: &[&str]) {
    if keys.is_empty() {
        return;
    }
    let mut hkey = HKEY::default();
    if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, FONTS_REG_PATH, Some(0), KEY_WRITE, &mut hkey) }
        .is_ok()
    {
        for k in keys {
            let k_w = to_wide_null(k);
            unsafe {
                let _ = RegDeleteValueW(hkey, PCWSTR(k_w.as_ptr()));
            }
        }
        unsafe {
            let _ = RegCloseKey(hkey);
        }
    }
}

/// Validates all replacement font buffers, writes uniquely tagged copies into `%WINDIR%\Fonts`,
/// commits `HKLM\...\Fonts`, cleans stale mod files, and sets `NONCLIENTMETRICSW` face name.
pub fn install_mod_fonts_from_bytes(items: &[(&SegoeFontSpec, Vec<u8>)]) -> anyhow::Result<usize> {
    if !items.iter().any(|(spec, _)| spec.mod_file == "segoeui_system_mod.ttf") {
        anyhow::bail!("Missing base replacement font segoeui_system_mod.ttf");
    }
    for (spec, bytes) in items {
        if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES || FontRef::new(bytes).is_err() {
            anyhow::bail!("Generated or imported font '{}' is invalid or corrupt", spec.mod_file);
        }
    }

    let fonts_dir = windows_fonts_dir();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1);
    let tag = format!("{ms}_{}", std::process::id());
    let mut reg_updates: Vec<(&str, String)> = Vec::with_capacity(items.len());
    let mut copied_paths: Vec<PathBuf> = Vec::with_capacity(items.len());

    for (spec, bytes) in items {
        let tagged = spec.mod_file.replace(".ttf", &format!("_{tag}.ttf"));
        let dst = fonts_dir.join(&tagged);
        if let Err(e) = fs::write(&dst, bytes) {
            for p in &copied_paths {
                let _ = fs::remove_file(p);
            }
            anyhow::bail!("Failed to install {} to {}: {e}", tagged, dst.display());
        }
        copied_paths.push(dst);
        reg_updates.push((spec.reg_key, tagged));
    }

    let reg_refs: Vec<(&str, &str)> = reg_updates
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    if let Err(e) = set_font_registry_entries(&reg_refs) {
        for p in &copied_paths {
            let _ = fs::remove_file(p);
        }
        return Err(e);
    }

    let _ = cleanup_stale_mod_files();
    let _ = crate::domain::resizer::set_system_font_face("Segoe UI");
    Ok(reg_updates.len())
}

fn is_valid_font_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_FONT_BYTES as u64 {
        return false;
    }
    fs::read(path).is_ok_and(|b| FontRef::new(&b).is_ok())
}

pub fn restore_original_fonts(delete_clones: bool) -> anyhow::Result<()> {
    let fonts_dir = windows_fonts_dir();
    let bak_dir = crate::domain::backup_cloner::backup_dir();
    let mut stock_entries: Vec<(&str, &str)> = Vec::new();
    let mut keys_to_delete: Vec<&str> = Vec::new();

    for spec in SEGOE_FONT_SPECS {
        let live_path = fonts_dir.join(spec.stock_file);
        let bak_path = bak_dir.join(spec.stock_file);

        if !is_valid_font_file(&live_path) && is_valid_font_file(&bak_path) {
            fs::copy(&bak_path, &live_path)?;
        }

        if is_valid_font_file(&live_path) {
            stock_entries.push((spec.reg_key, spec.stock_file));
        } else if spec.is_variable {
            // Windows 10 lacks SegUIVar.ttf; remove any dangling registry entry instead of pointing to missing file
            keys_to_delete.push(spec.reg_key);
        } else if spec.stock_file == "segoeui.ttf" {
            anyhow::bail!(
                "Stock {} is missing or corrupt in both {} and {}",
                spec.stock_file,
                fonts_dir.display(),
                bak_dir.display()
            );
        }
    }

    set_font_registry_entries(&stock_entries)?;
    delete_font_registry_values(&keys_to_delete);

    if delete_clones {
        let clone_keys: Vec<&str> = SEGOE_FONT_SPECS
            .iter()
            .filter(|s| !s.clone_reg_key.is_empty())
            .map(|s| s.clone_reg_key)
            .collect();
        delete_font_registry_values(&clone_keys);

        if let Ok(entries) = fs::read_dir(&fonts_dir) {
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

    let face = if fonts_dir.join("SegUIVar.ttf").exists() {
        "Segoe UI Variable Text"
    } else {
        "Segoe UI"
    };
    let _ = crate::domain::resizer::set_system_font_face(face);

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
    let windir = windows_dir();
    let fonts_dir = windows_fonts_dir();
    let shell_tok = crate::domain::elevation::capture_desktop_shell_token();

    // Remove modded fonts or re-add stock fonts in current GDI session
    let is_modded = check_system_state() == SystemFontState::Modded;
    if !is_modded && let Ok(entries) = fs::read_dir(&fonts_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.contains("_system_mod") && name.ends_with(".ttf") {
                let path_w = to_wide_null(&entry.path().to_string_lossy());
                unsafe {
                    let _ = RemoveFontResourceW(PCWSTR(path_w.as_ptr()));
                }
            }
        }
    }
    for spec in SEGOE_FONT_SPECS {
        let stock_path = to_wide_null(&fonts_dir.join(spec.stock_file).to_string_lossy());
        unsafe {
            if is_modded {
                let active_mod = query_font_value(spec.reg_key)
                    .filter(|v| v.to_ascii_lowercase().contains("system_mod"))
                    .unwrap_or_else(|| spec.mod_file.to_string());
                let mod_path = to_wide_null(&fonts_dir.join(&active_mod).to_string_lossy());
                let _ = RemoveFontResourceW(PCWSTR(stock_path.as_ptr()));
                let _ = AddFontResourceW(PCWSTR(mod_path.as_ptr()));
            } else {
                let _ = AddFontResourceW(PCWSTR(stock_path.as_ptr()));
            }
        }
    }

    let ps = r#"
Stop-Service -Name FontCache -Force -ErrorAction SilentlyContinue
Remove-Item "$env:SEGOEUI_WINDIR\ServiceProfiles\LocalService\AppData\Local\FontCache\*" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item "$env:SEGOEUI_WINDIR\System32\FNTCACHE.DAT" -Force -ErrorAction SilentlyContinue
Start-Service -Name FontCache -ErrorAction SilentlyContinue
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 400
"#;
    let _ = std::process::Command::new(powershell_exe())
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps])
        .env("SEGOEUI_WINDIR", &windir)
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    crate::domain::elevation::launch_shell_explorer(shell_tok.as_ref());

    let face = if is_modded || !fonts_dir.join("SegUIVar.ttf").exists() {
        "Segoe UI"
    } else {
        "Segoe UI Variable Text"
    };
    let _ = crate::domain::resizer::set_system_font_face(face);

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
