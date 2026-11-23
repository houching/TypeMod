# Implementation Plan: `segoeui-changer` (Rust + GPUI)

- **Spec:** `specs/segoeui-changer/spec.md`
- **Target Root:** Workspace root (`./Cargo.toml`, `./src/`, `./assets/`, `./scripts/`)
- **UI Stack:** `gpui` (`0.2`) + `gpui-component` (`0.5`), Vantage Graphite theme (Dark / Light), English only
- **Font Engine:** Hybrid — native Rust (`windows` crate, `read-fonts`/`skrifa`, JSON config) + FontForge/Python helper scripts (extended with Khmer `U+1780–U+17FF`, `U+19E0–U+19FF` support)

---

## 1. Proposed Directory Structure

```text
SegoeUI-Changer/
├── Cargo.toml
├── build.rs                      # Optional Windows manifest / icon embedding
├── assets/
│   └── logo.ico
├── scripts/                      # Bundled/runtime FontForge & fontTools helpers
│   ├── variable_slicer.py        # Slices variable fonts + prunes to Segoe + Khmer
│   ├── engine.py                 # Visual/cap-height sync + Latin/Khmer merge
│   ├── engine_italic.py          # Italic variant of engine.py
│   ├── grid_sync_engine.py       # EM grid sync + Latin/Khmer merge
│   └── grid_sync_engine_italic.py
├── specs/
│   └── segoeui-changer/
│       └── spec.md
└── src/
    ├── main.rs                   # Entry point, CLI --state parsing, GPUI Application bootstrap
    ├── app_state.rs              # Centralized Entity<AppState>, config persistence, theme mode
    ├── domain/
    │   ├── mod.rs
    │   ├── font_discovery.rs     # Sibling weight auto-detection, variable fvar check (skrifa/read-fonts)
    │   ├── registry.rs           # HKLM Fonts read/write, system state check, Segoe UI Clone check
    │   ├── resizer.rs            # Win32 SystemParametersInfoW (NONCLIENTMETRICSW, LOGFONTW)
    │   ├── elevation.rs          # IsUserAnAdmin + ShellExecuteW("runas") + --state handoff
    │   ├── backup_cloner.rs      # Original Segoe UI backup + Segoe UI Clone generation/registration
    │   └── builder.rs            # Async build/restore orchestrator calling scripts & parsing progress
    └── ui/
        ├── mod.rs
        ├── theme.rs              # Vantage Graphite Dark & Light color/geometry tokens
        ├── shell.rs              # 56px/224px left rail, 52px top bar, Theme toggle, status chip
        ├── home_view.rs          # Latin, Khmer, Italic font cards, Build/Restore, progress
        ├── resizer_view.rs       # Uniform + per-element system font size controls (pt)
        ├── settings_view.rs      # Merging mode, italic toggle, clone toggle, export/log toggles, FontForge status
        ├── font_picker_modal.rs  # Searchable System Font Picker (from Windows Registry)
        └── log_drawer.rs         # Slide-out / collapsible monospace build log viewer
```

---

## 2. Task Breakdown & Verification Criteria

1. **Task 1 — Scaffold Cargo & Vantage Theme (`Cargo.toml`, `src/main.rs`, `src/ui/theme.rs`, `src/app_state.rs`)**
   - *Input:* `gpui = "0.2"`, `gpui-component = "0.5"`, `windows = "0.58+"`, `serde`, `serde_json`, `read-fonts`, `skrifa`, `rfd` (native file dialog).
   - *Output:* Compilable GPUI window with Vantage Graphite Dark/Light tokens and JSON config load/save.
   - *Verify:* `cargo check` passes with zero errors; window opens cleanly.

2. **Task 2 — Win32 & Registry Domain (`elevation.rs`, `registry.rs`, `resizer.rs`, `font_discovery.rs`)**
   - *Input:* Port logic from `tmp/src/app.py` (`is_admin`, `SystemFontPicker`, `check_system_state`, `restore_system`) and `tmp/src/font_resizer.py`.
   - *Output:* Native Win32 calls via `windows` crate + unit tests for weight auto-detection and `pt <-> lfHeight` conversion.
   - *Verify:* `cargo test` passes unit tests; `resizer::get_current_metrics()` reads real Windows font sizes.

3. **Task 3 — Hybrid Font Engine + Khmer Support (`scripts/*.py`, `domain/builder.rs`, `domain/backup_cloner.rs`)**
   - *Input:* Port and clean up `tmp/src/variable_slicer.py`, `engine*.py`, `grid_sync_engine*.py`, `segoe_cloner.py`.
   - *Output:* Add `ESSENTIAL_GLYPHS` ranges for Khmer (`0x1780..0x1800`, `0x19E0..0x1A00`), optional Khmer secondary base font merging onto Latin (replacing Arabic), and clean UTF-8 filenames (fixing `‏‏grid_sync_engine_italic.py`).
   - *Verify:* Python syntax check (`python -m py_compile scripts/*.py`) + Rust progress parser unit tests pass.

4. **Task 4 — Vantage GPUI Views (`shell.rs`, `home_view.rs`, `resizer_view.rs`, `settings_view.rs`, `font_picker_modal.rs`, `log_drawer.rs`)**
   - *Input:* Vantage Design System (`36px` controls, `1px` hairlines, tabular monospace readouts, semantic status badges) + all `AC-1`..`AC-13` flows.
   - *Output:* Full interactive UI wired to `Entity<AppState>` and background `cx.spawn` build/restore tasks.
   - *Verify:* `cargo clippy --all-targets` + `cargo test` + `cargo build` pass with zero warnings/stubs.

5. **Task 5 — Spec & Quality Verification (`/spec-verify`)**
   - *Input:* `specs/segoeui-changer/spec.md` (`AC-1`..`AC-13`).
   - *Output:* Verified traceability, `cargo test` green, manual/smoke launch check.
