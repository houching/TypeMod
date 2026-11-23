use gpui::{
    App, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    ParentElement, Render, Styled, Window, div, px,
};

use crate::app_state::AppState;
use crate::domain::elevation::relaunch_elevated;
use crate::ui::components::{ButtonVariant, VantageButton};

pub type ConfirmHandler = Box<dyn FnOnce(&mut Window, &mut App) + 'static>;

pub struct ConfirmRequest {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
    pub dangerous: bool,
    pub on_confirm: ConfirmHandler,
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

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        cx.notify();
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
            "escape" => self.close(cx),
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
                cx.listener(|this, _, _, cx| {
                    this.close(cx);
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .id("confirm-modal-card")
                    .occlude()
                    .w_full()
                    .max_w(px(440.0))
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
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_2)
                            .child(message),
                    )
                    .child(
                        div()
                            .pt(px(4.0))
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("confirm-cancel", "Cancel", theme)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
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

/// Shared UAC elevation prompt + relaunch helper used across views.
pub fn prompt_elevated_relaunch(
    state: Entity<AppState>,
    confirm_modal: &Entity<ConfirmModal>,
    description: impl Into<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let state_path_res = state.read(cx).save_temp_state();
    let state_clone = state.clone();
    confirm_modal.update(cx, |m, cx| {
        m.open(
            ConfirmRequest {
                title: "Administrator Privileges Required".to_string(),
                message: description.into(),
                confirm_label: "Relaunch Elevated".to_string(),
                dangerous: false,
                on_confirm: Box::new(move |_, cx| {
                    if let Ok(state_path) = state_path_res {
                        match relaunch_elevated(&state_path) {
                            Ok(true) => std::process::exit(0),
                            Ok(false) => {}
                            Err(e) => {
                                state_clone.update(cx, |s, cx| {
                                    s.last_error = Some(format!("Elevation failed: {e}"));
                                    cx.notify();
                                });
                            }
                        }
                    }
                }),
            },
            window,
            cx,
        );
    });
}
