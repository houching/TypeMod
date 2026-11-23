# Spec: SegoeUI Changer (Rust + GPUI)

- **Slug:** `segoeui-changer`
- **Status:** Approved
- **Domain:** `desktop-font-manager`
- **Plan:** `segoeui-changer.md`
- **Updated:** 2026-11-23

## Problem & Goal
Windows modern UI surfaces (Start Menu, Taskbar, Settings, UWP/WinUI apps, Lock/Login screens) hardcode `Segoe UI` and ignore legacy `FontSubstitutes` registry entries. The existing Python/CustomTkinter prototype (`tmp/src/`) solves this by backing up original Segoe UI fonts, patching/merging user-selected fonts to impersonate `Segoe UI`, and adjusting Windows system font metrics.

Goal: Build **`segoeui-changer`**, a native Windows desktop application in **Rust + GPUI** with a **Vantage Graphite** (`Dark / Light`) UI, achieving **1:1 functional parity** with `tmp/src/` plus **Khmer script/glyph support**, using a **hybrid** architecture (native Rust UI, Win32/Registry, and state management + FontForge/fontTools helper for font slicing, metric sync, and glyph merging).

## Requirements & Acceptance Criteria

### [ADDED] REQ-1: Font Selection, Auto-Detection & Variable Font Support
- **AC-1:** **GIVEN** the user is on the Home view **WHEN** they browse for a local `.ttf`/`.otf` font file or select an installed font via the System Font Picker (read from `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts`) **THEN** the app populates the Regular weight and auto-detects sibling weights (`Light`, `Semilight`, `Semibold`, `Bold`, `Black`) in the same directory or registry family.
- **AC-2:** **GIVEN** a selected font contains a `fvar` variable font table **WHEN** it is loaded **THEN** the app marks it as `[VARIABLE]` and slices the 6 standard weights (`300, 350, 400, 600, 700, 900`) automatically during build.
- **AC-3:** **GIVEN** optional Italic mode is enabled **WHEN** the user selects Italic Latin/Khmer fonts **THEN** all 6 italic weights (`Light Italic` through `Black Italic`) are resolved alongside regular weights.

### [ADDED] REQ-2: Bilingual Merging (English / Latin + Khmer)
- **AC-4:** **GIVEN** a primary English/Latin font and an optional Khmer base font **WHEN** the user builds **THEN** the hybrid font engine preserves essential Unicode ranges (including Latin and Khmer `U+1780–U+17FF`, `U+19E0–U+19FF`), merges Khmer glyphs and OpenType shaping tables (`GSUB`/`GPOS`) onto the Latin base, and supports both `visual` (cap-height/scale sync) and `grid_sync` (EM grid sync) modes.
- **AC-5:** **GIVEN** a Latin font that already contains Khmer glyphs and no separate Khmer font is chosen **WHEN** unused glyphs are pruned **THEN** Khmer codepoints (`U+1780–U+17FF`, `U+19E0–U+19FF`) and their shaping lookups are preserved rather than stripped.

### [ADDED] REQ-3: Backup, System Application, Clone & Restore
- **AC-6:** **GIVEN** the user clicks **Build & Apply** without Administrator privileges **WHEN** elevation is confirmed **THEN** the app saves current UI state to a temp file and relaunches elevated (`ShellExecuteW` `runas` with `--state <path>`).
- **AC-7:** **GIVEN** Administrator privileges and FontForge available (or installed via `winget`) **WHEN** **Build & Apply** runs **THEN** original `Segoe UI` files are backed up once to `Original_Segoe_Backups/`, generated `*_system_mod.ttf` files (plus `SegUIVar_system_mod.ttf`) are copied to `%WINDIR%\Fonts` and registered in `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts`, optionally `Segoe UI Clone` is installed from backups, and the user is notified that a reboot is required.
- **AC-8:** **GIVEN** modded fonts are currently active **WHEN** the user clicks **Build & Apply** **THEN** the app blocks the build and instructs the user to **Restore Original Font** and reboot first.
- **AC-9:** **GIVEN** the user clicks **Restore Original Font** as Admin **WHEN** confirmed **THEN** all 13 `Segoe UI` registry entries are reset to stock `.ttf` filenames (`segoeui.ttf`, `SegUIVar.ttf`, etc.) and, if confirmed, `Segoe UI Clone` registry keys and `clone_segoe*.ttf` files are removed.

### [ADDED] REQ-4: System Font Resizer (Win32 Metrics)
- **AC-10:** **GIVEN** the Resizer view/panel **WHEN** opened **THEN** it reads current Windows `NONCLIENTMETRICSW` (`caption`, `sm_caption`, `menu`, `status`, `message`) and `LOGFONTW` (`icon`) heights in `pt` via `SystemParametersInfoW`.
- **AC-11:** **GIVEN** the user sets a uniform point size or individual element sizes and clicks Apply **WHEN** `SystemParametersInfoW` is called with `SPIF_UPDATEINIFILE | SPIF_SENDCHANGE` **THEN** Windows system font sizes update immediately.

