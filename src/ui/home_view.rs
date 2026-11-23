use std::path::PathBuf;

use gpui::{
    Context, Entity, IntoElement, KeyDownEvent, ParentElement, Render, SharedString, Styled, Window,
    div, prelude::*, px,
};
use rfd::FileDialog;

use crate::app_state::{AppState, FontFamilySelection, NavTab, SecondaryScript};
use crate::domain::builder::{BuildProgressEvent, find_fontforge, run_build_and_apply};
use crate::domain::font_discovery::auto_detect_weights;
use crate::domain::registry::{are_clones_installed, check_system_state, restore_original_fonts};
use crate::ui::components::{BadgeTone, ButtonVariant, StatusPill, VantageButton};
use crate::ui::confirm_modal::{ConfirmModal, ConfirmRequest, prompt_elevated_relaunch};
use crate::ui::font_picker_modal::{FontPickerModal, FontTargetSlot};

#[derive(Clone, Copy)]
enum WeightKind {
    Light,
    Semilight,
    Semibold,
    Bold,
    Black,
}

impl WeightKind {
    fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Semilight => "Semilight",
            Self::Semibold => "Semibold",
            Self::Bold => "Bold",
            Self::Black => "Black",
        }
    }
}

pub struct HomeView {
    state: Entity<AppState>,
    picker: Entity<FontPickerModal>,
    confirm_modal: Entity<ConfirmModal>,
    script_dropdown_open: bool,
    scroll_handle: gpui::ScrollHandle,
}

struct FontCardProps<'a> {
    title: SharedString,
    subtitle: SharedString,
    slot: FontTargetSlot,
    sel: &'a FontFamilySelection,
    is_busy: bool,
    extra_header_row: Option<gpui::AnyElement>,
    extra_note: Option<SharedString>,
}

impl HomeView {
    pub fn new(
        state: Entity<AppState>,
        picker: Entity<FontPickerModal>,
        confirm_modal: Entity<ConfirmModal>,
    ) -> Self {
        Self {
            state,
            picker,
            confirm_modal,
            script_dropdown_open: false,
            scroll_handle: gpui::ScrollHandle::new(),
        }
    }

