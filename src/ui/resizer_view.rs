use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px,
};

use crate::app_state::{AppState, SystemFontSizes};
use crate::domain::resizer;
use crate::ui::components::{ButtonVariant, StatusPill, BadgeTone, VantageButton};

pub struct ResizerView {
    state: Entity<AppState>,
}

impl ResizerView {
    pub fn new(state: Entity<AppState>) -> Self {
        Self { state }
    }

    fn refresh_metrics(&self, cx: &mut Context<Self>) {
        match resizer::get_current_metrics() {
            Ok(m) => {
                self.state.update(cx, |s, cx| {
                    s.uniform_pt = m.message;
                    s.sizes = m;
                    s.last_error = None;
                    s.last_notice = Some("Refreshed current Windows font metrics.".to_string());
                    cx.notify();
                });
            }
            Err(e) => {
                self.state.update(cx, |s, cx| {
                    s.last_error = Some(format!("Failed to read metrics: {e}"));
                    cx.notify();
                });
            }
        }
    }

    fn adjust_uniform(&self, delta: i32, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.uniform_pt = (s.uniform_pt + delta).clamp(6, 24);
            cx.notify();
        });
    }

    fn adjust_field(&self, field: &'static str, delta: i32, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            let val = match field {
                "caption" => &mut s.sizes.caption,
                "sm_caption" => &mut s.sizes.sm_caption,
                "menu" => &mut s.sizes.menu,
                "status" => &mut s.sizes.status,
                "message" => &mut s.sizes.message,
                _ => &mut s.sizes.icon,
            };
            *val = (*val + delta).clamp(6, 24);
            cx.notify();
        });
    }

    fn apply_uniform(&self, cx: &mut Context<Self>) {
        let pt = self.state.read(cx).uniform_pt;
        match resizer::apply_entire_size(pt) {
            Ok(updated) => {
                self.state.update(cx, |s, cx| {
                    s.sizes = updated;
                    s.last_error = None;
                    s.last_notice = Some(format!("Applied {pt} pt to all system UI elements."));
                    cx.notify();
                });
            }
            Err(e) => {
                self.state.update(cx, |s, cx| {
                    s.last_error = Some(format!("Failed to apply uniform size: {e}"));
                    cx.notify();
                });
            }
        }
    }

    fn apply_individual(&self, cx: &mut Context<Self>) {
        let sizes: SystemFontSizes = self.state.read(cx).sizes.clone();
        match resizer::apply_system_metrics(&sizes) {
            Ok(()) => {
                self.state.update(cx, |s, cx| {
                    s.last_error = None;
                    s.last_notice = Some("Applied individual system font metrics.".to_string());
                    cx.notify();
                });
            }
            Err(e) => {
                self.state.update(cx, |s, cx| {
                    s.last_error = Some(format!("Failed to apply metrics: {e}"));
                    cx.notify();
                });
            }
        }
    }
}

impl Render for ResizerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let theme = s.theme();
        let sizes = s.sizes.clone();
        let uniform_pt = s.uniform_pt;

        let rows: [(&'static str, &'static str, i32); 6] = [
            ("caption", "Window Title Bar (Caption)", sizes.caption),
            ("sm_caption", "Palette / Tool Window Caption", sizes.sm_caption),
            ("menu", "Menu Bar & Context Menus", sizes.menu),
            ("status", "Status Bars & Tooltips", sizes.status),
            ("message", "Message Boxes & Dialogs", sizes.message),
            ("icon", "Desktop & Explorer Icons", sizes.icon),
        ];

        div()
            .id("resizer-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
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
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("System Font Resizer"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_2)
                                    .child("Modify Windows NONCLIENTMETRICSW & ICONMETRICS via SystemParametersInfoW"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("resizer-reset-stock", "Reset 9 pt", theme)
                                    .icon("icons/rotate-ccw.svg")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |s, cx| {
                                            s.uniform_pt = 9;
                                            cx.notify();
                                        });
                                        this.apply_uniform(cx);
                                    })),
                            )
                            .child(
                                VantageButton::new("resizer-refresh", "Read Current", theme)
                                    .icon("icons/refresh-cw.svg")
                                    .on_click(cx.listener(|this, _, _, cx| this.refresh_metrics(cx))),
                            ),
                    ),
            )
            // Uniform Card
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(180.0))
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Uniform System Size"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_3)
                                    .child("Windows default is 9 pt across all UI elements."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("uni-dec", "−", theme)
                                    .on_click(cx.listener(|this, _, _, cx| this.adjust_uniform(-1, cx))),
                            )
                            .child(StatusPill::new(format!("{uniform_pt} pt"), BadgeTone::Accent, theme))
                            .child(
                                VantageButton::new("uni-inc", "+", theme)
                                    .on_click(cx.listener(|this, _, _, cx| this.adjust_uniform(1, cx))),
                            )
                            .child(
                                VantageButton::new("uni-apply", "Apply to All", theme)
                                    .variant(ButtonVariant::Primary)
                                    .on_click(cx.listener(|this, _, _, cx| this.apply_uniform(cx))),
                            ),
                    ),
            )
            // Individual Metrics Card
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child("Per-Element Metrics"),
                            )
                            .child(
                                VantageButton::new("ind-apply", "Apply Individual", theme)
                                    .variant(ButtonVariant::Primary)
                                    .on_click(cx.listener(|this, _, _, cx| this.apply_individual(cx))),
                            ),
                    )
                    .children(rows.into_iter().map(|(key, label, val)| {
                        div()
                            .min_h(px(44.0))
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(6.0))
                            .bg(theme.surface_2)
                            .border_1()
                            .border_color(theme.line)
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(div().text_sm().text_color(theme.text).child(label))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        VantageButton::new(
                                            gpui::SharedString::from(format!("dec-{key}")),
                                            "−",
                                            theme,
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.adjust_field(key, -1, cx)
                                        })),
                                    )
                                    .child(StatusPill::new(format!("{val} pt"), BadgeTone::Neutral, theme))
                                    .child(
                                        VantageButton::new(
                                            gpui::SharedString::from(format!("inc-{key}")),
                                            "+",
                                            theme,
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.adjust_field(key, 1, cx)
                                        })),
                                    ),
                            )
                    })),
            )
    }
}
