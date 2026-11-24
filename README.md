# TypeMod

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

## Quick Start

1. Download `typemod-windows-x64.zip` from [GitHub Releases](https://github.com/houching/TypeMod/releases).
2. *(Optional)* Verify the archive checksum against `typemod-windows-x64.zip.sha256`:
   ```powershell
   Get-FileHash .\typemod-windows-x64.zip -Algorithm SHA256
   ```
3. Extract the zip and run `typemod.exe`.
   > **SmartScreen Note:** `typemod.exe` is currently unsigned, so Windows Defender SmartScreen may display an "Unknown publisher" prompt on first launch. Click **More info → Run anyway** if you downloaded it directly from the official release and verified the SHA256 hash.
4. Select your Latin font (and any additional script fonts), click **Build**, inspect the result in **Preview**, and click **Apply** (prompts for UAC elevation).

## Recovery / Restoring Stock Fonts

- TypeMod automatically backs up your stock Windows `Segoe UI` files to `Original_Segoe_Backups/` before applying changes.
- To restore the original system fonts, open TypeMod and use the **Restore** action (or restore the backed-up `.ttf` files from `Original_Segoe_Backups/` with administrator privileges and sign out/reboot).

## Building from Source

```powershell
# Run tests and lint
cargo test
cargo clippy --all-targets -- -D warnings

# Run in release mode
cargo build --release
```

When distributing the compiled binary, `typemod.exe` bundles the Python scripts internally so it runs as a single standalone executable.

## CI / Releases

GitHub Actions (`.github/workflows/release.yml`) runs `cargo test`, `cargo clippy --all-targets -- -D warnings`, builds `typemod.exe` in release mode on `windows-latest`, and packages `typemod.exe`, `README.md`, `LICENSE`, and `THIRD_PARTY_LICENSES.md` into `typemod-windows-x64.zip` (published automatically to GitHub Releases on `v*` tags).

## Disclaimer & Font Licensing

- **Not affiliated with Microsoft:** TypeMod is an independent open-source utility and is not affiliated with, endorsed by, or sponsored by Microsoft Corporation. `Windows`, `Segoe UI`, and `Segoe UI Variable` are trademarks of Microsoft Corporation.
- **No proprietary fonts bundled:** TypeMod does not distribute Microsoft system fonts or third-party font files. It operates locally on fonts already installed or provided by the user.
- **Personal backup only for exported packs:** Users are solely responsible for complying with the license terms of any fonts they modify or export. Portable `.zip` font packs that contain proprietary or system fonts (such as `Segoe UI`) are intended strictly for **personal backup and local restoration** and must not be redistributed.

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for local setup and check guidelines, and [.github/SECURITY.md](.github/SECURITY.md) for reporting security issues.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). Third-party Rust crate attributions are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
