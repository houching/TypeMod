use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, ElementId, FocusHandle, InteractiveElement, KeyDownEvent, MouseButton,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};
use unicode_segmentation::UnicodeSegmentation;

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

    pub fn move_left(&mut self) -> bool {
        self.clamp_cursor();
        if self.cursor == 0 {
            return false;
        }
        let prev = self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.cursor = prev;
        self.cursor_visible = true;
        true
    }

    pub fn move_right(&mut self) -> bool {
        self.clamp_cursor();
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = self.text[self.cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len());
        self.cursor = next;
        self.cursor_visible = true;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        self.clamp_cursor();
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = self.text[self.cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len());
        self.text.drain(self.cursor..next);
        self.cursor_visible = true;
        true
    }

    pub fn backspace(&mut self, by_word: bool) -> bool {
        self.clamp_cursor();
        if self.cursor == 0 {
            return false;
        }
        if by_word {
            let is_sep = |c: char| c.is_whitespace() || c == '\u{200B}';
            let mut start = self.cursor;
            for (idx, ch) in self.text[..self.cursor].char_indices().rev() {
                if !is_sep(ch) {
                    break;
                }
                start = idx;
            }
            for (idx, ch) in self.text[..start].char_indices().rev() {
                if is_sep(ch) {
                    break;
                }
                start = idx;
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
            "left" => self.move_left(),
            "right" => self.move_right(),
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
            "backspace" => self.backspace(mods.control),
            "delete" => self.delete_forward(),
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

        let caret = |visible: bool| {
            div()
                .w(px(1.5))
                .h(px(15.0))
                .bg(if visible {
                    theme.accent
                } else {
                    gpui::transparent_black()
                })
                .flex_shrink_0()
        };

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
                            .when(is_focused, |d| d.child(caret(show_cursor).mr(px(2.0))))
                            .child(SharedString::from(placeholder.to_string()))
                    } else {
                        let visible_before = if is_focused {
                            truncate_before_graphemes(before, 48)
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
                            .when(is_focused, |d| d.child(caret(show_cursor)))
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

fn truncate_before_graphemes(before: &str, max_graphemes: usize) -> String {
    let count = before.grapheme_indices(true).count();
    if count > max_graphemes {
        let skip = count - max_graphemes;
        let byte_idx = before
            .grapheme_indices(true)
            .nth(skip)
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("…{}", &before[byte_idx..])
    } else {
        before.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_before_graphemes;

    #[test]
    fn grapheme_truncation_keeps_khmer_clusters_intact() {
        // "ខ្ញុំ" is 5 Unicode scalars forming 1 Khmer grapheme cluster
        let cluster = "ខ្ញុំ";
        let text = format!("{cluster}abc");
        let truncated = truncate_before_graphemes(&text, 3);
        assert_eq!(truncated, "…abc");

        let exact = truncate_before_graphemes(cluster, 1);
        assert_eq!(exact, cluster);
    }
}