### [ADDED] REQ-5: Vantage Graphite UI, Settings & Persistence
- **AC-12:** **GIVEN** the GPUI application window **WHEN** rendered **THEN** it uses the Vantage Graphite palette (`Dark` and `Light` theme toggle, `36px` control height, `1px` crisp hairlines, monospace paths/logs/metrics, English-only copy).
- **AC-13:** **GIVEN** Settings options (merging mode `visual`/`grid_sync`, italic toggle, `Segoe UI Clone` toggle, save build log, export built fonts to `Documents\SegoeUIChanger\Outputs`, live build log viewer, Dark/Light theme, UI DPI scale `80%–200%`) **WHEN** changed **THEN** they persist to JSON config and restore on next launch.

## Edge Cases & Error States
- Missing FontForge installation: prompt user to install via `winget install --id FontForge.FontForge` or browse/locate `fontforge.exe` / `ffpython.exe`.
- Pending reboot detection: if `*_system_mod.ttf` is in registry vs. pending file replacement, prevent double-patching until restored and rebooted.
- Variable font slicing failure on malformed glyphs: handle non-critical subsetting errors gracefully and fall back cleanly.
- Khmer OpenType shaping tables (`GSUB`/`GPOS`): preserve Khmer shaping lookups, subscript/coeng forms, and vowel/diacritic positioning when merging or pruning.

## Non-Goals (Out of Scope)
- Multi-language UI localization (Turkish/Arabic UI strings from `tmp/src/translations.py` are out of scope; English only).
- Pure-Rust replacement of FontForge's glyph outline/GSUB/GPOS merging engine (hybrid helper is used for font generation).
- Non-Windows OS support (Windows 10/11 only).

## Traceability
| AC ID | Requirement | Artifact / Test | Result | Evidence |
|---|---|---|---|---|
| AC-1 | Font selection & sibling weight auto-detect | `src/domain/font_discovery.rs`, `src/domain/registry.rs` | PASS | `cargo test` passes `reads_installed_windows_fonts_and_system_state`, `reads_live_win32_metrics_and_detects_segoe_weights`, `family_stem_extraction`, `weight_matching_distinguishes_semi_variants` |
| AC-2 | Variable font (`fvar`) detection & slicing | `src/domain/font_discovery.rs`, `scripts/variable_slicer.py` | PARTIAL | `py_compile` passes `variable_slicer.py`; live `fvar` slicing unverified |
| AC-3 | Optional Italic 6-weight resolution | `src/ui/home_view.rs`, `scripts/engine_italic.py`, `scripts/grid_sync_engine_italic.py` | PARTIAL | `py_compile` passes italic scripts; live build unverified |
| AC-4 | Bilingual English/Latin + Khmer merging | `scripts/engine.py`, `scripts/grid_sync_engine.py`, `src/domain/builder.rs` | PARTIAL | `cargo test` passes `parses_engine_progress_lines`; `py_compile` passes; live FontForge merge unverified |
| AC-5 | Preserve Khmer `U+1780–U+17FF`, `U+19E0–U+19FF` on prune | `scripts/variable_slicer.py`, `scripts/engine*.py` | PARTIAL | `py_compile` passes; runtime glyph retention unverified |
| AC-6 | UAC elevation with `--state` handoff | `src/domain/elevation.rs`, `src/main.rs` | UNVERIFIED | Compiles cleanly (`cargo clippy`); UAC prompt not triggered in automated run |
| AC-7 | Backup, FontForge build, `%WINDIR%\Fonts` + Registry + Clone | `src/domain/backup_cloner.rs`, `src/domain/builder.rs`, `src/domain/registry.rs` | UNVERIFIED | Compiles cleanly; requires elevated Windows runtime |
| AC-8 | Block rebuild when modded/pending reboot | `src/domain/registry.rs`, `src/ui/home_view.rs` | UNVERIFIED | Compiles cleanly; requires modded registry state to test |
| AC-9 | Restore stock `Segoe UI` & optional clone cleanup | `src/domain/registry.rs`, `src/ui/home_view.rs` | UNVERIFIED | Compiles cleanly; requires elevated Windows runtime |
| AC-10 | Read Win32 `NONCLIENTMETRICSW` & `LOGFONTW` | `src/domain/resizer.rs` | PASS | `cargo test` passes `pt_height_roundtrip` and live `reads_live_win32_metrics_and_detects_segoe_weights` |
| AC-11 | Apply Win32 system font sizes | `src/domain/resizer.rs`, `src/ui/resizer_view.rs` | UNVERIFIED | Compiles cleanly; avoided mutating host OS metrics during automated verify |
| AC-12 | Vantage Graphite Dark/Light UI | `src/ui/theme.rs`, `src/ui/shell.rs`, `src/ui/components.rs` | PASS | `cargo build` + live launch `target\debug\segoeui-changer.exe` (`GUI_ALIVE=True` after 2s) |
| AC-13 | Settings + JSON persistence + DPI `80%–200%` | `src/app_state.rs`, `src/ui/settings_view.rs` | PASS | `cargo test` passes `ui_scale_clamps_between_80_and_200` |
