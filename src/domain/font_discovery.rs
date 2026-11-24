use std::fs;
use std::path::{Path, PathBuf};

use read_fonts::{FontRef, TableProvider, types::Tag};

use crate::app_state::FontFamilySelection;

const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;

pub fn is_variable_font(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() > MAX_FONT_BYTES {
        return false;
    }
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(font) = FontRef::new(&bytes) else {
        return false;
    };
    let Ok(fvar) = font.fvar() else {
        return false;
    };
    let Ok(axes) = fvar.axes() else {
        return false;
    };
    axes.iter().any(|a| a.axis_tag() == Tag::new(b"wght"))
}

pub fn auto_detect_weights(
    reg_path: &Path,
    reg_name: Option<&str>,
    installed_fonts: &[(String, PathBuf)],
) -> FontFamilySelection {
    auto_detect_weights_for_style(reg_path, reg_name, installed_fonts, false)
}

pub fn auto_detect_weights_for_style(
    reg_path: &Path,
    reg_name: Option<&str>,
    installed_fonts: &[(String, PathBuf)],
    expect_italic: bool,
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
        if !stem_base.is_empty() {
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

                let Some(suffix_lc) = strip_family_prefix(&name_lc, &stem_base) else {
                    continue;
                };
                let is_italic = name_lc.contains("italic") || name_lc.contains("oblique");
                if is_italic != expect_italic {
                    continue;
                }

                assign_weight_by_name(&mut sel, suffix_lc, path);
            }
        }
    }

    if let Some(base_name) = reg_name {
        let family = strip_weight_suffix(base_name).to_ascii_lowercase();
        if !family.is_empty() {
            for (name, path) in installed_fonts {
                let name_lc = name.to_ascii_lowercase();
                let Some(suffix_lc) = strip_family_prefix(&name_lc, &family) else {
                    continue;
                };
                let is_italic = name_lc.contains("italic") || name_lc.contains("oblique");
                if is_italic != expect_italic {
                    continue;
                }
                assign_weight_by_name(&mut sel, suffix_lc, path);
            }
        }
    }

    sel
}

fn extract_family_stem(path: &Path) -> String {
    let mut stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    for suffix in [
        "-italic",
        "_italic",
        " italic",
        "-oblique",
        "_oblique",
        " oblique",
        "-regular",
        "_regular",
        " regular",
        "-variable",
        "_variable",
        "variable",
        "regular",
        "italic",
        "oblique",
    ] {
        if let Some(stripped) = stem.strip_suffix(suffix) {
            stem = stripped.trim_end_matches(['-', '_', ' ']).to_string();
            break;
        }
    }
    stem
}

fn strip_weight_suffix(name: &str) -> String {
    let mut out = name.trim().to_string();
    loop {
        let mut changed = false;
        for suffix in [
            " Oblique",
            " Italic",
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
                out = s.trim_end().to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            break;
        }
    }
    out
}

fn strip_family_prefix<'a>(candidate_lc: &'a str, family_lc: &str) -> Option<&'a str> {
    let rest = candidate_lc.strip_prefix(family_lc)?;
    if rest.is_empty() || rest.starts_with(['-', '_', ' ']) {
        Some(rest)
    } else {
        None
    }
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

    #[test]
    fn detects_italic_weights_and_avoids_family_keyword_collisions() {
        let dir = std::env::temp_dir().join(format!("segoeui_fd_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let reg = dir.join("Highlight-Regular.ttf");
        let med = dir.join("Highlight-Medium.ttf");
        let bold = dir.join("Highlight-Bold.ttf");
        let ital = dir.join("Highlight-Italic.ttf");
        let bold_ital = dir.join("Highlight-BoldItalic.ttf");
        for p in [&reg, &med, &bold, &ital, &bold_ital] {
            std::fs::write(p, b"").unwrap();
        }

        let norm = auto_detect_weights_for_style(&reg, Some("Highlight"), &[], false);
        assert!(norm.light.is_none(), "Highlight-Medium must not match light");
        assert_eq!(norm.bold.as_deref(), Some(bold.as_path()));

        let it = auto_detect_weights_for_style(&ital, Some("Highlight Italic"), &[], true);
        assert_eq!(it.bold.as_deref(), Some(bold_ital.as_path()));

        // Empty family stem ("Regular.ttf") must not match random siblings
        let empty_stem = dir.join("Regular.ttf");
        std::fs::write(&empty_stem, b"").unwrap();
        let empty_sel = auto_detect_weights_for_style(&empty_stem, None, &[], false);
        assert!(empty_sel.bold.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
