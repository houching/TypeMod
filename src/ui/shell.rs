use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, Image, ImageFormat, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, img, prelude::*, px, rems,
};

use crate::app_state::{AppState, NavTab, SystemFontState};
use crate::ui::components::{BadgeTone, ButtonVariant, StatusPill, VantageButton};
use crate::ui::font_picker_modal::FontPickerModal;
use crate::ui::home_view::HomeView;
use crate::ui::log_drawer::LogDrawer;
use crate::ui::preview_view::PreviewView;
use crate::ui::resizer_view::ResizerView;
use crate::ui::settings_view::SettingsView;

pub struct ShellView {
    state: Entity<AppState>,
    home: Entity<HomeView>,
    preview: Entity<PreviewView>,
    resizer: Entity<ResizerView>,
    settings: Entity<SettingsView>,
    picker: Entity<FontPickerModal>,
    log_drawer: Entity<LogDrawer>,
    ui_font_family: SharedString,
    logo_img: Arc<Image>,
    _sub: Subscription,
}

fn pick_safe_ui_font(cx: &gpui::App) -> SharedString {
    let names = cx.text_system().all_font_names();
    for candidate in [
        "Segoe UI",
        "Segoe UI Clone",
        "Arial",
        "Tahoma",
        "Microsoft Sans Serif",
    ] {
        if names.iter().any(|n| n.eq_ignore_ascii_case(candidate)) {
            return candidate.into();
        }
    }
    "Arial".into()
}

