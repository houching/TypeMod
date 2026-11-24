use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use read_fonts::FontRef;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::domain::registry::{
    SEGOE_FONT_SPECS, SegoeFontSpec, install_mod_fonts_from_bytes, set_font_registry_entries,
    windows_fonts_dir,
};

const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

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
        if src.exists()
            && !dst.exists()
            && let Ok(meta) = fs::metadata(&src)
            && meta.len() <= MAX_FONT_BYTES
            && let Ok(bytes) = fs::read(&src)
            && FontRef::new(&bytes).is_ok()
        {
            fs::write(&dst, &bytes)?;
        }
    }

    Ok(dir)
}

pub fn backup_status() -> (usize, usize) {
    let dir = backup_dir();
    let fonts_dir = windows_fonts_dir();
    let mut have = 0usize;
    let mut total = 0usize;
    for s in SEGOE_FONT_SPECS {
        let in_bak = dir.join(s.stock_file).exists();
        if in_bak || fonts_dir.join(s.stock_file).exists() {
            total += 1;
            if in_bak {
                have += 1;
            }
        }
    }
    (have, total.max(1))
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

/// Packages all built `*_system_mod.ttf` files plus a `package.json` manifest into a shareable `.zip`.
pub fn export_mod_package(
    dest_zip: &Path,
    latin_name: Option<&str>,
    khmer_name: Option<&str>,
) -> anyhow::Result<usize> {
    let work_dir = crate::app_state::AppState::app_docs_dir().join("work");
    if !work_dir.exists() {
        anyhow::bail!("No built fonts found in {}. Build a font first.", work_dir.display());
    }

    let mut font_files: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in fs::read_dir(&work_dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with("_system_mod.ttf")
            && fs::symlink_metadata(entry.path()).is_ok_and(|m| m.file_type().is_file())
        {
            let bytes = fs::read(entry.path())?;
            if bytes.len() as u64 <= MAX_FONT_BYTES && FontRef::new(&bytes).is_ok() {
                font_files.push((name, bytes));
            }
        }
    }
    if font_files.is_empty() {
        anyhow::bail!("No built *_system_mod.ttf files found to export.");
    }

    let count = font_files.len();
    let manifest = serde_json::json!({
        "format": "segoeui-changer-package-v1",
        "latin": latin_name,
        "khmer": khmer_name,
        "font_count": count,
    });
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;

    if let Some(parent) = dest_zip.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_zip = dest_zip.with_extension("zip.tmp");
    let file = fs::File::create(&tmp_zip)?;
    let mut writer = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for (name, bytes) in &font_files {
        writer.start_file(name, opts)?;
        writer.write_all(bytes)?;
    }
    writer.start_file("package.json", opts)?;
    writer.write_all(&manifest_bytes)?;
    writer.finish()?;

    if dest_zip.exists() {
        let _ = fs::remove_file(dest_zip);
    }
    fs::rename(&tmp_zip, dest_zip)?;
    Ok(count)
}

fn read_validated_package_fonts(
    src_zip: &Path,
    include_variable: bool,
) -> anyhow::Result<Vec<(&'static SegoeFontSpec, Vec<u8>)>> {
    let file = fs::File::open(src_zip)?;
    let mut archive = ZipArchive::new(file)?;
    let mut extracted: Vec<(&'static SegoeFontSpec, Vec<u8>)> = Vec::new();

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)?;
        if entry.is_dir() {
            continue;
        }
        // enclosed_name() rejects any path traversal (`..`, absolute/drive prefixes)
        let Some(safe_path) = entry.enclosed_name() else {
            continue;
        };
        if safe_path.components().count() != 1 {
            continue;
        }
        let Some(file_name) = safe_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(spec) = SEGOE_FONT_SPECS
            .iter()
            .find(|s| s.mod_file.eq_ignore_ascii_case(file_name))
        else {
            continue;
        };
        if spec.is_variable && !include_variable {
            continue;
        }
        if entry.size() > MAX_FONT_BYTES {
            anyhow::bail!("Font entry '{}' exceeds 64 MB size limit", spec.mod_file);
        }

        let mut bytes = Vec::with_capacity((entry.size() as usize).min(MAX_FONT_BYTES as usize));
        let mut limited = (&mut entry).take(MAX_FONT_BYTES + 1);
        limited.read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FONT_BYTES {
            anyhow::bail!("Font entry '{}' exceeds 64 MB size limit", spec.mod_file);
        }
        if bytes.is_empty() || FontRef::new(&bytes).is_err() {
            anyhow::bail!("Invalid or corrupt font file in package: {}", spec.mod_file);
        }
        extracted.push((spec, bytes));
    }

    if !extracted
        .iter()
        .any(|(spec, _)| spec.mod_file == "segoeui_system_mod.ttf")
    {
        anyhow::bail!("Invalid package: segoeui_system_mod.ttf not found in archive.");
    }

    Ok(extracted)
}

