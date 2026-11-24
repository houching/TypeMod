use gpui::{
    App, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Render, Styled, Window, div, px,
};

use crate::app_state::AppState;
use crate::ui::components::{ButtonVariant, VantageButton};

pub type ConfirmHandler = Box<dyn FnOnce(&mut Window, &mut App) + 'static>;

pub struct ConfirmRequest {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub dangerous: bool,
    pub on_confirm: ConfirmHandler,
    pub on_cancel: Option<ConfirmHandler>,
}

pub struct ConfirmModal {
    state: Entity<AppState>,
    pending: Option<ConfirmRequest>,
    focus_handle: FocusHandle,
}

impl ConfirmModal {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        Self {
            state,
            pending: None,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn open(&mut self, req: ConfirmRequest, window: &mut Window, cx: &mut Context<Self>) {
        self.pending = Some(req);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    pub fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(req) = self.pending.take() {
            cx.notify();
            if let Some(on_cancel) = req.on_cancel {
                on_cancel(window, cx);
            }
        }
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(req) = self.pending.take() {
            cx.notify();
            (req.on_confirm)(window, cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending.is_none() {
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.cancel(window, cx),
            "enter" => self.confirm(window, cx),
            _ => {}
        }
    }
}

impl Render for ConfirmModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(req) = &self.pending else {
            return div().into_any_element();
        };
        let theme = self.state.read(cx).theme();
        let title = req.title.clone();
        let message = req.message.clone();
        let confirm_label = req.confirm_label.clone();
        let dangerous = req.dangerous;

        div()
            .id("confirm-modal-backdrop")
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
                cx.listener(|this, _, window, cx| {
                    this.cancel(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("confirm-modal-card")
                    .occlude()
                    .w(px(440.0))
                    .max_w_full()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line_2)
                    .rounded(px(8.0))
                    .p(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme.text_2)
                            .child(message),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex_shrink_0()
                            .pt(px(4.0))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("confirm-cancel", "Cancel", theme)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.cancel(window, cx)
                                    })),
                            )
                            .child(
                                VantageButton::new("confirm-ok", confirm_label, theme)
                                    .variant(if dangerous {
                                        ButtonVariant::Danger
                                    } else {
                                        ButtonVariant::Primary
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.confirm(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}