    fn browse_regular(&self, slot: FontTargetSlot, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(path) = cx
                .background_executor()
                .spawn(async move {
                    FileDialog::new()
                        .add_filter("Font Files", &["ttf", "otf"])
                        .pick_file()
                })
                .await
            else {
                return;
            };

            let detected = cx
                .background_executor()
                .spawn(async move { auto_detect_weights(&path, None, &[]) })
                .await;

            let _ = this.update(cx, |view, cx| {
                view.state.update(cx, |s, cx| {
                    if s.is_busy {
                        return;
                    }
                    *slot.select_mut(&mut s.config) = detected;
                    s.last_error = None;
                    let _ = s.save_config();
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn browse_weight(&self, slot: FontTargetSlot, weight: WeightKind, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let Some(path) = cx
                .background_executor()
                .spawn(async move {
                    FileDialog::new()
                        .add_filter("Font Files", &["ttf", "otf"])
                        .pick_file()
                })
                .await
            else {
                return;
            };

            let _ = this.update(cx, |view, cx| {
                view.state.update(cx, |s, cx| {
                    if s.is_busy {
                        return;
                    }
                    let target = slot.select_mut(&mut s.config);
                    match weight {
                        WeightKind::Light => target.light = Some(path),
                        WeightKind::Semilight => target.semilight = Some(path),
                        WeightKind::Semibold => target.semibold = Some(path),
                        WeightKind::Bold => target.bold = Some(path),
                        WeightKind::Black => target.black = Some(path),
                    }
                    let _ = s.save_config();
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn clear_slot(&self, slot: FontTargetSlot, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            slot.select_mut(&mut s.config).clear();
            let _ = s.save_config();
            cx.notify();
        });
    }

    fn open_system_picker(
        &self,
        slot: FontTargetSlot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |p, cx| {
            p.open(slot, window, cx);
        });
    }

    fn build_and_apply(&self, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        if !s.config.latin.has_regular() {
            self.state.update(cx, |s, cx| {
                s.last_error = Some("Please select at least a Primary English / Latin Regular font.".to_string());
                cx.notify();
            });
            return;
        }

        if !s.is_admin {
            prompt_elevated_relaunch(
                self.state.clone(),
                &self.confirm_modal,
                "Modifying Windows system fonts requires Administrator privileges. Relaunch elevated now?",
                window,
                cx,
            );
            return;
        }

        let Some(ff_exe) = s
            .fontforge_path
            .clone()
            .or_else(|| s.config.custom_fontforge_path.clone().filter(|p| p.exists()))
            .or_else(find_fontforge)
        else {
            self.state.update(cx, |s, cx| {
                s.last_error = Some(
                    "FontForge not found. Open Settings -> Install via winget to install FontForge."
                        .to_string(),
                );
                cx.notify();
            });
            return;
        };

        let cfg = s.config.clone();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel_worker = cancel.clone();
        self.state.update(cx, |s, cx| {
            s.is_busy = true;
            s.cancel_flag = Some(cancel);
            s.progress = 0.05;
            s.status_text = "Starting build...".to_string();
            s.last_error = None;
            s.last_notice = None;
            s.build_log.clear();
            cx.notify();
        });

        enum WorkerMsg {
            Progress(BuildProgressEvent),
            Done(anyhow::Result<()>),
        }

        let state = self.state.clone();
        let (tx, rx) = std::sync::mpsc::channel::<WorkerMsg>();

        cx.spawn(async move |_this, cx| {
            let tx_done = tx.clone();
            cx.background_executor()
                .spawn(async move {
                    let res = run_build_and_apply(&cfg, &ff_exe, &cancel_worker, |ev| {
                        let _ = tx.send(WorkerMsg::Progress(ev));
                    });
                    let _ = tx_done.send(WorkerMsg::Done(res));
                })
                .detach();

            let mut final_res: Option<anyhow::Result<()>> = None;
            loop {
                let mut disconnected = false;
                loop {
                    match rx.try_recv() {
                        Ok(WorkerMsg::Progress(ev)) => {
                            let _ = state.update(cx, |s, cx| {
                                match ev {
                                    BuildProgressEvent::Step { progress, status } => {
                                        s.progress = progress;
                                        s.status_text = status;
                                    }
                                    BuildProgressEvent::Log(line) => s.push_log(line),
                                }
                                cx.notify();
                            });
                        }
                        Ok(WorkerMsg::Done(res)) => {
                            final_res = Some(res);
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
                if final_res.is_some() {
                    break;
                }
                if disconnected {
                    final_res = Some(Err(anyhow::anyhow!("Build worker terminated unexpectedly.")));
                    break;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(80))
                    .await;
            }

            let res = final_res.unwrap_or_else(|| Err(anyhow::anyhow!("Build cancelled")));

            let _ = state.update(cx, |s, cx| {
                let was_cancelled = s
                    .cancel_flag
                    .as_ref()
                    .is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed));
                s.is_busy = false;
                s.cancel_flag = None;
                s.system_state = check_system_state();
                s.clones_installed = are_clones_installed();
                match res {
                    Ok(()) => {
                        s.progress = 1.0;
                        s.status_text = "Success! Reboot required to apply changes.".to_string();
                        s.last_notice = Some(
                            "Success! REBOOT REQUIRED to apply system-wide font changes."
                                .to_string(),
                        );
                    }
                    Err(_) if was_cancelled => {
                        s.progress = 0.0;
                        s.status_text = "Cancelled".to_string();
                        s.last_notice = Some("Build cancelled before modifying Windows Fonts.".to_string());
                    }
                    Err(e) => {
                        s.status_text = "Build failed".to_string();
                        s.last_error = Some(e.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn cancel_build(&self, cx: &mut Context<Self>) {
        self.state.update(cx, |s, cx| {
            if let Some(flag) = &s.cancel_flag {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                s.status_text = "Cancelling build...".to_string();
                cx.notify();
            }
        });
    }

    fn restore_system(&self, window: &mut Window, cx: &mut Context<Self>) {
        let s = self.state.read(cx);
        if !s.is_admin {
            prompt_elevated_relaunch(
                self.state.clone(),
                &self.confirm_modal,
                "Restoring Windows system fonts requires Administrator privileges. Relaunch elevated now?",
                window,
                cx,
            );
            return;
        }

        let has_clones = s.clones_installed;
        let state_handle = self.state.clone();
        let confirm_modal = self.confirm_modal.clone();
        let run_restore = move |delete_clones: bool, cx: &mut gpui::App| {
            let state_bg = state_handle.clone();
            state_handle.update(cx, |s, cx| {
                s.is_busy = true;
                s.last_error = None;
                s.last_notice = Some("Restoring stock Segoe UI registry entries...".to_string());
                cx.notify();
            });
            cx.spawn(async move |cx| {
                let res = cx
                    .background_executor()
                    .spawn(async move { restore_original_fonts(delete_clones) })
                    .await;
                let _ = state_bg.update(cx, |s, cx| {
                    s.is_busy = false;
                    match res {
                        Ok(()) => {
                            s.system_state = check_system_state();
                            s.clones_installed = are_clones_installed();
                            s.last_error = None;
                            s.last_notice = Some(
                                "Restored stock Segoe UI registry entries. REBOOT REQUIRED to finalize."
                                    .to_string(),
                            );
                        }
                        Err(e) => {
                            s.last_error = Some(format!("Restore failed: {e}"));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        };

        self.confirm_modal.update(cx, |m, cx| {
            m.open(
                ConfirmRequest {
                    title: "Restore Original Font".to_string(),
                    message: "This will reset all Segoe UI Registry entries back to stock Windows defaults. Proceed?".to_string(),
                    confirm_label: "Restore".to_string(),
                    dangerous: true,
                    on_confirm: Box::new(move |window, cx| {
                        if has_clones {
                            let modal = confirm_modal.clone();
                            window.defer(cx, move |window, cx| {
                                modal.update(cx, |m2, cx| {
                                    m2.open(
                                        ConfirmRequest {
                                            title: "Remove Segoe UI Clone?".to_string(),
                                            message: "Segoe UI Clone fonts are installed. Do you want to remove them as well? (Press Cancel to keep clones and still restore.)".to_string(),
                                            confirm_label: "Remove Clones".to_string(),
                                            dangerous: true,
                                            on_confirm: Box::new(move |_, cx| run_restore(true, cx)),
                                        },
                                        window,
                                        cx,
                                    );
                                });
                            });
                        } else {
                            run_restore(false, cx);
                        }
                    }),
                },
                window,
                cx,
            );
        });
    }

    fn render_font_card(
        &self,
        props: FontCardProps<'_>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let FontCardProps {
            title,
            subtitle,
            slot,
            sel,
            is_busy,
            extra_header_row,
            extra_note,
        } = props;
        let theme = self.state.read(cx).theme();
        let file_label = sel
            .regular
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "No font selected".to_string());

        let weights: [(WeightKind, &Option<PathBuf>); 5] = [
            (WeightKind::Light, &sel.light),
            (WeightKind::Semilight, &sel.semilight),
            (WeightKind::Semibold, &sel.semibold),
            (WeightKind::Bold, &sel.bold),
            (WeightKind::Black, &sel.black),
        ];

        let detected_count = [
            sel.regular.is_some(),
            sel.light.is_some(),
            sel.semilight.is_some(),
            sel.semibold.is_some(),
            sel.bold.is_some(),
            sel.black.is_some(),
        ]
        .into_iter()
        .filter(|&x| x)
        .count();
        let weight_summary = if sel.is_variable || !sel.has_regular() {
            String::new()
        } else {
            format!("{detected_count}/6 weights")
        };

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
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(theme.text)
                                            .child(title.clone()),
                                    )
                                    .when(sel.is_variable, |d| {
                                        d.child(StatusPill::new("VARIABLE", BadgeTone::Accent, theme))
                                    })
                                    .when(!weight_summary.is_empty(), |d| {
                                        d.child(StatusPill::new(weight_summary, BadgeTone::Ok, theme))
                                    }),
                            )
                            .child(div().text_xs().text_color(theme.text_3).child(subtitle)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(6.0))
                            .child(
                                VantageButton::new(
                                    gpui::SharedString::from(format!("browse-{title}")),
                                    "Browse File...",
                                    theme,
                                )
                                .icon("icons/folder-open.svg")
                                .disabled(is_busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.browse_regular(slot, cx)
                                })),
                            )
                            .child(
                                VantageButton::new(
                                    gpui::SharedString::from(format!("sys-{title}")),
                                    "System Font",
                                    theme,
                                )
                                .icon("icons/search.svg")
                                .disabled(is_busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_system_picker(slot, window, cx)
                                })),
                            )
                            .when(sel.has_regular(), |d| {
                                d.child(
                                    VantageButton::new(
                                        gpui::SharedString::from(format!("clr-{title}")),
                                        "Clear",
                                        theme,
                                    )
                                    .icon("icons/x.svg")
                                    .variant(ButtonVariant::Ghost)
                                    .disabled(is_busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.clear_slot(slot, cx)
                                    })),
                                )
                            }),
                    ),
            )
            .when_some(extra_header_row, |d, row| d.child(row))
            .when_some(extra_note, |d, note| {
                d.child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(6.0))
                        .bg(theme.accent_soft)
                        .text_xs()
                        .text_color(theme.accent)
                        .child(note),
                )
            })
            .child(
                div()
                    .min_h(px(36.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .bg(theme.surface_2)
                    .border_1()
                    .border_color(theme.line)
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .min_w_0()
                            .text_xs()
                            .text_color(if sel.has_regular() {
                                theme.text
                            } else {
                                theme.text_3
                            })
                            .child(file_label),
                    )
                    .when_some(
                        sel.display_name.clone().filter(|name| {
                            sel.regular
                                .as_ref()
                                .and_then(|p| p.file_stem())
                                .is_none_or(|s| s.to_string_lossy() != *name)
                        }),
                        |d, name| d.child(StatusPill::new(name, BadgeTone::Neutral, theme)),
                    ),
            )
            .when(!sel.is_variable, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.0))
                        .children(weights.into_iter().map(|(w, path_opt)| {
                            let detected = path_opt.is_some();
                            let w_label = if let Some(p) = path_opt {
                                format!(
                                    "{}: {}",
                                    w.label(),
                                    p.file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_default()
                                )
                            } else {
                                format!("+ {}", w.label())
                            };
                            VantageButton::new(
                                gpui::SharedString::from(format!("w-{title}-{}", w.label())),
                                w_label,
                                theme,
                            )
                                .variant(if detected {
                                    ButtonVariant::Secondary
                                } else {
                                    ButtonVariant::Ghost
                                })
                                .disabled(is_busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.browse_weight(slot, w, cx)
                                }))
                        })),
                )
            })
    }
}

impl Render for HomeView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let theme = s.theme();
        let cfg = s.config.clone();
        let is_busy = s.is_busy;
        let progress = s.progress.clamp(0.0, 1.0);
        let status_text = s.status_text.clone();

        let active_scripts = cfg.active_scripts();
        let script_name = cfg.active_scripts_summary();
        let ff_ready = s.fontforge_path.is_some()
            || s.config
                .custom_fontforge_path
                .as_ref()
                .is_some_and(|p| p.exists());
        let can_build = !is_busy && cfg.latin.has_regular();
        let pct_label = format!("{}%", (progress * 100.0).round() as u32);

        let dropdown_open = self.script_dropdown_open;
        let script_selector_row = div()
            .flex()
            .items_center()
            .justify_between()
            .pt(px(2.0))
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child("Target Script"),
            )
            .child(
                div()
                    .relative()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .child(
                        VantageButton::new(
                            "script-dropdown-trigger",
                            format!("{script_name}  ▾"),
                            theme,
                        )
                        .disabled(is_busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.script_dropdown_open = !this.script_dropdown_open;
                            cx.notify();
                        })),
                    )
                    .when(dropdown_open, |d| {
                        d.child(
                            gpui::deferred(
                                div()
                                    .id("script-dropdown-menu")
                                    .occlude()
                                    .absolute()
                                    .top(px(40.0))
                                    .right_0()
                                    .w(gpui::rems(11.5))
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.line_2)
                                    .rounded(px(6.0))
                                    .p(px(4.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .children(SecondaryScript::ALL.into_iter().map(|script| {
                                        let active = active_scripts.contains(&script);
                                        let label = if active {
                                            format!("✓  {}", script.label())
                                        } else {
                                            format!("    {}", script.label())
                                        };
                                        VantageButton::new(
                                            SharedString::from(format!("script-{}", script.label())),
                                            label,
                                            theme,
                                        )
                                        .align_left(true)
                                        .variant(if active {
                                            ButtonVariant::Primary
                                        } else {
                                            ButtonVariant::Ghost
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.state.update(cx, |s, cx| {
                                                s.config.toggle_secondary_script(script);
                                                let _ = s.save_config();
                                                cx.notify();
                                            });
                                        }))
                                    })),
                            )
                            .with_priority(10),
                        )
                    }),
            )
            .into_any_element();

        let mut script_selector_opt = Some(script_selector_row);
        let mut base_script_cards = Vec::with_capacity(active_scripts.len());
        for (idx, &script) in active_scripts.iter().enumerate() {
            let s_name = script.label();
            let script_sel = cfg.script_font(script).cloned().unwrap_or_default();
            let script_same_as_latin =
                script_sel.has_regular() && script_sel.regular == cfg.latin.regular;
            let title = if idx == 0 {
                format!("2. {s_name} Base Font (Optional)")
            } else {
                format!("2.{} {s_name} Base Font (Optional)", idx + 1)
            };
            base_script_cards.push(self.render_font_card(
                FontCardProps {
                    title: title.into(),
                    subtitle: format!(
                        "Optional {s_name} font merged with OpenType shaping (GSUB/GPOS)."
                    )
                    .into(),
                    slot: FontTargetSlot::Script(script),
                    sel: &script_sel,
                    is_busy,
                    extra_header_row: if idx == 0 {
                        script_selector_opt.take()
                    } else {
                        None
                    },
                    extra_note: script_same_as_latin.then(|| {
                        SharedString::from(format!(
                            "Same file as Latin — {s_name} glyphs & shaping will be preserved automatically in single-font mode."
                        ))
                    }),
                },
                cx,
            ));
        }

        let mut italic_script_cards = Vec::new();
        if cfg.enable_italic {
            for (idx, &script) in active_scripts.iter().enumerate() {
                let s_name = script.label();
                let script_ital_sel = cfg.script_italic_font(script).cloned().unwrap_or_default();
                let title = if idx == 0 {
                    format!("4. {s_name} Italic / Oblique Font (Optional)")
                } else {
                    format!("4.{} {s_name} Italic / Oblique Font (Optional)", idx + 1)
                };
                italic_script_cards.push(self.render_font_card(
                    FontCardProps {
                        title: title.into(),
                        subtitle: format!(
                            "Optional {s_name} italic weights merged into Segoe UI Italic."
                        )
                        .into(),
                        slot: FontTargetSlot::ScriptItalic(script),
                        sel: &script_ital_sel,
                        is_busy,
                        extra_header_row: None,
                        extra_note: None,
                    },
                    cx,
                ));
            }
        }

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
            .child(
                div()
                    .id("home-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" && this.script_dropdown_open {
                    this.script_dropdown_open = false;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.script_dropdown_open {
                        this.script_dropdown_open = false;
                        cx.notify();
                    }
                }),
            )
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
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
                            .child("Font Replacer & Merger"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.text_2)
                            .child("Select Latin and secondary script fonts, then build & apply system-wide."),
                    ),
            )
            .child(self.render_font_card(
                FontCardProps {
                    title: "1. Primary English / Latin Font".into(),
                    subtitle: "Required base font for Latin UI text, digits, and punctuation."
                        .into(),
                    slot: FontTargetSlot::Latin,
                    sel: &cfg.latin,
                    is_busy,
                    extra_header_row: None,
                    extra_note: None,
                },
                cx,
            ))
            .children(base_script_cards)
            .when(cfg.enable_italic, |d| {
                d.child(self.render_font_card(
                    FontCardProps {
                        title: "3. Latin Italic Font".into(),
                        subtitle:
                            "Optional Italic weights (6 variants) when Italic replacement is enabled."
                                .into(),
                        slot: FontTargetSlot::LatinItalic,
                        sel: &cfg.latin_italic,
                        is_busy,
                        extra_header_row: None,
                        extra_note: None,
                    },
                    cx,
                ))
                .children(italic_script_cards)
            })
            // Action & Progress Panel
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
                                    .min_w_0()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_2)
                                                    .child("BUILD & APPLY"),
                                            )
                                            .child(StatusPill::new(pct_label, BadgeTone::Accent, theme))
                                            .child(StatusPill::new(
                                                cfg.merging_mode.label(),
                                                BadgeTone::Neutral,
                                                theme,
                                            ))
                                            .when(!ff_ready, |d| {
                                                d.child(
                                                    VantageButton::new(
                                                        "ff-missing-cta",
                                                        "FontForge Missing · Configure",
                                                        theme,
                                                    )
                                                    .variant(ButtonVariant::Danger)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.state.update(cx, |s, cx| {
                                                            s.active_tab = NavTab::Settings;
                                                            cx.notify();
                                                        });
                                                    })),
                                                )
                                            }),
                                    )
                                    .child(div().text_sm().text_color(theme.text).child(status_text)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .when(is_busy, |d| {
                                        d.child(
                                            VantageButton::new("cancel-build-btn", "Cancel Build", theme)
                                                .icon("icons/x.svg")
                                                .variant(ButtonVariant::Danger)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cancel_build(cx)
                                                })),
                                        )
                                    })
                                    .when(!is_busy, |d| {
                                        d.child(
                                            VantageButton::new("restore-btn", "Restore Original Font", theme)
                                                .icon("icons/rotate-ccw.svg")
                                                .variant(ButtonVariant::Danger)
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.restore_system(window, cx)
                                                })),
                                        )
                                    })
                                    .child(
                                        VantageButton::new("build-btn", "Build & Apply", theme)
                                            .icon("icons/play.svg")
                                            .variant(ButtonVariant::Primary)
                                            .disabled(!can_build)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.build_and_apply(window, cx)
                                            })),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h(px(6.0))
                            .w_full()
                            .rounded(px(3.0))
                            .bg(theme.surface_3)
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(gpui::relative(progress))
                                    .bg(if progress >= 1.0 {
                                        theme.ok
                                    } else {
                                        theme.accent
                                    }),
                            ),
                    ),
            )
            )
            .child(crate::ui::components::vertical_scrollbar(
                &self.scroll_handle,
                "home-scroll-bar",
                theme,
            ))
    }
}