impl ShellView {
    pub fn new(state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| FontPickerModal::new(state.clone(), cx));
        let home = cx.new(|_| HomeView::new(state.clone(), picker.clone()));
        let preview = cx.new(|cx| PreviewView::new(state.clone(), cx));
        let resizer = cx.new(|_| ResizerView::new(state.clone()));
        let settings = cx.new(|_| SettingsView::new(state.clone()));
        let log_drawer = cx.new(|_| LogDrawer::new(state.clone()));
        let ui_font_family = pick_safe_ui_font(cx);
        let logo_img = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../../assets/logo/variants/segoeui-changer-symbol-app-icon-64.png")
                .to_vec(),
        ));

        let _sub = cx.observe(&state, |_, _, cx| cx.notify());

        Self {
            state,
            home,
            preview,
            resizer,
            settings,
            picker,
            log_drawer,
            ui_font_family,
            logo_img,
            _sub,
        }
    }

    fn set_tab(&self, tab: NavTab, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.active_tab = tab;
            cx.notify();
        });
    }

    fn toggle_rail(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.config.rail_pinned = !s.config.rail_pinned;
            let _ = s.save_config();
            cx.notify();
        });
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        window.set_rem_size(px(s.ui_rem_size()));
        let collapsed = !s.config.rail_pinned || window.viewport_size().width < px(760.0);
        let theme = s.theme();
        let active_tab = s.active_tab;
        let is_admin = s.is_admin;
        let sys_state = s.system_state;
        let last_error = s.last_error.clone();
        let last_notice = s.last_notice.clone();

        let sys_tone = match sys_state {
            SystemFontState::Original => BadgeTone::Ok,
            SystemFontState::Modded => BadgeTone::Accent,
            SystemFontState::PendingRestoreReboot => BadgeTone::Warn,
        };

        let breadcrumb = match active_tab {
            NavTab::Home => "Fonts > Replace & Merge",
            NavTab::Preview => "Fonts > Preview & Test",
            NavTab::Resizer => "System > Font Resizer",
            NavTab::Settings => "System > Settings",
        };

        div()
            .relative()
            .flex()
            .size_full()
            .font_family(self.ui_font_family.clone())
            .bg(theme.bg)
            .text_color(theme.text)
            // Left Navigation Rail (scales with UI rem size & collapsible)
            .child(
                div()
                    .w(if collapsed { rems(4.25) } else { rems(14.0) })
                    .flex_shrink_0()
                    .h_full()
                    .bg(theme.surface)
                    .border_r_1()
                    .border_color(theme.line)
                    .flex()
                    .flex_col()
                    .justify_between()
                    .p(px(10.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.0))
                            .child(
                                div()
                                    .py(px(4.0))
                                    .flex()
                                    .items_center()
                                    .when(!collapsed, |d| d.justify_between())
                                    .when(collapsed, |d| d.justify_center())
                                    .gap(px(8.0))
                                    .when(!collapsed, |d| {
                                        d.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .min_w_0()
                                                .child(
                                                    img(self.logo_img.clone())
                                                        .w(px(28.0))
                                                        .h(px(28.0))
                                                        .rounded(px(6.0))
                                                        .flex_shrink_0(),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .min_w_0()
                                                        .gap(px(1.0))
                                                        .child(
                                                            div()
                                                                .text_sm()
                                                                .font_weight(gpui::FontWeight::BOLD)
                                                                .text_color(theme.text)
                                                                .child("SegoeUI Changer"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.text_3)
                                                                .child("System Font Manager"),
                                                        ),
                                                ),
                                        )
                                    })
                                    .child(
                                        div()
                                            .when(collapsed, |d| d.w_full())
                                            .child(
                                                VantageButton::new("rail-toggle", "", theme)
                                                    .icon("icons/panel-left.svg")
                                                    .icon_only(true)
                                                    .variant(ButtonVariant::Ghost)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.toggle_rail(cx)
                                                    })),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        VantageButton::new("nav-home", "Font Replacer", theme)
                                            .icon("icons/type.svg")
                                            .icon_only(collapsed)
                                            .align_left(true)
                                            .variant(if active_tab == NavTab::Home {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_tab(NavTab::Home, cx)
                                            })),
                                    )
                                    .child(
                                        VantageButton::new("nav-preview", "Font Preview", theme)
                                            .icon("icons/eye.svg")
                                            .icon_only(collapsed)
                                            .align_left(true)
                                            .variant(if active_tab == NavTab::Preview {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_tab(NavTab::Preview, cx)
                                            })),
                                    )
                                    .child(
                                        VantageButton::new("nav-resizer", "Font Resizer", theme)
                                            .icon("icons/sliders-horizontal.svg")
                                            .icon_only(collapsed)
                                            .align_left(true)
                                            .variant(if active_tab == NavTab::Resizer {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_tab(NavTab::Resizer, cx)
                                            })),
                                    )
                                    .child(
                                        VantageButton::new("nav-settings", "Settings", theme)
                                            .icon("icons/settings.svg")
                                            .icon_only(collapsed)
                                            .align_left(true)
                                            .variant(if active_tab == NavTab::Settings {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Ghost
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_tab(NavTab::Settings, cx)
                                            })),
                                    ),
                            ),
                    )
                    // Bottom-left About App block
                    .child(if collapsed {
                        div()
                            .w_full()
                            .py(px(6.0))
                            .border_t_1()
                            .border_color(theme.line)
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_3)
                                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .w_full()
                            .px(px(10.0))
                            .py(px(8.0))
                            .border_t_1()
                            .border_color(theme.line)
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.text_3)
                                    .child("Version"),
                            )
                            .child(StatusPill::new(
                                format!("v{}", env!("CARGO_PKG_VERSION")),
                                BadgeTone::Neutral,
                                theme,
                            ))
                            .into_any_element()
                    }),
            )
            // Main Column (Top Bar + Banner + View + LogDrawer)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Top Bar
                    .child(
                        div()
                            .min_h(px(52.0))
                            .w_full()
                            .bg(theme.surface)
                            .border_b_1()
                            .border_color(theme.line)
                            .px(px(20.0))
                            .py(px(8.0))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(theme.text_2)
                                    .child(breadcrumb),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(StatusPill::new(
                                        if is_admin { "ADMIN" } else { "STANDARD USER" },
                                        if is_admin {
                                            BadgeTone::Ok
                                        } else {
                                            BadgeTone::Warn
                                        },
                                        theme,
                                    ))
                                    .child(StatusPill::new(sys_state.label(), sys_tone, theme)),
                            ),
                    )
                    // Error / Notice Banner
                    .when_some(last_error, |d, err| {
                        d.child(
                            div()
                                .px(px(20.0))
                                .py(px(8.0))
                                .bg(theme.err_soft)
                                .border_b_1()
                                .border_color(theme.err.opacity(0.4))
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .child(div().min_w_0().text_xs().text_color(theme.err).child(err))
                                .child(
                                    VantageButton::new("dismiss-err", "Dismiss", theme)
                                        .variant(ButtonVariant::Ghost)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.state.update(cx, |s, cx| {
                                                s.last_error = None;
                                                cx.notify();
                                            });
                                        })),
                                ),
                        )
                    })
                    .when_some(last_notice, |d, note| {
                        d.child(
                            div()
                                .px(px(20.0))
                                .py(px(8.0))
                                .bg(theme.ok_soft)
                                .border_b_1()
                                .border_color(theme.ok.opacity(0.4))
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .justify_between()
                                .gap(px(8.0))
                                .child(div().min_w_0().text_xs().text_color(theme.ok).child(note.clone()))
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(6.0))
                                        .when(note.contains("REBOOT REQUIRED"), |d| {
                                            d.child(
                                                VantageButton::new("live-refresh-btn", "Live Refresh (No Reboot)", theme)
                                                    .icon("icons/refresh-cw.svg")
                                                    .variant(ButtonVariant::Secondary)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        crate::domain::registry::live_refresh_font_cache();
                                                        this.state.update(cx, |s, cx| {
                                                            s.last_notice = Some(
                                                                "Refreshed FontCache, WM_FONTCHANGE, and Explorer. Reboot still recommended for full OS coverage.".to_string(),
                                                            );
                                                            cx.notify();
                                                        });
                                                    })),
                                            )
                                            .child(
                                                VantageButton::new("reboot-now-btn", "Reboot Now", theme)
                                                    .variant(ButtonVariant::Primary)
                                                    .on_click(|_, _, cx| {
                                                        cx.spawn(async move |_cx| {
                                                            let ok = rfd::MessageDialog::new()
                                                                .set_level(rfd::MessageLevel::Warning)
                                                                .set_title("Reboot System")
                                                                .set_description(
                                                                    "Save any open work before restarting. Reboot Windows now?",
                                                                )
                                                                .set_buttons(rfd::MessageButtons::YesNo)
                                                                .show()
                                                                == rfd::MessageDialogResult::Yes;
                                                            if ok {
                                                                let _ = std::process::Command::new("shutdown")
                                                                    .args(["/r", "/t", "3"])
                                                                    .spawn();
                                                            }
                                                        })
                                                        .detach();
                                                    }),
                                            )
                                        })
                                        .child(
                                            VantageButton::new("dismiss-note", "Dismiss", theme)
                                                .variant(ButtonVariant::Ghost)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.state.update(cx, |s, cx| {
                                                        s.last_notice = None;
                                                        cx.notify();
                                                    });
                                                })),
                                        ),
                                ),
                        )
                    })
                    // Active Tab View
                    .child(match active_tab {
                        NavTab::Home => self.home.clone().into_any_element(),
                        NavTab::Preview => self.preview.clone().into_any_element(),
                        NavTab::Resizer => self.resizer.clone().into_any_element(),
                        NavTab::Settings => self.settings.clone().into_any_element(),
                    })
                    // Bottom Log Drawer
                    .child(self.log_drawer.clone()),
            )
            // Modal Overlay
            .child(self.picker.clone())
    }
}
