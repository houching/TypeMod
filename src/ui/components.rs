use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
};

use crate::ui::theme::VantageTheme;

#[derive(Clone, Copy)]
pub enum BadgeTone {
    Neutral,
    Accent,
    Ok,
    Warn,
    Err,
}

#[derive(IntoElement)]
pub struct StatusPill {
    label: SharedString,
    tone: BadgeTone,
    theme: VantageTheme,
}

impl StatusPill {
    pub fn new(label: impl Into<SharedString>, tone: BadgeTone, theme: VantageTheme) -> Self {
        Self {
            label: label.into(),
            tone,
            theme,
        }
    }
}

impl RenderOnce for StatusPill {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (bg, fg) = match self.tone {
            BadgeTone::Neutral => (self.theme.surface_3, self.theme.text_2),
            BadgeTone::Accent => (self.theme.accent_soft, self.theme.accent),
            BadgeTone::Ok => (self.theme.ok_soft, self.theme.ok),
            BadgeTone::Warn => (self.theme.warn_soft, self.theme.warn),
            BadgeTone::Err => (self.theme.err_soft, self.theme.err),
        };

        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(px(4.0))
            .bg(bg)
            .text_color(fg)
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(self.label)
    }
}

#[derive(Clone, Copy)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

#[derive(IntoElement)]
pub struct VantageButton {
    id: ElementId,
    label: SharedString,
    icon: Option<SharedString>,
    icon_only: bool,
    align_left: bool,
    variant: ButtonVariant,
    disabled: bool,
    tooltip_text: Option<SharedString>,
    theme: VantageTheme,
    on_click: Option<ClickHandler>,
}

struct SimpleTooltip {
    text: SharedString,
    theme: VantageTheme,
}

impl gpui::Render for SimpleTooltip {
    fn render(
        &mut self,
        _window: &mut Window,
        _cx: &mut gpui::Context<'_, SimpleTooltip>,
    ) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .bg(self.theme.surface_3)
            .border_1()
            .border_color(self.theme.line_2)
            .text_xs()
            .text_color(self.theme.text)
            .child(self.text.clone())
    }
}

