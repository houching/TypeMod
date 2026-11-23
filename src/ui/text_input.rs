use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, ElementId, FocusHandle, InteractiveElement, KeyDownEvent, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::ui::theme::VantageTheme;

pub struct TextFieldState {
    pub text: String,
    pub cursor: usize,
    pub focus_handle: FocusHandle,
    pub cursor_visible: bool,
    pub _blink_task: Option<gpui::Task<()>>,
}

impl TextFieldState {
    pub fn new(initial: impl Into<String>, focus_handle: FocusHandle) -> Self {
        let text = initial.into();
        let cursor = text.len();
        Self {
            text,
            cursor,
            focus_handle,
            cursor_visible: true,
            _blink_task: None,
        }
    }

    pub fn set_text(&mut self, value: impl Into<String>) {
        self.text = value.into();
        self.cursor = self.text.len();
        self.cursor_visible = true;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.cursor_visible = true;
    }

    fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.min(self.text.len());
        while self.cursor > 0 && !self.text.is_char_boundary(self.cursor) {
            self.cursor -= 1;
        }
    }

    /// Handles common text editing keys. Returns `true` if text or cursor changed.
    pub fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &App) -> bool {
        self.clamp_cursor();
        let mods = &event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();

        if (mods.control || mods.platform) && key.eq_ignore_ascii_case("v") {
            if let Some(clip) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let sanitized = clip.replace(['\r', '\n'], " ");
                self.text.insert_str(self.cursor, &sanitized);
                self.cursor += sanitized.len();
                self.cursor_visible = true;
                return true;
            }
            return false;
        }

        match key {
            "left" => {
                if self.cursor > 0 {
                    let prev = self.text[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.cursor = prev;
                    self.cursor_visible = true;
                    return true;
                }
                false
            }
            "right" => {
                if self.cursor < self.text.len() {
                    let next = self.text[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.text.len());
                    self.cursor = next;
                    self.cursor_visible = true;
                    return true;
                }
                false
            }
            "home" => {
                self.cursor = 0;
                self.cursor_visible = true;
                true
            }
            "end" => {
                self.cursor = self.text.len();
                self.cursor_visible = true;
                true
            }
            "backspace" => {
                if self.cursor == 0 {
                    return false;
                }
                if mods.control {
                    let mut start = self.cursor;
                    let bytes = self.text.as_bytes();
                    while start > 0 && bytes[start - 1].is_ascii_whitespace() {
                        start -= 1;
                    }
                    while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
                        start -= 1;
                    }
                    while start > 0 && !self.text.is_char_boundary(start) {
                        start -= 1;
                    }
                    self.text.drain(start..self.cursor);
                    self.cursor = start;
                } else {
                    let prev = self.text[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.text.drain(prev..self.cursor);
                    self.cursor = prev;
                }
                self.cursor_visible = true;
                true
            }
            "delete" => {
                if self.cursor >= self.text.len() {
                    return false;
                }
                let next = self.text[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| self.cursor + i)
                    .unwrap_or(self.text.len());
                self.text.drain(self.cursor..next);
                self.cursor_visible = true;
                true
            }
            _ => {
                if let Some(ch) = &event.keystroke.key_char
                    && !mods.control
                    && !mods.alt
                    && !ch.chars().all(|c| c.is_control())
                {
                    self.text.insert_str(self.cursor, ch);
                    self.cursor += ch.len();
                    self.cursor_visible = true;
                    return true;
                }
                false
            }
        }
    }
}

pub struct TextBoxProps<'a> {
    pub id: ElementId,
    pub clear_id: ElementId,
    pub placeholder: &'a str,
    pub leading_icon: Option<&'static str>,
    pub trailing_badge: Option<String>,
    pub is_focused: bool,
    pub theme: VantageTheme,
}

impl TextFieldState {
    pub fn render_box(
        &self,
        props: TextBoxProps<'_>,
        on_focus: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
        on_clear: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let TextBoxProps {
            id,
            clear_id,
            placeholder,
            leading_icon,
            trailing_badge,
            is_focused,
            theme,
        } = props;
        let show_cursor = is_focused && self.cursor_visible;
        let safe_cursor = self.cursor.min(self.text.len());
        let (before, after) = if self.text.is_empty() {
            ("", "")
        } else if self.text.is_char_boundary(safe_cursor) {
            (&self.text[..safe_cursor], &self.text[safe_cursor..])
        } else {
            (self.text.as_str(), "")
        };

        let caret = || div().w(px(1.5)).h(px(15.0)).bg(theme.accent).flex_shrink_0();

        div()
            .id(id)
            .min_h(px(36.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .bg(theme.surface_2)
            .border_1()
            .border_color(if is_focused { theme.accent } else { theme.line_2 })
            .cursor_text()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .on_mouse_down(MouseButton::Left, on_focus)
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .flex_1()
                    .overflow_hidden()
                    .gap(px(8.0))
                    .text_sm()
                    .text_color(if self.text.is_empty() {
                        theme.text_3
                    } else {
                        theme.text
                    })
                    .when_some(leading_icon, |d, icon_path| {
                        d.child(
                            svg()
                                .path(icon_path)
                                .size(px(14.0))
                                .text_color(theme.text_3)
                                .flex_shrink_0(),
                        )
                    })
                    .child(if self.text.is_empty() {
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .when(show_cursor, |d| d.child(caret().mr(px(2.0))))
                            .child(SharedString::from(placeholder.to_string()))
                    } else {
                        let visible_before = if is_focused {
                            let count = before.chars().count();
                            if count > 48 {
                                let skip = count - 48;
                                let byte_idx = before
                                    .char_indices()
                                    .nth(skip)
                                    .map(|(i, _)| i)
                                    .unwrap_or(0);
                                format!("…{}", &before[byte_idx..])
                            } else {
                                before.to_string()
                            }
                        } else {
                            before.to_string()
                        };
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .when(!visible_before.is_empty(), |d| {
                                d.child(SharedString::from(visible_before))
                            })
                            .when(show_cursor, |d| d.child(caret().flex_shrink_0()))
                            .when(!after.is_empty(), |d| {
                                d.child(SharedString::from(after.to_string()))
                            })
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .flex_shrink_0()
                    .when_some(trailing_badge, |d, badge| {
                        d.child(div().text_xs().text_color(theme.text_3).child(badge))
                    })
                    .when(!self.text.is_empty(), |d| {
                        d.child(
                            div()
                                .id(clear_id)
                                .size(px(20.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.surface_3))
                                .flex()
                                .items_center()
                                .justify_center()
                                .on_click(on_clear)
                                .child(svg().path("icons/x.svg").size(px(12.0)).text_color(theme.text_3)),
                        )
                    }),
            )
    }
}
