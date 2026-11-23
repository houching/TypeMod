use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::app_state::FontFamilySelection;

pub fn is_variable_font(path: &Path) -> bool {
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let mut header = [0u8; 12];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    let sfnt = &header[..4];
    if sfnt != [0x00, 0x01, 0x00, 0x00] && sfnt != b"OTTO" && sfnt != b"true" {
        return false;
    }
    let num_tables = u16::from_be_bytes([header[4], header[5]]) as usize;
    if num_tables == 0 || num_tables > 512 {
        return false;
    }
    let mut directory = vec![0u8; num_tables * 16];
    if file.read_exact(&mut directory).is_err() {
        return false;
    }
    directory.chunks_exact(16).any(|rec| &rec[..4] == b"fvar")
}

pub fn auto_detect_weights(
    reg_path: &Path,
    reg_name: Option<&str>,
    installed_fonts: &[(String, PathBuf)],
) -> FontFamilySelection {
    let mut sel = FontFamilySelection {
        regular: Some(reg_path.to_path_buf()),
        is_variable: is_variable_font(reg_path),
        display_name: reg_name.map(ToString::to_string).or_else(|| {
            reg_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
        }),
        ..Default::default()
    };

    if sel.is_variable {
        return sel;
    }

    if let Some(dir) = reg_path.parent()
        && let Ok(entries) = fs::read_dir(dir)
    {
        let stem_base = extract_family_stem(reg_path);
        let files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "ttf" | "otf"))
            })
            .collect();

        for path in &files {
            if path == reg_path {
                continue;
            }
            let name_lc = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();

            if !stem_base.is_empty() && !matches_family_prefix(&name_lc, &stem_base) {
                continue;
            }
            if name_lc.contains("italic") || name_lc.contains("oblique") {
                continue;
            }

            assign_weight_by_name(&mut sel, &name_lc, path);
        }
    }

    if let Some(base_name) = reg_name {
        let family = strip_weight_suffix(base_name).to_ascii_lowercase();
        for (name, path) in installed_fonts {
            let name_lc = name.to_ascii_lowercase();
            if !matches_family_prefix(&name_lc, &family) {
                continue;
            }
            if name_lc.contains("italic") || name_lc.contains("oblique") {
                continue;
            }
            assign_weight_by_name(&mut sel, &name_lc, path);
        }
    }

    sel
}

fn extract_family_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    for suffix in [
        "-regular",
        "_regular",
        " regular",
        "-variable",
        "_variable",
        "variable",
        "regular",
    ] {
        if let Some(stripped) = stem.strip_suffix(suffix) {
            return stripped.trim_end_matches(['-', '_', ' ']).to_string();
        }
    }
    stem
}

fn strip_weight_suffix(name: &str) -> String {
    let mut out = name.trim().to_string();
    for suffix in [
        " Regular",
        " Light",
        " Semilight",
        " SemiLight",
        " Semibold",
        " SemiBold",
        " Bold",
        " Black",
    ] {
        if let Some(s) = out.strip_suffix(suffix) {
            out = s.to_string();
            break;
        }
    }
    out
}

fn matches_family_prefix(candidate_lc: &str, family_lc: &str) -> bool {
    if family_lc.is_empty() {
        return true;
    }
    let Some(rest) = candidate_lc.strip_prefix(family_lc) else {
        return false;
    };
    rest.is_empty() || rest.starts_with(['-', '_', ' '])
}

fn assign_weight_by_name(sel: &mut FontFamilySelection, name_lc: &str, path: &Path) {
    if sel.semilight.is_none() && (name_lc.contains("semilight") || name_lc.contains("demilight")) {
        sel.semilight = Some(path.to_path_buf());
    } else if sel.light.is_none()
        && name_lc.contains("light")
        && !name_lc.contains("extralight")
        && !name_lc.contains("ultralight")
    {
        sel.light = Some(path.to_path_buf());
    } else if sel.semibold.is_none()
        && (name_lc.contains("semibold") || name_lc.contains("demibold"))
    {
        sel.semibold = Some(path.to_path_buf());
    } else if sel.bold.is_none()
        && name_lc.contains("bold")
        && !name_lc.contains("extrabold")
        && !name_lc.contains("ultrabold")
    {
        sel.bold = Some(path.to_path_buf());
    } else if sel.black.is_none() && (name_lc.contains("black") || name_lc.contains("heavy")) {
        sel.black = Some(path.to_path_buf());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_stem_extraction() {
        assert_eq!(
            extract_family_stem(Path::new("C:/Fonts/Inter-Regular.ttf")),
            "inter"
        );
        assert_eq!(
            extract_family_stem(Path::new("C:/Fonts/KantumruyPro-Regular.ttf")),
            "kantumruypro"
        );
    }

    #[test]
    fn weight_matching_distinguishes_semi_variants() {
        let mut sel = FontFamilySelection::default();
        assign_weight_by_name(&mut sel, "inter semibold", Path::new("Inter-SemiBold.ttf"));
        assign_weight_by_name(&mut sel, "inter bold", Path::new("Inter-Bold.ttf"));
        assign_weight_by_name(&mut sel, "inter semilight", Path::new("Inter-SemiLight.ttf"));
        assign_weight_by_name(&mut sel, "inter light", Path::new("Inter-Light.ttf"));
        assign_weight_by_name(&mut sel, "inter black", Path::new("Inter-Black.ttf"));

        assert_eq!(sel.semibold.unwrap(), Path::new("Inter-SemiBold.ttf"));
        assert_eq!(sel.bold.unwrap(), Path::new("Inter-Bold.ttf"));
        assert_eq!(sel.semilight.unwrap(), Path::new("Inter-SemiLight.ttf"));
        assert_eq!(sel.light.unwrap(), Path::new("Inter-Light.ttf"));
        assert_eq!(sel.black.unwrap(), Path::new("Inter-Black.ttf"));
    }

    #[test]
    fn detects_variable_vs_static_fonts() {
        let static_segoe = Path::new(r"C:\Windows\Fonts\segoeui.ttf");
        assert!(static_segoe.exists(), "expected Windows segoeui.ttf");
        assert!(!is_variable_font(static_segoe));

        let var_segoe = Path::new(r"C:\Windows\Fonts\SegUIVar.ttf");
        if var_segoe.exists() {
            assert!(is_variable_font(var_segoe));
        }
    }
}
