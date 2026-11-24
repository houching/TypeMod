# Changelog

All notable changes to TypeMod will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-11-24

### Changed
- Renamed the project, binary, and release artifacts from `segoeui-changer` to `typemod` (`typemod.exe`, `typemod-windows-x64.zip`).
- Replaced app icons, favicons, brand guidelines, and SVG/PNG logo assets with TypeMod branding.
- Split the workflow into distinct **Build**, **Preview**, and **Apply** stages.
- Updated UAC elevation actions to run in-place and fixed modal layout issues.
- Improved UI responsiveness across the home view, settings, font picker modal, and text inputs.
- Virtualized the log drawer, isolated scrollbar/cursor blink updates, and offloaded startup and config I/O.
- Added Apache-2.0 `LICENSE`, `THIRD_PARTY_LICENSES.md`, `CONTRIBUTING.md`, `.github/SECURITY.md`, issue templates, Windows `VERSIONINFO` metadata, and updated `README.md` + release packaging.
- Suppressed the console window in release builds.
- Omitted the redundant `scripts` folder from the Windows release zip package.
- Scoped release workflow permissions (`contents: write` on the release job) and refined CI caching.

### Security & Reliability
- Hardened registry, elevation, backup cloning, font discovery, and resizer handling along with edge-case fixes.
- Updated Python font engine and variable slicer scripts.

## [0.1.0] - 2026-11-23

### Added
- Initial release (`segoeui-changer`) with Rust + GPUI desktop UI, Windows system font replacement, multi-script merging, font preview, system font resizer, and Windows CI/release workflow.
