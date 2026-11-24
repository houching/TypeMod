use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
    Render, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use read_fonts::{FontRef, TableProvider, types::NameId};

use crate::app_state::AppState;
use crate::ui::components::{BadgeTone, ButtonVariant, StatusPill, VantageButton};
use crate::ui::text_input::{TextBoxProps, TextFieldState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewSource {
    BuiltOutput,
    SelectedFonts,
    SystemSegoe,
}

pub struct PreviewView {
    state: Entity<AppState>,
    source: PreviewSource,
    font_size_px: f32,
    input: TextFieldState,
    loaded_paths: HashSet<PathBuf>,
    last_candidates: Vec<PathBuf>,
    has_selected: bool,
    load_gen: u64,
    selected_family: Option<SharedString>,
    built_loaded_rev: Option<u64>,
    built_gen: u64,
    built_family: Option<SharedString>,
    load_error: Option<String>,
    scroll_handle: gpui::ScrollHandle,
}

impl PreviewView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            source: PreviewSource::SelectedFonts,
            font_size_px: 18.0,
            input: TextFieldState::new(
                "សួស្តីកម្ពុជា — ភាសាខ្មែរ និង English UI 0123456789",
                cx.focus_handle(),
            ),
            loaded_paths: HashSet::new(),
            last_candidates: Vec::new(),
            has_selected: false,
            load_gen: 0,
            selected_family: None,
            built_loaded_rev: None,
            built_gen: 0,
            built_family: None,
            load_error: None,
            scroll_handle: gpui::ScrollHandle::new(),
        }
    }

    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.input.cursor_visible = true;
        if self.input._blink_task.is_some() {
            return;
        }
        self.input._blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(530))
                    .await;
                let keep_going = this
                    .update(cx, |view, cx| {
                        if view.state.read(cx).active_tab != crate::app_state::NavTab::Preview {
                            view.input.cursor_visible = true;
                            view.input._blink_task = None;
                            return false;
                        }
                        view.input.cursor_visible = !view.input.cursor_visible;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    fn ensure_selected_fonts_loaded(&mut self, cx: &mut Context<Self>) {
        let cfg = &self.state.read(cx).config;
        let reg_path = cfg.latin.regular.clone();
        let mut raw_candidates: Vec<PathBuf> = [
            &cfg.latin.regular,
            &cfg.latin.light,
            &cfg.latin.semilight,
            &cfg.latin.semibold,
            &cfg.latin.bold,
            &cfg.latin.black,
            &cfg.khmer.regular,
        ]
        .into_iter()
        .filter_map(|p| p.clone())
        .collect();

        for sel in cfg.extra_script_fonts.values() {
            if let Some(p) = &sel.regular {
                raw_candidates.push(p.clone());
            }
        }

        if raw_candidates == self.last_candidates {
            return;
        }
        self.last_candidates = raw_candidates.clone();
        self.has_selected = reg_path.is_some();
        self.selected_family = None;
        self.load_gen += 1;
        let req_gen = self.load_gen;

        let to_load: Vec<PathBuf> = raw_candidates
            .into_iter()
            .filter(|p| !self.loaded_paths.contains(p))
            .collect();
        let all_candidates = self.last_candidates.clone();

        cx.spawn(async move |this, cx| {
            let (valid_fonts, first_family, reg_exists, read_err) = cx
                .background_executor()
                .spawn(async move {
                    let mut read_cache: std::collections::HashMap<PathBuf, Vec<u8>> =
                        std::collections::HashMap::new();
                    let mut valid_fonts: Vec<(PathBuf, Vec<u8>)> = Vec::new();
                    let mut first_err: Option<String> = None;
                    for path in &to_load {
                        match fs::read(path) {
                            Ok(bytes) => {
                                if FontRef::new(&bytes).is_ok() {
                                    read_cache.insert(path.clone(), bytes.clone());
                                    valid_fonts.push((path.clone(), bytes));
                                } else if first_err.is_none() {
                                    first_err = Some(format!(
                                        "Invalid font file for preview: {}",
                                        path.display()
                                    ));
                                }
                            }
                            Err(e) => {
                                if first_err.is_none() {
                                    first_err = Some(format!(
                                        "Cannot read font {}: {e}",
                                        path.display()
                                    ));
                                }
                            }
                        }
                    }
                    let first_family = all_candidates.iter().find_map(|p| {
                        if let Some(bytes) = read_cache.get(p) {
                            extract_family_name(bytes)
                        } else {
                            fs::read(p).ok().and_then(|b| extract_family_name(&b))
                        }
                    });
                    let reg_exists = reg_path.is_some_and(|p| p.exists());
                    (valid_fonts, first_family, reg_exists, first_err)
                })
                .await;

            let _ = this.update(cx, |view, cx| {
                if view.load_gen != req_gen {
                    return;
                }
                view.has_selected = reg_exists;
                if view.selected_family.is_none() {
                    view.selected_family = first_family.map(Into::into);
                }
                let mut err_msg = read_err;
                for (path, bytes) in valid_fonts {
                    match cx.text_system().add_fonts(vec![Cow::Owned(bytes)]) {
                        Ok(()) => {
                            view.loaded_paths.insert(path);
                        }
                        Err(e) => {
                            if err_msg.is_none() {
                                err_msg = Some(format!(
                                    "Failed to load {} into preview: {e}",
                                    path.display()
                                ));
                            }
                        }
                    }
                }
                view.load_error = err_msg;
                cx.notify();
            });
        })
        .detach();
    }

    fn ensure_built_fonts_loaded(&mut self, cx: &mut Context<Self>) {
        let (has_built, rev) = {
            let s = self.state.read(cx);
            (s.has_built_fonts, s.built_revision)
        };
        if !has_built {
            self.built_family = None;
            self.built_loaded_rev = None;
            return;
        }
        if self.built_loaded_rev == Some(rev) {
            return;
        }
        self.built_loaded_rev = Some(rev);
        self.built_gen += 1;
        let req_gen = self.built_gen;
        let preview_family = format!("SegoeUI Built Preview {}", rev + 1);

        cx.spawn(async move |this, cx| {
            let fam_clone = preview_family.clone();
            let (patched_blobs, first_err) = cx
                .background_executor()
                .spawn(async move {
                    let work_dir = crate::domain::builder::built_fonts_dir();
                    let mut blobs: Vec<Vec<u8>> = Vec::new();
                    let mut first_err: Option<String> = None;
                    for spec in crate::domain::registry::SEGOE_FONT_SPECS
                        .iter()
                        .filter(|s| !s.is_italic && !s.is_variable)
                    {
                        let p = work_dir.join(spec.mod_file);
                        if !p.is_file() {
                            continue;
                        }
                        match fs::read(&p) {
                            Ok(bytes) => {
                                let fam_ref = fam_clone.as_str();
                                let fam_no_space = fam_ref.replace(' ', "");
                                match crate::domain::backup_cloner::patch_font_name_table_bytes(
                                    &bytes,
                                    |name_id, text| {
                                        if name_id == 6 {
                                            if text.contains("SegoeUI") {
                                                Some(text.replace("SegoeUI", &fam_no_space))
                                            } else if text.contains("Segoe UI") {
                                                Some(text.replace("Segoe UI", &fam_no_space))
                                            } else {
                                                None
                                            }
                                        } else if text.contains("Segoe UI") {
                                            Some(text.replace("Segoe UI", fam_ref))
                                        } else if text.contains("SegoeUI") {
                                            Some(text.replace("SegoeUI", &fam_no_space))
                                        } else {
                                            None
                                        }
                                    },
                                ) {
                                    Ok(patched) => blobs.push(patched),
                                    Err(e) if first_err.is_none() => {
                                        first_err = Some(format!(
                                            "Failed to prepare built preview for {}: {e}",
                                            spec.mod_file
                                        ));
                                    }
                                    Err(_) => {}
                                }
                            }
                            Err(e) if first_err.is_none() => {
                                first_err = Some(format!("Cannot read {}: {e}", p.display()));
                            }
                            Err(_) => {}
                        }
                    }
                    (blobs, first_err)
                })
                .await;

            let _ = this.update(cx, |view, cx| {
                if view.built_gen != req_gen {
                    return;
                }
                let mut err_msg = first_err;
                let mut any_ok = false;
                for bytes in patched_blobs {
                    match cx.text_system().add_fonts(vec![Cow::Owned(bytes)]) {
                        Ok(()) => any_ok = true,
                        Err(e) if err_msg.is_none() => {
                            err_msg = Some(format!("Failed to load built font into preview: {e}"));
                        }
                        Err(_) => {}
                    }
                }
                if any_ok {
                    view.built_family = Some(preview_family.into());
                }
                view.load_error = err_msg;
                cx.notify();
            });
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.input.handle_key_down(event, cx) {
            cx.notify();
        }
    }

    fn adjust_size(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.font_size_px = (self.font_size_px + delta).clamp(10.0, 48.0);
        cx.notify();
    }

    fn set_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.font_size_px = size.clamp(10.0, 48.0);
        cx.notify();
    }
}

fn extract_family_name(bytes: &[u8]) -> Option<String> {
    let font = FontRef::new(bytes).ok()?;
    let name_table = font.name().ok()?;
    for target_id in [NameId::TYPOGRAPHIC_FAMILY_NAME, NameId::FAMILY_NAME] {
        for rec in name_table.name_record() {
            if rec.name_id() == target_id
                && let Ok(s) = rec.string(name_table.string_data())
            {
                let text: String = s.chars().collect();
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

impl Render for PreviewView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.state.read(cx).preview_built_requested {
            self.source = PreviewSource::BuiltOutput;
            self.state.update(cx, |s, _| {
                s.preview_built_requested = false;
            });
        }

        match self.source {
            PreviewSource::BuiltOutput => self.ensure_built_fonts_loaded(cx),
            PreviewSource::SelectedFonts => self.ensure_selected_fonts_loaded(cx),
            PreviewSource::SystemSegoe => {}
        }

        let s = self.state.read(cx);
        let theme = s.theme();
        let has_built = s.has_built_fonts;
        let is_focused = self.input.focus_handle.is_focused(window);
        if !is_focused && self.input._blink_task.is_some() {
            self.input.cursor_visible = true;
            self.input._blink_task = None;
        }
        let has_selected = self.has_selected;
        let active_family: SharedString = match self.source {
            PreviewSource::BuiltOutput => self
                .built_family
                .clone()
                .unwrap_or_else(|| "Segoe UI".into()),
            PreviewSource::SelectedFonts => self
                .selected_family
                .clone()
                .unwrap_or_else(|| "Segoe UI".into()),
            PreviewSource::SystemSegoe => "Segoe UI".into(),
        };

        let weights: [(&'static str, FontWeight); 5] = [
            ("Light (300)", FontWeight::LIGHT),
            ("Regular (400)", FontWeight::NORMAL),
            ("Semibold (600)", FontWeight::SEMIBOLD),
            ("Bold (700)", FontWeight::BOLD),
            ("Black (900)", FontWeight::BLACK),
        ];

        let presets: [(&'static str, &'static str); 3] = [
            (
                "English + Khmer",
                "សួស្តីកម្ពុជា — ភាសាខ្មែរ និង English UI 0123456789",
            ),
            (
                "Khmer Coengs",
                "ខ្ញុំស្រឡាញ់ភាសាខ្មែរ ព្រះរាជាណាចក្រកម្ពុជា ក្រុមហ៊ុន ស្រ្តី ភ្នំពេញ",
            ),
            (
                "UI & Symbols",
                "Settings > System Font (100%) — C:\\Windows\\Fonts [OK]",
            ),
        ];

        let size_px = self.font_size_px;
        let sample = if self.input.text.is_empty() {
            "Type custom text above to preview...".to_string()
        } else {
            self.input.text.clone()
        };

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .child(
                div()
                    .id("preview-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .track_focus(&self.input.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_lg()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Font Preview & Ligature Tester"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_2)
                                    .child("Test English, Khmer coengs/vowels, scale, and weights"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .child(
                                VantageButton::new("prev-src-built", "Built Output", theme)
                                    .variant(if self.source == PreviewSource::BuiltOutput {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Secondary
                                    })
                                    .disabled(!has_built)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.source = PreviewSource::BuiltOutput;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                VantageButton::new("prev-src-sel", "Selected Font", theme)
                                    .variant(if self.source == PreviewSource::SelectedFonts {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Secondary
                                    })
                                    .disabled(!has_selected)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.source = PreviewSource::SelectedFonts;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                VantageButton::new("prev-src-sys", "System Segoe UI", theme)
                                    .variant(if self.source == PreviewSource::SystemSegoe {
                                        ButtonVariant::Primary
                                    } else {
                                        ButtonVariant::Secondary
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.source = PreviewSource::SystemSegoe;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            // Controls Card (Custom Text + Scale)
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_2)
                                    .child("CUSTOM TEST TEXT"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .children(presets.into_iter().map(|(label, text)| {
                                        VantageButton::new(
                                            SharedString::from(format!("preset-{label}")),
                                            label,
                                            theme,
                                        )
                                        .variant(ButtonVariant::Ghost)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.input.set_text(text);
                                            cx.notify();
                                        }))
                                    })),
                            ),
                    )
                    .child(
                        self.input.render_box(
                            TextBoxProps {
                                id: "preview-text-input".into(),
                                clear_id: "preview-text-clear".into(),
                                placeholder: "Type or paste custom text to preview...",
                                leading_icon: None,
                                trailing_badge: None,
                                is_focused,
                                theme,
                            },
                            cx.listener(|this, _, window, cx| {
                                window.focus(&this.input.focus_handle);
                                this.start_blink(cx);
                                cx.notify();
                            }),
                            cx.listener(|this, _, _, cx| {
                                this.input.clear();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_2)
                                            .child("PREVIEW SIZE"),
                                    )
                                    .child(StatusPill::new(
                                        format!("{} px", size_px as i32),
                                        BadgeTone::Accent,
                                        theme,
                                    ))
                                    .child(StatusPill::new(
                                        format!("Family: {active_family}"),
                                        BadgeTone::Neutral,
                                        theme,
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        VantageButton::new("prev-sz-dec", "−", theme)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.adjust_size(-2.0, cx)
                                            })),
                                    )
                                    .child(
                                        VantageButton::new("prev-sz-inc", "+", theme)
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.adjust_size(2.0, cx)
                                            })),
                                    )
                                    .children([12.0f32, 16.0, 24.0, 32.0, 42.0].into_iter().map(
                                        |sz| {
                                            let active = (size_px - sz).abs() < 0.5;
                                            VantageButton::new(
                                                SharedString::from(format!("prev-sz-{sz}")),
                                                format!("{}px", sz as i32),
                                                theme,
                                            )
                                            .variant(if active {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Secondary
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.set_size(sz, cx)
                                            }))
                                        },
                                    )),
                            ),
                    )
                    .when_some(self.load_error.clone(), |d, err| {
                        d.child(div().text_xs().text_color(theme.err).child(err))
                    }),
            )
            // Weight Specimen List
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(weights.into_iter().map(|(label, weight)| {
                        div()
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(theme.surface_2)
                            .border_1()
                            .border_color(theme.line)
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_3)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .font_family(active_family.clone())
                                    .font_weight(weight)
                                    .text_size(px(size_px))
                                    .text_color(theme.text)
                                    .child(sample.clone()),
                            )
                    })),
            )
            )
            .child(crate::ui::components::vertical_scrollbar(
                &self.scroll_handle,
                "preview-scroll-bar",
                theme,
            ))
    }
}
