use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Bounds, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Point, Render, StatefulInteractiveElement, Styled, Window, canvas, div, px,
};

use crate::app_state::{AppState, MergingMode};
use crate::domain::backup_cloner::{
    backup_dir, backup_status, ensure_backup, export_mod_package, import_and_apply_package,
};
use crate::domain::builder::{find_fontforge, install_fontforge_via_winget};
use crate::domain::registry::{are_clones_installed, check_system_state};
use crate::ui::components::{BadgeTone, ButtonVariant, StatusPill, ToggleSwitch, VantageButton};
use crate::ui::confirm_modal::{ConfirmModal, prompt_elevated_relaunch};
use crate::ui::theme::ThemeMode;

pub struct SettingsView {
    state: Entity<AppState>,
    confirm_modal: Entity<ConfirmModal>,
    dpi_track_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    is_dragging_dpi: bool,
    cached_backup: (usize, usize),
    last_busy: bool,
    scroll_handle: gpui::ScrollHandle,
}

impl SettingsView {
    pub fn new(state: Entity<AppState>, confirm_modal: Entity<ConfirmModal>) -> Self {
        Self {
            state,
            confirm_modal,
            dpi_track_bounds: Rc::new(Cell::new(None)),
            is_dragging_dpi: false,
            cached_backup: backup_status(),
            last_busy: false,
            scroll_handle: gpui::ScrollHandle::new(),
        }
    }

    fn update_dpi_from_pos(&self, pos: Point<Pixels>, save: bool, cx: &mut Context<Self>) {
        let Some(bounds) = self.dpi_track_bounds.get() else {
            return;
        };
        let width: f32 = bounds.size.width.into();
        if width <= 1.0 {
            return;
        }
        let rel_x: f32 = (pos.x - bounds.left()).into();
        let ratio = (rel_x / width).clamp(0.0, 1.0);
        let pct = (80 + ((ratio * 24.0).round() as u16) * 5).clamp(80, 200);
        self.state.update(cx, |s, cx| {
            s.config.ui_scale_pct = pct;
            if save {
                let _ = s.save_config();
            }
            cx.notify();
        });
    }