impl VantageButton {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        theme: VantageTheme,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            icon_only: false,
            align_left: false,
            variant: ButtonVariant::Secondary,
            disabled: false,
            tooltip_text: None,
            theme,
            on_click: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }

    pub fn align_left(mut self, align_left: bool) -> Self {
        self.align_left = align_left;
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip_text = Some(text.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for VantageButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (bg, fg, border, hover_bg) = match self.variant {
            ButtonVariant::Primary => (
                self.theme.accent,
                self.theme.accent_ink,
                self.theme.accent,
                self.theme.accent.opacity(0.88),
            ),
            ButtonVariant::Secondary => (
                self.theme.surface_2,
                self.theme.text,
                self.theme.line_2,
                self.theme.surface_3,
            ),
            ButtonVariant::Danger => (
                self.theme.err_soft,
                self.theme.err,
                self.theme.err.opacity(0.45),
                self.theme.err.opacity(0.22),
            ),
            ButtonVariant::Ghost => (
                gpui::transparent_black(),
                self.theme.text_2,
                gpui::transparent_black(),
                self.theme.surface_3,
            ),
        };

        let mut el = div()
            .id(self.id)
            .h(px(36.0))
            .px(px(if self.icon_only { 8.0 } else { 12.0 }))
            .rounded(px(6.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_color(fg)
            .text_sm()
            .font_weight(gpui::FontWeight::MEDIUM)
            .flex()
            .items_center()
            .when(self.align_left && !self.icon_only, |d| d.justify_start())
            .when(!self.align_left || self.icon_only, |d| d.justify_center())
            .gap(px(8.0))
            .when_some(self.icon, |d, icon_path| {
                d.child(
                    div()
                        .size(px(15.0))
                        .when(!self.icon_only, |d| d.mb(px(1.5)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_shrink_0()
                        .child(svg().path(icon_path).size(px(15.0)).text_color(fg)),
                )
            })
            .when(!self.icon_only, |d| {
                d.child(div().line_height(px(15.0)).child(self.label))
            });

        if self.disabled {
            el = el.opacity(0.45);
        } else {
            el = el.cursor_pointer().hover(move |s| s.bg(hover_bg));
            if let Some(handler) = self.on_click {
                el = el.on_click(handler);
            }
        }

        if let Some(tip) = self.tooltip_text {
            let tip_theme = self.theme;
            el = el.tooltip(move |_window, cx| {
                cx.new(|_| SimpleTooltip {
                    text: tip.clone(),
                    theme: tip_theme,
                })
                .into()
            });
        }

        el
    }
}

#[derive(IntoElement)]
pub struct ToggleSwitch {
    id: ElementId,
    checked: bool,
    theme: VantageTheme,
    on_click: Option<ClickHandler>,
}

impl ToggleSwitch {
    pub fn new(id: impl Into<ElementId>, checked: bool, theme: VantageTheme) -> Self {
        Self {
            id: id.into(),
            checked,
            theme,
            on_click: None,
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for ToggleSwitch {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let track_bg = if self.checked {
            self.theme.accent
        } else {
            self.theme.surface_3
        };
        let border_col = if self.checked {
            self.theme.accent
        } else {
            self.theme.line_2
        };
        let knob_left = if self.checked { px(20.0) } else { px(2.0) };
        let knob_bg = if self.checked {
            self.theme.accent_ink
        } else {
            self.theme.text_2
        };

        let mut el = div()
            .id(self.id)
            .w(px(40.0))
            .h(px(22.0))
            .rounded(px(11.0))
            .bg(track_bg)
            .border_1()
            .border_color(border_col)
            .relative()
            .cursor_pointer()
            .flex_shrink_0()
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(knob_left)
                    .size(px(16.0))
                    .rounded(px(8.0))
                    .bg(knob_bg),
            );
        if let Some(handler) = self.on_click {
            el = el.on_click(handler);
        }
        el
    }
}

pub fn vertical_scrollbar(
    handle: &gpui::ScrollHandle,
    id: impl Into<gpui::ElementId>,
    theme: VantageTheme,
) -> gpui::AnyElement {
    let handle_click = handle.clone();
    let handle_paint = handle.clone();

    div()
        .id(id)
        .absolute()
        .top(px(4.0))
        .bottom(px(4.0))
        .right(px(3.0))
        .w(px(6.0))
        .cursor_pointer()
        .on_mouse_down(gpui::MouseButton::Left, move |ev, _window, _cx| {
            let max_y: f32 = handle_click.max_offset().height.into();
            if max_y <= 1.0 {
                return;
            }
            let bounds = handle_click.bounds();
            let view_h: f32 = bounds.size.height.into();
            let view_h = if view_h > 1.0 { view_h } else { 400.0 };
            let track_top: f32 = bounds.top().into();
            let click_y: f32 = ev.position.y.into();
            let ratio = ((click_y - track_top) / view_h).clamp(0.0, 1.0);
            let target_y = (ratio * max_y).clamp(0.0, max_y);
            handle_click.set_offset(gpui::Point::new(px(0.0), px(-target_y)));
        })
        .child(
            gpui::canvas(
                |_, _, _| {},
                move |track_bounds, _, window, _cx| {
                    let max_y: f32 = handle_paint.max_offset().height.into();
                    if max_y <= 1.0 {
                        return;
                    }
                    let view_h: f32 = handle_paint.bounds().size.height.into();
                    let view_h = if view_h > 1.0 {
                        view_h
                    } else {
                        f32::from(track_bounds.size.height).max(1.0)
                    };
                    let content_h = view_h + max_y;
                    let thumb_frac = (view_h / content_h).clamp(0.10, 1.0);
                    let scroll_y = (-f32::from(handle_paint.offset().y)).clamp(0.0, max_y);
                    let top_frac = (scroll_y / max_y) * (1.0 - thumb_frac);

                    window.paint_quad(gpui::quad(
                        track_bounds,
                        gpui::Corners::all(px(3.0)),
                        theme.line.opacity(0.35),
                        gpui::Edges::default(),
                        gpui::transparent_black(),
                        gpui::BorderStyle::Solid,
                    ));

                    let thumb_h = track_bounds.size.height * thumb_frac;
                    let thumb_y = track_bounds.origin.y + track_bounds.size.height * top_frac;
                    let thumb_bounds = gpui::Bounds::new(
                        gpui::Point::new(track_bounds.origin.x, thumb_y),
                        gpui::Size::new(track_bounds.size.width, thumb_h),
                    );
                    window.paint_quad(gpui::quad(
                        thumb_bounds,
                        gpui::Corners::all(px(3.0)),
                        theme.text_3.opacity(0.65),
                        gpui::Edges::default(),
                        gpui::transparent_black(),
                        gpui::BorderStyle::Solid,
                    ));
                },
            )
            .size_full(),
        )
        .into_any_element()
}
