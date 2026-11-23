use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement,
    Render, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::app_state::{AppState, FontFamilySelection, PersistedConfig, SecondaryScript};
use crate::domain::font_discovery::auto_detect_weights;
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
    selected_idx: usize,
    scroll_handle: gpui::ScrollHandle,
    state: Entity<AppState>,
}

impl FontPickerModal {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            is_open: false,
            target: FontTargetSlot::Latin,
            input: TextFieldState::new("", cx.focus_handle()),
            fonts: Vec::new(),
            selected_idx: 0,
            scroll_handle: gpui::ScrollHandle::new(),
            state,
        }
    }

    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.input.cursor_visible = true;
        self.input._blink_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(530))
                    .await;
                let keep_going = this
                    .update(cx, |modal, cx| {
                        if !modal.is_open {
                            return false;
                        }
                        modal.input.cursor_visible = !modal.input.cursor_visible;
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

    pub fn open(&mut self, target: FontTargetSlot, window: &mut Window, cx: &mut Context<Self>) {
        self.target = target;
        self.input.clear();
        self.selected_idx = 0;
        self.fonts.clear();
        self.is_open = true;
        window.focus(&self.input.focus_handle);
        self.start_blink(cx);
        cx.notify();

        cx.spawn(async move |this, cx| {
            let fonts = cx
                .background_executor()
                .spawn(async move { list_installed_fonts() })
                .await;
            let _ = this.update(cx, |modal, cx| {
                modal.fonts = fonts;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.is_open = false;
        self.input._blink_task = None;
        cx.notify();
    }

    fn filtered_fonts(&self) -> (usize, Vec<(String, PathBuf)>) {
        let query_lc = self.input.text.to_ascii_lowercase();
        let total = self
            .fonts
            .iter()
            .filter(|(name, _)| query_lc.is_empty() || name.to_ascii_lowercase().contains(&query_lc))
            .count();
        let list = self
            .fonts
            .iter()
            .filter(|(name, _)| query_lc.is_empty() || name.to_ascii_lowercase().contains(&query_lc))
            .take(150)
            .cloned()
            .collect();
        (total, list)
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
                    self.scroll_handle.scroll_to_item(self.selected_idx);
                    cx.notify();
                }
            }
            "down" => {
                let (_, filtered) = self.filtered_fonts();
                if !filtered.is_empty() && self.selected_idx + 1 < filtered.len() {
                    self.selected_idx += 1;
                    self.scroll_handle.scroll_to_item(self.selected_idx);
                    cx.notify();
                }
            }
            "enter" => {
                let (_, filtered) = self.filtered_fonts();
                if let Some((name, path)) = filtered.get(self.selected_idx).cloned() {
                    self.select_font(name, path, cx);
                }
            }
            _ => {
                if self.input.handle_key_down(event, cx) {
                    self.selected_idx = 0;
                    cx.notify();
                }
            }
        }
    }

    fn select_font(&mut self, name: String, path: PathBuf, cx: &mut Context<Self>) {
        let installed = self.fonts.clone();
        let detected = auto_detect_weights(&path, Some(&name), &installed);
        let target = self.target;

        self.state.update(cx, |s, cx| {
            *target.select_mut(&mut s.config) = detected;
            let _ = s.save_config();
            cx.notify();
        });

        self.close(cx);
    }
}

impl Render for FontPickerModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.is_open {
            return div().into_any_element();
        }

        let theme = self.state.read(cx).theme();
        let is_focused = self.input.focus_handle.is_focused(window);
        let (total_matches, filtered) = self.filtered_fonts();
        let selected_idx = self.selected_idx.min(filtered.len().saturating_sub(1));
        let match_badge = if total_matches > filtered.len() {
            format!("{}/{} matches", filtered.len(), total_matches)
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
                    .child(
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
                            cx.listener(|this, _, window, cx| {
                                window.focus(&this.input.focus_handle);
                                this.start_blink(cx);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                            cx.listener(|this, _, _, cx| {
                                this.input.clear();
                                this.selected_idx = 0;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .flex_col()
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .child(
                                div()
                                    .id("font-picker-list")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.scroll_handle)
                                    .border_1()
                                    .border_color(theme.line)
                                    .rounded(px(6.0))
                                    .bg(theme.bg)
                                    .p(px(6.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .children(filtered.into_iter().enumerate().map(|(idx, (name, path))| {
                                        let name_clone = name.clone();
                                        let path_clone = path.clone();
                                        let file_name = path
                                            .file_name()
                                            .map(|f| f.to_string_lossy().to_string())
                                            .unwrap_or_default();

                                        let is_row_selected = idx == selected_idx;
                                        div()
                                            .id(("font-row", idx))
                                            .h(px(34.0))
                                            .flex_shrink_0()
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
                                                this.select_font(name_clone.clone(), path_clone.clone(), cx);
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
                                            )
                                    })),
                            )
                            .child(crate::ui::components::vertical_scrollbar(
                                &self.scroll_handle,
                                "picker-scroll-bar",
                                theme,
                            )),
                    ),
            )
            .into_any_element()
    }
}
