use gpui::prelude::FluentBuilder;
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px, svg,
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
    theme: VantageTheme,
    on_click: Option<ClickHandler>,
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
                d.child(svg().path(icon_path).size(px(15.0)).text_color(fg).flex_shrink_0())
            })
            .when(!self.icon_only, |d| d.child(self.label));

        if self.disabled {
            el = el.opacity(0.45);
        } else {
            el = el.cursor_pointer().hover(move |s| s.bg(hover_bg));
            if let Some(handler) = self.on_click {
                el = el.on_click(handler);
            }
        }

        el
    }
}
