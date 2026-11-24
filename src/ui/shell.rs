use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    Context, Entity, Image, ImageFormat, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window, div, img, prelude::*, px, rems,
};

use crate::app_state::{AppState, NavTab, SystemFontState};
use crate::ui::components::{BadgeTone, ButtonVariant, StatusPill, VantageButton};
use crate::ui::confirm_modal::{ConfirmModal, ConfirmRequest};
use crate::ui::font_picker_modal::FontPickerModal;
use crate::ui::home_view::HomeView;
use crate::ui::log_drawer::LogDrawer;
use crate::ui::preview_view::PreviewView;
use crate::ui::resizer_view::ResizerView;
use crate::ui::settings_view::SettingsView;
use crate::ui::theme::ThemeMode;

pub struct ShellView {
    state: Entity<AppState>,
    home: Entity<HomeView>,
    preview: Entity<PreviewView>,
    resizer: Entity<ResizerView>,
    settings: Entity<SettingsView>,
    picker: Entity<FontPickerModal>,
    confirm_modal: Entity<ConfirmModal>,
    log_drawer: Entity<LogDrawer>,
    ui_font_family: SharedString,
    logo_img: Arc<Image>,
    last_rem_size: Option<f32>,
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
        let confirm_modal = cx.new(|cx| ConfirmModal::new(state.clone(), cx));
        let home = cx.new(|_| HomeView::new(state.clone(), picker.clone(), confirm_modal.clone()));
        let preview = cx.new(|cx| PreviewView::new(state.clone(), cx));
        let resizer = cx.new(|_| ResizerView::new(state.clone()));
        let settings = cx.new(|cx| SettingsView::new(state.clone(), cx));
        let log_drawer = cx.new(|_| LogDrawer::new(state.clone()));
        let ui_font_family = pick_safe_ui_font(cx);
        let logo_img = Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../../assets/logo/variants/typemod-symbol-app-icon-64.png").to_vec(),
        ));

        let _sub = cx.observe(&state, |_, _, cx| cx.notify());

        Self {
            state,
            home,
            preview,
            resizer,
            settings,
            picker,
            confirm_modal,
            log_drawer,
            ui_font_family,
            logo_img,
            last_rem_size: None,
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
            s.save_config_async(cx.background_executor());
            cx.notify();
        });
    }

    fn toggle_theme(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.config.theme = match s.config.theme {
                ThemeMode::Dark => ThemeMode::Light,
                ThemeMode::Light => ThemeMode::Dark,
            };
            s.save_config_async(cx.background_executor());
            cx.notify();
        });
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let rem = s.ui_rem_size();
        if self.last_rem_size != Some(rem) {
            window.set_rem_size(px(rem));
            self.last_rem_size = Some(rem);
        }
        let collapsed = !s.config.rail_pinned || window.viewport_size().width < px(760.0);
        let theme = s.theme();
        let active_tab = s.active_tab;
        let is_admin = s.is_admin;
        let sys_state = s.system_state;
        let (theme_btn_label, theme_icon) = match s.config.theme {
            ThemeMode::Dark => ("Light Mode", "icons/sun.svg"),
            ThemeMode::Light => ("Dark Mode", "icons/moon.svg"),
        };
        let last_error = s.last_error.clone();
        let last_notice = s.last_notice.clone();

        let sys_tone = match sys_state {
            SystemFontState::Original => BadgeTone::Ok,
            SystemFontState::Modded => BadgeTone::Accent,
            SystemFontState::PendingRestoreReboot => BadgeTone::Warn,
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
                                                                .child("TypeMod"),
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
                                                    .tooltip(if collapsed {
                                                        "Expand sidebar"
                                                    } else {
                                                        "Collapse sidebar"
                                                    })
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
                                            .tooltip("Font Replacer")
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
                                            .tooltip("Font Preview")
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
                                            .tooltip("Font Resizer")
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
                                            .tooltip("Settings")
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
                    // Bottom-left Theme, Status & Version block
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("rail-theme-toggle", theme_btn_label, theme)
                                    .icon(theme_icon)
                                    .icon_only(collapsed)
                                    .align_left(true)
                                    .tooltip(theme_btn_label)
                                    .variant(ButtonVariant::Ghost)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_theme(cx)
                                    })),
                            )
                            .child(if collapsed {
                                div()
                                    .w_full()
                                    .pt(px(8.0))
                                    .border_t_1()
                                    .border_color(theme.line)
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(StatusPill::new(
                                        if is_admin { "ADM" } else { "USR" },
                                        if is_admin {
                                            BadgeTone::Ok
                                        } else {
                                            BadgeTone::Warn
                                        },
                                        theme,
                                    ))
                                    .child(StatusPill::new(
                                        match sys_state {
                                            SystemFontState::Original => "ORIG",
                                            SystemFontState::Modded => "MOD",
                                            SystemFontState::PendingRestoreReboot => "BOOT",
                                        },
                                        sys_tone,
                                        theme,
                                    ))
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
                                    .px(px(2.0))
                                    .pt(px(8.0))
                                    .border_t_1()
                                    .border_color(theme.line)
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(StatusPill::new(
                                                if is_admin { "ADMIN" } else { "STANDARD USER" },
                                                if is_admin {
                                                    BadgeTone::Ok
                                                } else {
                                                    BadgeTone::Warn
                                                },
                                                theme,
                                            ))
                                            .child(StatusPill::new(
                                                sys_state.label(),
                                                sys_tone,
                                                theme,
                                            )),
                                    )
                                    .child(
                                        div()
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
                                            )),
                                    )
                                    .into_any_element()
                            }),
                    ),
            )
            // Main Column (View + Bottom Banner + LogDrawer)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Active Tab View
                    .child(match active_tab {
                        NavTab::Home => self.home.clone().into_any_element(),
                        NavTab::Preview => self.preview.clone().into_any_element(),
                        NavTab::Resizer => self.resizer.clone().into_any_element(),
                        NavTab::Settings => self.settings.clone().into_any_element(),
                    })
                    // Bottom Error / Notice Banner
                    .when_some(last_error, |d, err| {
                        d.child(
                            div()
                                .px(px(20.0))
                                .py(px(8.0))
                                .bg(theme.err_soft)
                                .border_t_1()
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
                                .border_t_1()
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
                                                        if this.state.read(cx).is_busy {
                                                            return;
                                                        }
                                                        let state = this.state.clone();
                                                        state.update(cx, |s, cx| {
                                                            s.is_busy = true;
                                                            s.last_notice = Some("Refreshing Windows FontCache and Explorer...".to_string());
                                                            cx.notify();
                                                        });
                                                        cx.spawn(async move |_, cx| {
                                                            cx.background_executor()
                                                                .spawn(async move {
                                                                    crate::domain::registry::live_refresh_font_cache();
                                                                })
                                                                .await;
                                                            let _ = state.update(cx, |s, cx| {
                                                                s.is_busy = false;
                                                                s.last_notice = Some(
                                                                    "Refreshed FontCache, WM_FONTCHANGE, and Explorer. Reboot still recommended for full OS coverage.".to_string(),
                                                                );
                                                                cx.notify();
                                                            });
                                                        })
                                                        .detach();
                                                    })),
                                            )
                                            .child(
                                                VantageButton::new("reboot-now-btn", "Reboot Now", theme)
                                                    .variant(ButtonVariant::Primary)
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.confirm_modal.update(cx, |m, cx| {
                                                            m.open(
                                                                ConfirmRequest {
                                                                    title: "Reboot System".to_string(),
                                                                    message: "Save any open work before restarting. Reboot Windows now?".to_string(),
                                                                    confirm_label: "Reboot Now".to_string(),
                                                                    dangerous: true,
                                                                    on_cancel: None,
                                                                    on_confirm: Box::new(|_, _| {
                                                                        use std::os::windows::process::CommandExt;
                                                                        let _ = std::process::Command::new(
                                                                            crate::domain::registry::shutdown_exe(),
                                                                        )
                                                                        .args(["/r", "/t", "3"])
                                                                        .creation_flags(0x0800_0000)
                                                                        .spawn();
                                                                    }),
                                                                },
                                                                window,
                                                                cx,
                                                            );
                                                        });
                                                    })),
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
                    // Bottom Log Drawer
                    .child(self.log_drawer.clone()),
            )
            // Modal Overlays
            .child(self.picker.clone())
            .child(self.confirm_modal.clone())
    }
}