/// Extracts a `.zip` mod package in-memory, validates all fonts, backs up stock Segoe UI,
/// installs the fonts, and updates HKLM.
pub fn import_and_apply_package(src_zip: &Path) -> anyhow::Result<usize> {
    let include_variable = windows_fonts_dir().join("SegUIVar.ttf").exists();
    let items = read_validated_package_fonts(src_zip, include_variable)?;
    let _ = ensure_backup()?;
    install_mod_fonts_from_bytes(&items)
}

/// Patches a font's OpenType `name` table in memory using `write-fonts`.
pub fn patch_font_name_table_bytes(
    bytes: &[u8],
    transform: impl Fn(u16, &str) -> Option<String>,
) -> anyhow::Result<Vec<u8>> {
    use write_fonts::from_obj::ToOwnedTable;
    use write_fonts::read::{FontRef, TableProvider};
    use write_fonts::{FontBuilder, OffsetMarker};

    let font = FontRef::new(bytes)?;
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
    Ok(builder.build())
}

/// Patches a font's OpenType `name` table in pure Rust using `write-fonts` and writes the result to `dst`.
pub fn patch_font_name_table(
    src: &Path,
    dst: &Path,
    transform: impl Fn(u16, &str) -> Option<String>,
) -> anyhow::Result<()> {
    let bytes = fs::read(src)?;
    let out_bytes = patch_font_name_table_bytes(&bytes, transform)?;
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

    #[test]
    fn package_reader_rejects_zip_slip_and_corrupt_fonts() {
        use std::io::Write;
        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let valid_font = std::path::Path::new(r"C:\Windows\Fonts\segoeui.ttf");
        assert!(valid_font.exists());
        let valid_bytes = std::fs::read(valid_font).unwrap();

        let dir = std::env::temp_dir().join(format!("segoeui_zip_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // 1. Archive with Zip Slip entry and valid segoeui_system_mod.ttf: Zip Slip must be ignored
        let slip_zip = dir.join("slip.zip");
        {
            let mut w = ZipWriter::new(std::fs::File::create(&slip_zip).unwrap());
            let opts = SimpleFileOptions::default();
            w.start_file("../../segoeuib_system_mod.ttf", opts).unwrap();
            w.write_all(&valid_bytes).unwrap();
            w.start_file("segoeui_system_mod.ttf", opts).unwrap();
            w.write_all(&valid_bytes).unwrap();
            w.finish().unwrap();
        }
        let items = super::read_validated_package_fonts(&slip_zip, false).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0.mod_file, "segoeui_system_mod.ttf");

        // 2. Archive with corrupt segoeui_system_mod.ttf must fail validation
        let corrupt_zip = dir.join("corrupt.zip");
        {
            let mut w = ZipWriter::new(std::fs::File::create(&corrupt_zip).unwrap());
            w.start_file("segoeui_system_mod.ttf", SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"not a real ttf").unwrap();
            w.finish().unwrap();
        }
        assert!(super::read_validated_package_fonts(&corrupt_zip, false).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
