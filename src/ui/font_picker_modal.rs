use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::app_state::AppState;
use crate::domain::font_discovery::auto_detect_weights;
use crate::domain::registry::list_installed_fonts;
use crate::ui::components::{ButtonVariant, VantageButton};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontTargetSlot {
    Latin,
    Khmer,
    LatinItalic,
    KhmerItalic,
}

impl FontTargetSlot {
    pub fn title(self) -> &'static str {
        match self {
            Self::Latin => "Primary English / Latin Font",
            Self::Khmer => "Khmer Base Font (Optional)",
            Self::LatinItalic => "Latin Italic Font",
            Self::KhmerItalic => "Khmer Italic Font",
        }
    }
}

pub struct FontPickerModal {
    pub is_open: bool,
    pub target: FontTargetSlot,
    pub query: String,
    pub fonts: Vec<(String, PathBuf)>,
    pub focus_handle: FocusHandle,
    state: Entity<AppState>,
}

impl FontPickerModal {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            is_open: false,
            target: FontTargetSlot::Latin,
            query: String::new(),
            fonts: Vec::new(),
            focus_handle: cx.focus_handle(),
            state,
        }
    }

    pub fn open(&mut self, target: FontTargetSlot, window: &mut Window, cx: &mut Context<Self>) {
        self.target = target;
        self.query.clear();
        if self.fonts.is_empty() {
            self.fonts = list_installed_fonts();
        }
        self.is_open = true;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.is_open = false;
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
            "backspace" => {
                self.query.pop();
                cx.notify();
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt
                {
                    self.query.push_str(ch);
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
            let slot = match target {
                FontTargetSlot::Latin => &mut s.config.latin,
                FontTargetSlot::Khmer => &mut s.config.khmer,
                FontTargetSlot::LatinItalic => &mut s.config.latin_italic,
                FontTargetSlot::KhmerItalic => &mut s.config.khmer_italic,
            };
            *slot = detected;
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
        let is_focused = self.focus_handle.is_focused(window);
        let query_lc = self.query.to_ascii_lowercase();
        let filtered: Vec<(String, PathBuf)> = self
            .fonts
            .iter()
            .filter(|(name, _)| query_lc.is_empty() || name.to_ascii_lowercase().contains(&query_lc))
            .take(120)
            .cloned()
            .collect();

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
            .track_focus(&self.focus_handle)
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
                        div()
                            .id("picker-search-box")
                            .h(px(36.0))
                            .px(px(10.0))
                            .rounded(px(6.0))
                            .bg(theme.surface_2)
                            .border_1()
                            .border_color(if is_focused { theme.accent } else { theme.line_2 })
                            .cursor_text()
                            .flex()
                            .items_center()
                            .justify_between()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    window.focus(&this.focus_handle);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_sm()
                                    .text_color(if self.query.is_empty() {
                                        theme.text_3
                                    } else {
                                        theme.text
                                    })
                                    .child(
                                        gpui::svg()
                                            .path("icons/search.svg")
                                            .size(px(14.0))
                                            .text_color(theme.text_3),
                                    )
                                    .child(if self.query.is_empty() {
                                        "Type to filter installed fonts...".to_string()
                                    } else {
                                        self.query.clone()
                                    })
                                    .when(is_focused, |d| {
                                        d.child(
                                            div()
                                                .w(px(1.5))
                                                .h(px(14.0))
                                                .ml(px(2.0))
                                                .bg(theme.accent),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_3)
                                    .child(format!("{} matches", filtered.len())),
                            ),
                    )
                    .child(
                        div()
                            .id("font-picker-list")
                            .flex_1()
                            .overflow_y_scroll()
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

                                div()
                                    .id(("font-row", idx))
                                    .h(px(34.0))
                                    .px(px(10.0))
                                    .rounded(px(4.0))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme.surface_3))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_font(name_clone.clone(), path_clone.clone(), cx);
                                    }))
                                    .child(div().text_sm().text_color(theme.text).child(name))
                                    .child(div().text_xs().text_color(theme.text_3).child(file_name))
                            })),
                    ),
            )
            .into_any_element()
    }
}
