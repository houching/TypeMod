# SegoeUI Changer

A native Windows desktop utility built with **Rust + GPUI** for replacing and merging Windows system fonts (`Segoe UI` / `Segoe UI Variable`), tuning system UI font sizes, and previewing multi-script typography.

## Features

- **Font Replacer & Multi-Script Merger**
  - Replace `Segoe UI` (6 normal weights + `SegUIVar` + optional 6 Italic weights) with your chosen Latin font.
  - Multi-script selection (**Khmer**, **Arabic**, **Thai**, **Lao**, **Myanmar**, **Vietnamese**, **CJK**) with per-script font cards and OpenType `GSUB`/`GPOS` shaping preservation.
  - Supports both static weight families (auto-detected from folder/registry) and **Variable Fonts** (sliced to 6 weights via `fonttools`).
  - Two merging modes: **Match Segoe Size (Visual)** and **Native Proportions (Grid Sync)**.
  - Parallel FontForge workers (`scripts/engine_core.py`) for fast builds.
- **Font Preview & Ligature Tester**
  - Live preview across Light (`300`) through Black (`900`) weights with Khmer, English, and UI symbol presets and adjustable font size (`10–48 px`).
- **System Font Resizer**
  - Adjust Windows UI metrics (`Caption`, `Small Caption`, `Menu`, `Status`, `Message`, `Icon`) uniformly or individually (`6–24 pt`, or reset to `9 pt`).
- **Backup, Clone & Portable `.zip` Packs**
  - Backs up stock Windows `Segoe UI` files to `Original_Segoe_Backups/`.
  - Optional pure-Rust **Segoe UI Clone** (`write-fonts`) so original `Segoe UI` remains usable side-by-side.
  - Export and import portable `.zip` font packs without needing FontForge on the target machine.
- **Vantage UI**
  - Dark & Light themes, `80%–200%` DPI UI scaling, collapsible navigation rail, live build log drawer with Copy-to-Clipboard, and themed in-app confirmation modals.

## Requirements

- **OS:** Windows 10 / Windows 11 (x64)
- **Toolchain for building fonts:** [FontForge](https://fontforge.org/) (`ffpython.exe` at `C:\Program Files\FontForgeBuilds\bin\ffpython.exe`, or installable from **Settings → Install via winget**).
- **Variable Fonts:** Uses `fonttools` via system `python` or FontForge's bundled `ffpython.exe` (auto-installed via `pip` on first variable-font build if missing).
- **Privileges:** Administrator rights are requested via UAC only when applying/restoring or importing system fonts.

## Building from Source

```powershell
# Run tests and lint
cargo test
cargo clippy --all-targets -- -D warnings

# Run in release mode
cargo build --release
```

When distributing the compiled binary, place the `scripts/` directory next to `segoeui-changer.exe` (the app resolves `scripts/` relative to the executable first, then the working directory).

## CI / Releases

GitHub Actions (`.github/workflows/release.yml`) runs `cargo test`, `cargo clippy --all-targets -- -D warnings`, builds `segoeui-changer.exe` in release mode on `windows-latest`, and packages `segoeui-changer.exe` + `scripts/` + `README.md` into `segoeui-changer-windows-x64.zip` (published automatically to GitHub Releases on `v*` tags).
