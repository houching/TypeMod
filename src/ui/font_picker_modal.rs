use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Render, ScrollStrategy, StatefulInteractiveElement, Styled, UniformListScrollHandle, Window,
    div, px, uniform_list,
};

use crate::app_state::{AppState, FontFamilySelection, PersistedConfig, SecondaryScript};
use crate::domain::font_discovery::auto_detect_weights_for_style;
use crate::domain::registry::list_installed_fonts;
use crate::ui::components::{ButtonVariant, VantageButton};
use crate::ui::text_input::{TextBoxProps, TextFieldState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontTargetSlot {
    Latin,
    Script(SecondaryScript),
    LatinItalic,
    ScriptItalic(SecondaryScript),
}

impl FontTargetSlot {
    pub fn is_italic(self) -> bool {
        matches!(self, Self::LatinItalic | Self::ScriptItalic(_))
    }

    pub fn title(self) -> String {
        match self {
            Self::Latin => "Primary English / Latin Font".to_string(),
            Self::Script(script) => format!("{} Base Font (Optional)", script.label()),
            Self::LatinItalic => "Latin Italic Font".to_string(),
            Self::ScriptItalic(script) => format!("{} Italic Font", script.label()),
        }
    }

    pub fn select_mut(self, cfg: &mut PersistedConfig) -> &mut FontFamilySelection {
        match self {
            Self::Latin => &mut cfg.latin,
            Self::Script(script) => cfg.script_font_mut(script),
            Self::LatinItalic => &mut cfg.latin_italic,
            Self::ScriptItalic(script) => cfg.script_italic_font_mut(script),
        }
    }
}

pub struct FontPickerModal {
    pub is_open: bool,
    pub target: FontTargetSlot,
    pub input: TextFieldState,
    pub fonts: Vec<(String, PathBuf)>,
    fonts_lc: Vec<String>,
    cached_installed: Option<Vec<(String, PathBuf)>>,
    filtered_indices: Vec<usize>,
    total_matches: usize,
    cached_query: String,
    filter_dirty: bool,
    open_gen: u64,
    selected_idx: usize,
    scroll_handle: UniformListScrollHandle,
    state: Entity<AppState>,
}

impl FontPickerModal {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            is_open: false,
            target: FontTargetSlot::Latin,
            input: TextFieldState::new("", cx.focus_handle(), cx),
            fonts: Vec::new(),
            fonts_lc: Vec::new(),
            cached_installed: None,
            filtered_indices: Vec::new(),
            total_matches: 0,
            cached_query: String::new(),
            filter_dirty: true,
            open_gen: 0,
            selected_idx: 0,
            scroll_handle: UniformListScrollHandle::new(),
            state,
        }
    }

    fn set_fonts(&mut self, fonts: Vec<(String, PathBuf)>) {
        self.fonts_lc = fonts.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
        self.fonts = fonts;
        self.filter_dirty = true;
    }

    fn ensure_filtered(&mut self) {
        if !self.filter_dirty && self.input.text == self.cached_query {
            return;
        }
        self.cached_query.clear();
        self.cached_query.push_str(&self.input.text);
        let query_lc = self.cached_query.to_ascii_lowercase();
        self.filtered_indices.clear();
        let mut total = 0usize;
        for (idx, name_lc) in self.fonts_lc.iter().enumerate() {
            if query_lc.is_empty() || name_lc.contains(&query_lc) {
                total += 1;
                if self.filtered_indices.len() < 150 {
                    self.filtered_indices.push(idx);
                }
            }
        }
        self.total_matches = total;
        self.filter_dirty = false;
    }

    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.input.start_blink(cx);
    }

    pub fn open(&mut self, target: FontTargetSlot, window: &mut Window, cx: &mut Context<Self>) {
        self.target = target;
        self.input.clear(cx);
        self.selected_idx = 0;
        self.open_gen += 1;
        let req_gen = self.open_gen;
        self.is_open = true;
        window.focus(&self.input.focus_handle);
        self.start_blink(cx);

        if let Some(cached) = self.cached_installed.clone() {
            self.set_fonts(cached);
            self.ensure_filtered();
            cx.notify();
            return;
        }

        self.fonts.clear();
        self.fonts_lc.clear();
        self.filtered_indices.clear();
        self.total_matches = 0;
        self.filter_dirty = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let fonts = cx
                .background_executor()
                .spawn(async move { list_installed_fonts() })
                .await;
            let _ = this.update(cx, |modal, cx| {
                modal.cached_installed = Some(fonts.clone());
                if modal.is_open && modal.open_gen == req_gen {
                    modal.set_fonts(fonts);
                    modal.ensure_filtered();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.is_open = false;
        self.input.stop_blink(cx);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_open {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close(cx);
            }
            "up" => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    self.scroll_handle
                        .scroll_to_item(self.selected_idx, ScrollStrategy::Center);
                    cx.notify();
                }
            }
            "down" => {
                self.ensure_filtered();
                if !self.filtered_indices.is_empty()
                    && self.selected_idx + 1 < self.filtered_indices.len()
                {
                    self.selected_idx += 1;
                    self.scroll_handle
                        .scroll_to_item(self.selected_idx, ScrollStrategy::Center);
                    cx.notify();
                }
            }
            "enter" => {
                self.ensure_filtered();
                if let Some(&font_idx) = self.filtered_indices.get(self.selected_idx)
                    && let Some((name, path)) = self.fonts.get(font_idx).cloned()
                {
                    self.select_font(name, path, cx);
                }
            }
            _ => {
                if self.input.handle_key_down(event, cx) {
                    self.selected_idx = 0;
                    self.filter_dirty = true;
                    cx.notify();
                }
            }
        }
    }

    fn select_font(&mut self, name: String, path: PathBuf, cx: &mut Context<Self>) {
        let installed = self.fonts.clone();
        let target = self.target;
        let state = self.state.clone();
        self.close(cx);

        cx.spawn(async move |_, cx| {
            let detected = cx
                .background_executor()
                .spawn(async move {
                    auto_detect_weights_for_style(
                        &path,
                        Some(&name),
                        &installed,
                        target.is_italic(),
                    )
                })
                .await;

            let _ = state.update(cx, |s, cx| {
                if s.is_busy {
                    return;
                }
                *target.select_mut(&mut s.config) = detected;
                s.save_config_async(cx.background_executor());
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for FontPickerModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_open {
            return div().into_any_element();
        }

        self.ensure_filtered();
        let theme = self.state.read(cx).theme();
        let is_focused = self.input.focus_handle.is_focused(window);
        let visible_count = self.filtered_indices.len();
        let total_matches = self.total_matches;
        let selected_idx = self.selected_idx.min(visible_count.saturating_sub(1));
        let match_badge = if total_matches > visible_count {
            format!("{visible_count}/{total_matches} matches")
        } else {
            format!("{total_matches} matches")
        };

        div()
            .id("font-picker-backdrop")
            .occlude()
            .absolute()
            .inset_0()
            .p(px(20.0))
            .bg(gpui::rgba(0x00000088))
            .flex()
            .items_center()
            .justify_center()
            .track_focus(&self.input.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.close(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("font-picker-card")
                    .occlude()
                    .w_full()
                    .max_w(px(520.0))
                    .h_full()
                    .max_h(px(560.0))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line_2)
                    .rounded(px(8.0))
                    .flex()
                    .flex_col()
                    .p(px(16.0))
                    .gap(px(12.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text)
                                            .child("Select Installed System Font"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_3)
                                            .child(self.target.title()),
                                    ),
                            )
                            .child(
                                VantageButton::new("picker-close", "Close", theme)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                            ),
                    )
                    .child({
                        let on_focus = cx.listener(|this, _, window, cx| {
                            window.focus(&this.input.focus_handle);
                            this.start_blink(cx);
                            cx.stop_propagation();
                            cx.notify();
                        });
                        let on_clear = cx.listener(|this, _, _, cx| {
                            this.input.clear(cx);
                            this.selected_idx = 0;
                            this.filter_dirty = true;
                            cx.notify();
                        });
                        self.input.render_box(
                            TextBoxProps {
                                id: "picker-search-box".into(),
                                clear_id: "picker-search-clear".into(),
                                placeholder: "Type to filter installed fonts...",
                                leading_icon: Some("icons/search.svg"),
                                trailing_badge: Some(match_badge),
                                is_focused,
                                theme,
                            },
                            cx,
                            on_focus,
                            on_clear,
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .border_1()
                            .border_color(theme.line)
                            .rounded(px(6.0))
                            .bg(theme.bg)
                            .p(px(6.0))
                            .child(
                                uniform_list(
                                    "font-picker-list",
                                    visible_count,
                                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                        range
                                            .filter_map(|idx| {
                                                let &font_idx = this.filtered_indices.get(idx)?;
                                                let (name, path) = this.fonts.get(font_idx)?.clone();
                                                let file_name = path
                                                    .file_name()
                                                    .map(|f| f.to_string_lossy().to_string())
                                                    .unwrap_or_default();
                                                let is_row_selected = idx == selected_idx;
                                                let name_click = name.clone();
                                                let path_click = path.clone();
                                                Some(
                                                    div()
                                                        .id(("font-row", idx))
                                                        .h(px(34.0))
                                                        .w_full()
                                                        .px(px(10.0))
                                                        .gap(px(10.0))
                                                        .rounded(px(4.0))
                                                        .when(is_row_selected, |d| d.bg(theme.surface_2))
                                                        .cursor_pointer()
                                                        .hover(|s| s.bg(theme.surface_3))
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .on_click(cx.listener(move |this, _, _, cx| {
                                                            this.select_font(
                                                                name_click.clone(),
                                                                path_click.clone(),
                                                                cx,
                                                            );
                                                        }))
                                                        .child(
                                                            div()
                                                                .min_w_0()
                                                                .truncate()
                                                                .text_sm()
                                                                .text_color(theme.text)
                                                                .child(name),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .text_xs()
                                                                .text_color(theme.text_3)
                                                                .child(file_name),
                                                        ),
                                                )
                                            })
                                            .collect::<Vec<_>>()
                                    }),
                                )
                                .flex_1()
                                .min_h_0()
                                .track_scroll(self.scroll_handle.clone()),
                            )
                            .child(crate::ui::components::vertical_scrollbar(
                                &self.scroll_handle.0.borrow().base_handle,
                                "picker-scroll-bar",
                                theme,
                            )),
                    ),
            )
            .into_any_element()
    }
}
