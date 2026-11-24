use gpui::{
    Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::app_state::AppState;
use crate::ui::components::{ButtonVariant, VantageButton};

pub struct LogDrawer {
    state: Entity<AppState>,
    copied: bool,
    scroll_handle: gpui::ScrollHandle,
}

impl LogDrawer {
    pub fn new(state: Entity<AppState>) -> Self {
        Self {
            state,
            copied: false,
            scroll_handle: gpui::ScrollHandle::new(),
        }
    }
}

impl Render for LogDrawer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        if !s.config.show_log_drawer {
            return div().into_any_element();
        }
        let theme = s.theme();
        let lines: Vec<gpui::SharedString> =
            s.build_log.iter().rev().take(200).rev().cloned().collect();

        div()
            .h(px(190.0))
            .w_full()
            .bg(theme.surface)
            .border_t_1()
            .border_color(theme.line)
            .flex()
            .flex_col()
            .p(px(10.0))
            .gap(px(6.0))
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
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_2)
                            .child(format!("BUILD LOG ({} LINES)", s.build_log.len())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(
                                VantageButton::new(
                                    "copy-log",
                                    if self.copied { "Copied!" } else { "Copy" },
                                    theme,
                                )
                                .icon("icons/copy.svg")
                                .variant(ButtonVariant::Ghost)
                                .disabled(s.build_log.is_empty())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let text: String = this
                                        .state
                                        .read(cx)
                                        .build_log
                                        .iter()
                                        .map(|l| l.as_ref())
                                        .collect::<Vec<&str>>()
                                        .join("\n");
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
                                    this.copied = true;
                                    cx.notify();
                                    cx.spawn(async move |view, cx| {
                                        cx.background_executor()
                                            .timer(std::time::Duration::from_millis(1500))
                                            .await;
                                        let _ = view.update(cx, |drawer, cx| {
                                            drawer.copied = false;
                                            cx.notify();
                                        });
                                    })
                                    .detach();
                                })),
                            )
                            .child(
                                VantageButton::new("clear-log", "Clear", theme)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |s, cx| {
                                            s.build_log.clear();
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                VantageButton::new("hide-log", "Hide", theme)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |s, cx| {
                                            s.config.show_log_drawer = false;
                                            let _ = s.save_config();
                                            cx.notify();
                                        });
                                    })),
                            ),
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
                            .id("log-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll_handle)
                            .bg(theme.bg)
                            .border_1()
                            .border_color(theme.line)
                            .rounded(px(6.0))
                            .p(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .children(if lines.is_empty() {
                                vec![
                                    div()
                                        .text_xs()
                                        .text_color(theme.text_3)
                                        .child("No build output yet.")
                                        .into_any_element(),
                                ]
                            } else {
                                lines
                                    .into_iter()
                                    .map(|line| {
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_2)
                                            .child(line)
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                    )
                    .child(crate::ui::components::vertical_scrollbar(
                        &self.scroll_handle,
                        "log-scroll-bar",
                        theme,
                    )),
            )
            .into_any_element()
    }
}
