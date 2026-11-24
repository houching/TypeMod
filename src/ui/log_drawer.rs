use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, UniformListScrollHandle, Window,
    div, px, uniform_list,
};

use crate::app_state::AppState;
use crate::ui::components::{ButtonVariant, VantageButton};

pub struct LogDrawer {
    state: Entity<AppState>,
    copied: bool,
    scroll_handle: UniformListScrollHandle,
}

impl LogDrawer {
    pub fn new(state: Entity<AppState>) -> Self {
        Self {
            state,
            copied: false,
            scroll_handle: UniformListScrollHandle::new(),
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
        let line_count = s.build_log.len();

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
                                            s.save_config_async(cx.background_executor());
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
                    .bg(theme.bg)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(6.0))
                    .p(px(8.0))
                    .child(if line_count == 0 {
                        div()
                            .text_xs()
                            .text_color(theme.text_3)
                            .child("No build output yet.")
                            .into_any_element()
                    } else {
                        uniform_list(
                            "log-scroll-list",
                            line_count,
                            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                                let log = &this.state.read(cx).build_log;
                                range
                                    .filter_map(|idx| {
                                        let line = log.get(idx)?.clone();
                                        Some(
                                            div()
                                                .h(px(18.0))
                                                .flex()
                                                .items_center()
                                                .text_xs()
                                                .text_color(theme.text_2)
                                                .truncate()
                                                .child(line),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .flex_1()
                        .min_h_0()
                        .track_scroll(self.scroll_handle.clone())
                        .into_any_element()
                    })
                    .child(crate::ui::components::vertical_scrollbar(
                        &self.scroll_handle.0.borrow().base_handle,
                        "log-scroll-bar",
                        theme,
                    )),
            )
            .into_any_element()
    }
}
