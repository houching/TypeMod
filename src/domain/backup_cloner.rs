use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use crate::domain::registry::{
    SEGOE_FONT_SPECS, cleanup_stale_mod_files, set_font_registry_entries, windows_fonts_dir,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn backup_dir() -> PathBuf {
    crate::app_state::AppState::app_docs_dir().join("Original_Segoe_Backups")
}

/// Backs up stock Segoe UI files once into `Original_Segoe_Backups/`.
pub fn ensure_backup() -> anyhow::Result<PathBuf> {
    let dir = backup_dir();
    fs::create_dir_all(&dir)?;
    let fonts_dir = windows_fonts_dir();

    for spec in SEGOE_FONT_SPECS {
        let src = fonts_dir.join(spec.stock_file);
        let dst = dir.join(spec.stock_file);
        if src.exists() && !dst.exists() {
            fs::copy(&src, &dst)?;
        }
    }

    Ok(dir)
}

pub fn backup_status() -> (usize, usize) {
    let dir = backup_dir();
    let count = SEGOE_FONT_SPECS
        .iter()
        .filter(|s| dir.join(s.stock_file).exists())
        .count();
    (count, SEGOE_FONT_SPECS.len())
}

pub fn export_built_fonts(dest_dir: &Path) -> anyhow::Result<usize> {
    let work_dir = crate::app_state::AppState::app_docs_dir().join("work");
    if !work_dir.exists() {
        anyhow::bail!("No built fonts found in {}. Build a font first.", work_dir.display());
    }
    fs::create_dir_all(dest_dir)?;
    let mut exported = 0usize;
    for entry in fs::read_dir(&work_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with("_system_mod.ttf") {
            fs::copy(entry.path(), dest_dir.join(&name))?;
            exported += 1;
        }
    }
    if exported == 0 {
        anyhow::bail!("No built *_system_mod.ttf files found to export.");
    }
    Ok(exported)
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}_{}_{}", std::process::id(), ts))
}

/// Packages all built `*_system_mod.ttf` files plus a `package.json` manifest into a shareable `.zip`.
pub fn export_mod_package(
    dest_zip: &Path,
    latin_name: Option<&str>,
    khmer_name: Option<&str>,
) -> anyhow::Result<usize> {
    let stage_dir = unique_temp_dir("segoeui_pkg_stage");
    if stage_dir.exists() {
        let _ = fs::remove_dir_all(&stage_dir);
    }
    let count = export_built_fonts(&stage_dir)?;

    let manifest = serde_json::json!({
        "format": "segoeui-changer-package-v1",
        "latin": latin_name,
        "khmer": khmer_name,
        "font_count": count,
    });
    fs::write(
        stage_dir.join("package.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    if dest_zip.exists() {
        let _ = fs::remove_file(dest_zip);
    }
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Compress-Archive -LiteralPath (Get-ChildItem -LiteralPath $env:SEGOEUI_PS_SRC | Select-Object -ExpandProperty FullName) -DestinationPath $env:SEGOEUI_PS_DST -Force",
        ])
        .env("SEGOEUI_PS_SRC", &stage_dir)
        .env("SEGOEUI_PS_DST", dest_zip)
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;

    let _ = fs::remove_dir_all(&stage_dir);
    if !status.success() || !dest_zip.exists() {
        anyhow::bail!("Failed to create zip package at {}", dest_zip.display());
    }
    Ok(count)
}

/// Extracts a `.zip` mod package, backs up stock Segoe UI, installs the fonts, and updates HKLM.
pub fn import_and_apply_package(src_zip: &Path) -> anyhow::Result<usize> {
    let import_dir = unique_temp_dir("segoeui_pkg_import");
    if import_dir.exists() {
        let _ = fs::remove_dir_all(&import_dir);
    }
    fs::create_dir_all(&import_dir)?;

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Expand-Archive -LiteralPath $env:SEGOEUI_PS_SRC -DestinationPath $env:SEGOEUI_PS_DST -Force",
        ])
        .env("SEGOEUI_PS_SRC", src_zip)
        .env("SEGOEUI_PS_DST", &import_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;

    if !status.success() {
        let _ = fs::remove_dir_all(&import_dir);
        anyhow::bail!("Failed to extract package {}", src_zip.display());
    }

    let marker = import_dir.join("segoeui_system_mod.ttf");
    if !fs::symlink_metadata(&marker).is_ok_and(|m| m.file_type().is_file()) {
        let _ = fs::remove_dir_all(&import_dir);
        anyhow::bail!("Invalid package: segoeui_system_mod.ttf not found in archive.");
    }

    let _ = ensure_backup()?;

    let fonts_dir = windows_fonts_dir();
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(1);
    let tag = format!("{ms}_{}", std::process::id());
    let mut reg_updates: Vec<(&str, String)> = Vec::new();
    let mut copied_paths: Vec<PathBuf> = Vec::new();
    for spec in SEGOE_FONT_SPECS {
        let src = import_dir.join(spec.mod_file);
        if fs::symlink_metadata(&src).is_ok_and(|m| m.file_type().is_file()) {
            let bytes = fs::read(&src)?;
            if write_fonts::read::FontRef::new(&bytes).is_err() {
                let _ = fs::remove_dir_all(&import_dir);
                anyhow::bail!("Invalid font file in package: {}", spec.mod_file);
            }
            let tagged = spec.mod_file.replace(".ttf", &format!("_{tag}.ttf"));
            let dst = fonts_dir.join(&tagged);
            if let Err(e) = fs::copy(&src, &dst) {
                for p in &copied_paths {
                    let _ = fs::remove_file(p);
                }
                let _ = fs::remove_dir_all(&import_dir);
                return Err(e.into());
            }
            copied_paths.push(dst);
            reg_updates.push((spec.reg_key, tagged));
        }
    }

    let reg_refs: Vec<(&str, &str)> = reg_updates
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    if let Err(e) = set_font_registry_entries(&reg_refs) {
        for p in &copied_paths {
            let _ = fs::remove_file(p);
        }
        let _ = fs::remove_dir_all(&import_dir);
        return Err(e);
    }
    let _ = crate::domain::resizer::set_system_font_face("Segoe UI");
    let _ = cleanup_stale_mod_files();
    let _ = fs::remove_dir_all(&import_dir);

    Ok(reg_updates.len())
}

