# Contributing to TypeMod

Thank you for your interest in contributing to TypeMod!

## Development Setup

- **OS:** Windows 10 / Windows 11 (x64)
- **Rust:** Stable toolchain (`rustup`)
- **FontForge (optional, for testing font builds):** `ffpython.exe` at `C:\Program Files\FontForgeBuilds\bin\ffpython.exe`

## Before Opening a Pull Request

Run tests, formatting, and lint checks locally:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Guidelines

- Keep changes focused and minimal; avoid unrelated refactoring.
- Never commit proprietary Windows font files (`.ttf` / `.otf`) or personal font backups to the repository.
- Match existing code style and conventions.