    fn toggle_bool(&self, field: &'static str, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            match field {
                "italic" => s.config.enable_italic = !s.config.enable_italic,
                "clone" => s.config.clone_segoe = !s.config.clone_segoe,
                "save_log" => s.config.save_log = !s.config.save_log,
                "save_fonts" => s.config.save_fonts = !s.config.save_fonts,
                "show_log" => s.config.show_log_drawer = !s.config.show_log_drawer,
                _ => {}
            }
            let _ = s.save_config();
            cx.notify();
        });
    }

    fn set_merging_mode(&self, mode: MergingMode, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.config.merging_mode = mode;
            let _ = s.save_config();
            cx.notify();
        });
    }

    fn set_theme(&self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.config.theme = mode;
            let _ = s.save_config();
            cx.notify();
        });
    }

    fn run_backup_now(&mut self, cx: &mut Context<Self>) {
        match ensure_backup() {
            Ok(dir) => {
                let (have, total) = backup_status();
                self.cached_backup = (have, total);
                self.state.update(cx, |s, cx| {
                    s.last_error = None;
                    s.last_notice = Some(format!(
                        "Stock Segoe UI backup verified ({have}/{total} files in {}).",
                        dir.display()
                    ));
                    cx.notify();
                });
            }
            Err(e) => {
                self.state.update(cx, |s, cx| {
                    s.last_error = Some(format!("Backup failed: {e}"));
                    cx.notify();
                });
            }
        }
    }

    fn package_zip_now(&self, cx: &mut Context<Self>) {
        if self.state.read(cx).is_busy {
            return;
        }
        let (lat, khm) = {
            let cfg = &self.state.read(cx).config;
            (
                cfg.latin.display_name.clone(),
                cfg.khmer.display_name.clone(),
            )
        };
        self.state.update(cx, |s, cx| {
            s.is_busy = true;
            s.last_error = None;
            cx.notify();
        });
        cx.spawn(async move |this, cx| {
            let Some(dest_zip) = cx
                .background_executor()
                .spawn(async move {
                    rfd::FileDialog::new()
                        .add_filter("SegoeUI Mod Package", &["zip"])
                        .set_file_name("segoeui-mod-pack.zip")
                        .save_file()
                })
                .await
            else {
                let _ = this.update(cx, |view, cx| {
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        cx.notify();
                    });
                });
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    export_mod_package(&dest_zip, lat.as_deref(), khm.as_deref())
                        .map(|count| (count, dest_zip))
                })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok((count, dest_zip)) => {
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        s.last_error = None;
                        s.last_notice = Some(format!(
                            "Packaged {count} modded fonts into {}.",
                            dest_zip.display()
                        ));
                        cx.notify();
                    });
                }
                Err(e) => {
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        s.last_error = Some(format!("Package failed: {e}"));
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    fn import_zip_now(&self, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        if s.is_busy {
            return;
        }
        if !s.is_admin {
            prompt_elevated_relaunch(
                self.state.clone(),
                &self.confirm_modal,
                "Importing and applying system font packages requires Administrator privileges. Relaunch elevated now?",
                window,
                cx,
            );
            return;
        }
        self.state.update(cx, |s, cx| {
            s.is_busy = true;
            s.last_error = None;
            cx.notify();
        });
        cx.spawn(async move |this, cx| {
            let Some(src_zip) = cx
                .background_executor()
                .spawn(async move {
                    rfd::FileDialog::new()
                        .add_filter("SegoeUI Mod Package", &["zip"])
                        .pick_file()
                })
                .await
            else {
                let _ = this.update(cx, |view, cx| {
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        cx.notify();
                    });
                });
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    import_and_apply_package(&src_zip).map(|count| (count, src_zip))
                })
                .await;
            let _ = this.update(cx, |view, cx| match result {
                Ok((count, src_zip)) => {
                    view.cached_backup = backup_status();
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        s.system_state = check_system_state();
                        s.clones_installed = are_clones_installed();
                        s.last_error = None;
                        s.last_notice = Some(format!(
                            "Imported & applied {count} fonts from {}. REBOOT REQUIRED to apply system-wide.",
                            src_zip.display()
                        ));
                        cx.notify();
                    });
                }
                Err(e) => {
                    view.state.update(cx, |s, cx| {
                        s.is_busy = false;
                        s.last_error = Some(format!("Import failed: {e}"));
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    fn install_fontforge(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            s.is_busy = true;
            s.status_text = "Installing FontForge via winget...".to_string();
            cx.notify();
        });

        let state = self.state.clone();
        cx.spawn(async move |_this, cx| {
            let res = cx
                .background_executor()
                .spawn(async move {
                    install_fontforge_via_winget()?;
                    Ok::<_, anyhow::Error>(find_fontforge())
                })
                .await;

            let _ = state.update(cx, |s, cx| {
                s.is_busy = false;
                match res {
                    Ok(ff) => {
                        s.fontforge_path = ff;
                        s.status_text = "Ready".to_string();
                        s.last_notice = Some("FontForge check completed.".to_string());
                    }
                    Err(e) => {
                        s.status_text = "FontForge install failed".to_string();
                        s.last_error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let theme = s.theme();
        let cfg = s.config.clone();
        let ff_display = s
            .fontforge_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Not detected".to_string());
        let ff_ok = s.fontforge_path.is_some();
        if self.last_busy && !s.is_busy {
            self.cached_backup = backup_status();
        }
        self.last_busy = s.is_busy;
        let (bak_have, bak_total) = self.cached_backup;

        let toggles: [(&'static str, &'static str, &'static str, bool); 5] = [
            (
                "italic",
                "Italic Font Replacement",
                "Enable separate Italic Latin & Secondary Script font slots (6 additional weights).",
                cfg.enable_italic,
            ),
            (
                "clone",
                "Clone Original Segoe UI",
                "Keep stock Segoe UI installed side-by-side as 'Segoe UI Clone'.",
                cfg.clone_segoe,
            ),
            (
                "save_log",
                "Save Build Log",
                "Write FontForge output to <app>_data\\SegoeUIChanger_Build_Log.txt.",
                cfg.save_log,
            ),
            (
                "save_fonts",
                "Export Generated Fonts",
                "Copy built *_system_mod.ttf files to <app>_data\\Outputs.",
                cfg.save_fonts,
            ),
            (
                "show_log",
                "Show Live Log Drawer",
                "Display the monospace FontForge output drawer at the bottom of the window.",
                cfg.show_log_drawer,
            ),
        ];

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .on_mouse_move(cx.listener(|this, ev: &gpui::MouseMoveEvent, _, cx| {
                if this.is_dragging_dpi && ev.pressed_button == Some(MouseButton::Left) {
                    this.update_dpi_from_pos(ev.position, false, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &gpui::MouseUpEvent, _, cx| {
                    if this.is_dragging_dpi {
                        this.is_dragging_dpi = false;
                        this.update_dpi_from_pos(ev.position, true, cx);
                    }
                }),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
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
                            .child("Application Settings"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_2)
                            .child("Engine mode, appearance, FontForge toolchain, and output options"),
                    ),
            )
            // Theme & Merging Mode
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(theme.text).child("Theme"))
                                    .child(div().text_xs().text_color(theme.text_3).child("Choose Dark or Light appearance")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .child(
                                        VantageButton::new("theme-dark", "Dark", theme)
                                            .variant(if cfg.theme == ThemeMode::Dark {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Secondary
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| this.set_theme(ThemeMode::Dark, cx))),
                                    )
                                    .child(
                                        VantageButton::new("theme-light", "Light", theme)
                                            .variant(if cfg.theme == ThemeMode::Light {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Secondary
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| this.set_theme(ThemeMode::Light, cx))),
                                    ),
                            ),
                    )
                    .child(div().h(px(1.0)).bg(theme.line))
                    .child(
                        div()
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
                                    .min_w(px(200.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(theme.text)
                                            .child("UI DPI / Scale (80% – 200%)"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_3)
                                            .child("Adjust application typography & UI scaling"),
                                    ),
                            )
                            .child({
                                let cur_pct = cfg.ui_scale_pct.clamp(80, 200);
                                let ratio = (cur_pct - 80) as f32 / 120.0;
                                let track_bounds_ref = self.dpi_track_bounds.clone();
                                let track_w = 220.0_f32;
                                let thumb_left = (ratio * (track_w - 14.0)).clamp(0.0, track_w - 14.0);

                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .id("dpi-slider")
                                            .relative()
                                            .w(px(track_w))
                                            .h(px(28.0))
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                                                    this.is_dragging_dpi = true;
                                                    this.update_dpi_from_pos(ev.position, false, cx);
                                                }),
                                            )
                                            .on_mouse_move(cx.listener(
                                                |this, ev: &gpui::MouseMoveEvent, _, cx| {
                                                    if this.is_dragging_dpi
                                                        && ev.pressed_button == Some(MouseButton::Left)
                                                    {
                                                        this.update_dpi_from_pos(ev.position, false, cx);
                                                    }
                                                },
                                            ))
                                            .on_mouse_up(
                                                MouseButton::Left,
                                                cx.listener(|this, ev: &gpui::MouseUpEvent, _, cx| {
                                                    if this.is_dragging_dpi {
                                                        this.is_dragging_dpi = false;
                                                        this.update_dpi_from_pos(ev.position, true, cx);
                                                    }
                                                }),
                                            )
                                            .child(
                                                canvas(
                                                    move |bounds, _, _| {
                                                        track_bounds_ref.set(Some(bounds));
                                                    },
                                                    |_, _, _, _| {},
                                                )
                                                .absolute()
                                                .inset_0(),
                                            )
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(6.0))
                                                    .rounded(px(3.0))
                                                    .bg(theme.surface_3)
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .h_full()
                                                            .w(gpui::relative(ratio))
                                                            .bg(theme.accent),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px(thumb_left))
                                                    .w(px(14.0))
                                                    .h(px(14.0))
                                                    .rounded(px(7.0))
                                                    .bg(theme.accent)
                                                    .border_2()
                                                    .border_color(theme.surface),
                                            ),
                                    )
                                    .child(StatusPill::new(
                                        format!("{cur_pct}%"),
                                        BadgeTone::Accent,
                                        theme,
                                    ))
                            }),
                    )
                    .child(div().h(px(1.0)).bg(theme.line))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(theme.text).child("Font Patch Engine"))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_3)
                                            .child(match cfg.merging_mode {
                                                MergingMode::Visual => "Scales letter height to match Segoe UI so labels don't clip or look oversized.",
                                                MergingMode::GridSync => "Keeps native letter proportions without height scaling—best if scaled text looks distorted.",
                                            }),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .child(
                                        VantageButton::new("mode-visual", "Match Segoe Size", theme)
                                            .variant(if cfg.merging_mode == MergingMode::Visual {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Secondary
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_merging_mode(MergingMode::Visual, cx)
                                            })),
                                    )
                                    .child(
                                        VantageButton::new("mode-grid", "Native Proportions", theme)
                                            .variant(if cfg.merging_mode == MergingMode::GridSync {
                                                ButtonVariant::Primary
                                            } else {
                                                ButtonVariant::Secondary
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_merging_mode(MergingMode::GridSync, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
            // Toggles
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
                    .children(toggles.into_iter().map(|(key, title, desc, enabled)| {
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .py(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w(px(180.0))
                                    .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(theme.text).child(title))
                                    .child(div().text_xs().text_color(theme.text_3).child(desc)),
                            )
                            .child(
                                ToggleSwitch::new(
                                    gpui::SharedString::from(format!("tog-{key}")),
                                    enabled,
                                    theme,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_bool(key, cx)
                                })),
                            )
                    })),
            )
            // Backup & Transfer
            .child(
                div()
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.line)
                    .rounded(px(8.0))
                    .p(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(gpui::FontWeight::MEDIUM)
                                                    .text_color(theme.text)
                                                    .child("Stock Segoe UI Backup"),
                                            )
                                            .child(StatusPill::new(
                                                format!("{bak_have}/{bak_total} FILES"),
                                                if bak_have == bak_total {
                                                    BadgeTone::Ok
                                                } else {
                                                    BadgeTone::Warn
                                                },
                                                theme,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_3)
                                            .child("Keep a local copy of original Windows Segoe UI fonts."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .child(
                                        VantageButton::new("act-backup", "Backup Stock", theme)
                                            .icon("icons/shield-check.svg")
                                            .disabled(s.is_busy)
                                            .on_click(cx.listener(|this, _, _, cx| this.run_backup_now(cx))),
                                    )
                                    .child(
                                        VantageButton::new("act-stock-dir", "Open Backups", theme)
                                            .icon("icons/folder-open.svg")
                                            .on_click(|_, _, _| {
                                                let dir = backup_dir();
                                                let _ = std::fs::create_dir_all(&dir);
                                                let _ = std::process::Command::new("explorer")
                                                    .arg(dir)
                                                    .spawn();
                                            }),
                                    ),
                            ),
                    )
                    .child(div().h(px(1.0)).bg(theme.line))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .text_color(theme.text)
                                            .child("Share & Import Mod Pack (.zip)"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.text_3)
                                            .child("Export your built font mod as a .zip or apply a shared pack without FontForge."),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(6.0))
                                    .child(
                                        VantageButton::new("act-pkg-zip", "Export Pack (.zip)", theme)
                                            .icon("icons/upload.svg")
                                            .disabled(s.is_busy)
                                            .on_click(cx.listener(|this, _, _, cx| this.package_zip_now(cx))),
                                    )
                                    .child(
                                        VantageButton::new("act-import-zip", "Import & Apply (.zip)", theme)
                                            .icon("icons/download.svg")
                                            .variant(ButtonVariant::Primary)
                                            .disabled(s.is_busy)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.import_zip_now(window, cx)
                                            })),
                                    ),
                            ),
                    ),
            )
            // FontForge Toolchain
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
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().text_sm().font_weight(gpui::FontWeight::MEDIUM).text_color(theme.text).child("FontForge Backend"))
                                    .child(StatusPill::new(
                                        if ff_ok { "DETECTED" } else { "MISSING" },
                                        if ff_ok { BadgeTone::Ok } else { BadgeTone::Warn },
                                        theme,
                                    )),
                            )
                            .child(div().text_xs().text_color(theme.text_3).child(ff_display)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.0))
                            .child(
                                VantageButton::new("open-docs-dir", "Open Data Folder", theme)
                                    .icon("icons/folder-open.svg")
                                    .on_click(|_, _, _| {
                                        let dir = AppState::app_docs_dir();
                                        let _ = std::fs::create_dir_all(&dir);
                                        let _ = std::process::Command::new("explorer")
                                            .arg(dir)
                                            .spawn();
                                    }),
                            )
                            .child(
                                VantageButton::new("ff-browse", "Browse ffpython.exe...", theme)
                                    .icon("icons/folder-open.svg")
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        cx.spawn(async move |this, cx| {
                                            let Some(path) = cx
                                                .background_executor()
                                                .spawn(async move {
                                                    rfd::FileDialog::new()
                                                        .add_filter("FontForge Python", &["exe"])
                                                        .pick_file()
                                                })
                                                .await
                                            else {
                                                return;
                                            };
                                            let path_check = path.clone();
                                            let is_valid = cx
                                                .background_executor()
                                                .spawn(async move {
                                                    use std::os::windows::process::CommandExt;
                                                    std::process::Command::new(&path_check)
                                                        .args(["-c", "import fontforge"])
                                                        .creation_flags(0x0800_0000)
                                                        .output()
                                                        .is_ok_and(|o| o.status.success())
                                                })
                                                .await;
                                            let _ = this.update(cx, |view, cx| {
                                                view.state.update(cx, |s, cx| {
                                                    if is_valid {
                                                        s.config.custom_fontforge_path =
                                                            Some(path.clone());
                                                        s.fontforge_path = Some(path);
                                                        s.last_error = None;
                                                        let _ = s.save_config();
                                                    } else {
                                                        s.last_error = Some(format!(
                                                            "Selected executable is not a valid ffpython.exe (missing fontforge module): {}",
                                                            path.display()
                                                        ));
                                                    }
                                                    cx.notify();
                                                });
                                            });
                                        })
                                        .detach();
                                    })),
                            )
                            .child(
                                VantageButton::new("ff-recheck", "Re-scan", theme)
                                    .icon("icons/refresh-cw.svg")
                                    .on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.state.update(cx, |s, cx| {
                                            s.fontforge_path = s
                                                .config
                                                .custom_fontforge_path
                                                .clone()
                                                .filter(|p| p.exists())
                                                .or_else(find_fontforge);
                                            cx.notify();
                                        });
                                    }),
                                ),
                            )
                            .child(
                                VantageButton::new("ff-install", "Install via winget", theme)
                                    .variant(ButtonVariant::Primary)
                                    .disabled(ff_ok || s.is_busy)
                                    .on_click(cx.listener(|this, _, _, cx| this.install_fontforge(cx))),
                            ),
                    ),
            )
            )
            .child(crate::ui::components::vertical_scrollbar(
                &self.scroll_handle,
                "settings-scroll-bar",
                theme,
            ))
    }
}