/// Patches a font's OpenType `name` table in pure Rust using `write-fonts` and writes the result to `dst`.
pub fn patch_font_name_table(
    src: &Path,
    dst: &Path,
    transform: impl Fn(u16, &str) -> Option<String>,
) -> anyhow::Result<()> {
    use write_fonts::from_obj::ToOwnedTable;
    use write_fonts::read::{FontRef, TableProvider};
    use write_fonts::{FontBuilder, OffsetMarker};

    let bytes = fs::read(src)?;
    let font = FontRef::new(&bytes)?;
    let mut name_table: write_fonts::tables::name::Name = font.name()?.to_owned_table();

    for record in &mut name_table.name_record {
        let name_id = record.name_id.to_u16();
        if let Some(new_text) = transform(name_id, record.string.as_str()) {
            record.string = OffsetMarker::new(new_text);
        }
    }

    let mut builder = FontBuilder::new();
    builder.add_table(&name_table)?;
    builder.copy_missing_tables(font);
    let out_bytes = builder.build();
    fs::write(dst, out_bytes)?;
    Ok(())
}

/// Clones backed-up Segoe UI fonts as `Segoe UI Clone` in pure Rust,
/// then registers them in HKLM Fonts.
pub fn clone_original_segoe(_fontforge_exe: &Path, backup_dir: &Path) -> anyhow::Result<()> {
    let fonts_dir = windows_fonts_dir();
    let mut existing: Vec<(&str, &str)> = Vec::new();

    for spec in SEGOE_FONT_SPECS.iter().filter(|s| !s.clone_file.is_empty()) {
        let src_path = backup_dir.join(spec.stock_file);
        if !src_path.exists() {
            continue;
        }
        let dst_path = fonts_dir.join(spec.clone_file);
        patch_font_name_table(&src_path, &dst_path, |name_id, text| {
            if !text.contains("Segoe UI") {
                return None;
            }
            let mut new_text = text.replace("Segoe UI", "Segoe UI Clone");
            if name_id == 6 {
                new_text = new_text.replace(' ', "");
            }
            Some(new_text)
        })?;
        if dst_path.exists() {
            existing.push((spec.clone_reg_key, spec.clone_file));
        }
    }

    if !existing.is_empty() {
        set_font_registry_entries(&existing)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::patch_font_name_table;
    use write_fonts::from_obj::ToOwnedTable;
    use write_fonts::read::{FontRef, TableProvider};

    #[test]
    fn patch_font_name_table_rewrites_records() {
        let src = std::path::Path::new(r"C:\Windows\Fonts\segoeui.ttf");
        assert!(src.exists(), "expected Windows system font segoeui.ttf");

        let dst = std::env::temp_dir().join("segoeui_patch_test.ttf");
        let _ = std::fs::remove_file(&dst);

        patch_font_name_table(src, &dst, |name_id, text| {
            if !text.contains("Segoe UI") {
                return None;
            }
            let mut updated = text.replace("Segoe UI", "Segoe UI Clone");
            if name_id == 6 {
                updated = updated.replace(' ', "");
            }
            Some(updated)
        })
        .expect("patch_font_name_table should succeed");

        let out_bytes = std::fs::read(&dst).expect("patched font should exist");
        let font = FontRef::new(&out_bytes).expect("patched font should parse");
        let name_table: write_fonts::tables::name::Name =
            font.name().expect("patched font should have name table").to_owned_table();
        let has_clone = name_table
            .name_record
            .iter()
            .any(|r| r.string.as_str().contains("Segoe UI Clone"));
        assert!(has_clone, "patched name table should contain 'Segoe UI Clone'");

        let _ = std::fs::remove_file(&dst);
    }
}
