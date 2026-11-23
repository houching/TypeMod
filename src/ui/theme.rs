use gpui::{Hsla, rgb, rgba};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
}

/// Vantage Graphite design tokens (`--v-*` palette & geometry).
#[derive(Clone, Copy, Debug)]
pub struct VantageTheme {
    pub mode: ThemeMode,
    pub bg: Hsla,
    pub surface: Hsla,
    pub surface_2: Hsla,
    pub surface_3: Hsla,
    pub line: Hsla,
    pub line_2: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    pub text_3: Hsla,
    pub accent: Hsla,
    pub accent_ink: Hsla,
    pub accent_soft: Hsla,
    pub ok: Hsla,
    pub ok_soft: Hsla,
    pub warn: Hsla,
    pub warn_soft: Hsla,
    pub err: Hsla,
    pub err_soft: Hsla,
    pub info: Hsla,
    pub info_soft: Hsla,
}

impl VantageTheme {
    pub fn from_mode(mode: ThemeMode) -> Self {
        match mode {
            ThemeMode::Dark => Self::dark(),
            ThemeMode::Light => Self::light(),
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,
            bg: rgb(0x0d0f12).into(),
            surface: rgb(0x15181d).into(),
            surface_2: rgb(0x1c2026).into(),
            surface_3: rgb(0x242931).into(),
            line: rgb(0x2a303a).into(),
            line_2: rgb(0x38404d).into(),
            text: rgb(0xedeef0).into(),
            text_2: rgb(0x9da4b0).into(),
            text_3: rgb(0x7a828e).into(),
            accent: rgb(0x4c77f7).into(),
            accent_ink: rgb(0xffffff).into(),
            accent_soft: rgba(0x4c77f72e).into(),
            ok: rgb(0x16a37f).into(),
            ok_soft: rgba(0x16a37f2e).into(),
            warn: rgb(0xd9822b).into(),
            warn_soft: rgba(0xd9822b2e).into(),
            err: rgb(0xe55353).into(),
            err_soft: rgba(0xe553532e).into(),
            info: rgb(0x3b82f6).into(),
            info_soft: rgba(0x3b82f62e).into(),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,
            bg: rgb(0xf3f4f6).into(),
            surface: rgb(0xffffff).into(),
            surface_2: rgb(0xf6f7f9).into(),
            surface_3: rgb(0xeceef1).into(),
            line: rgb(0xe2e5e9).into(),
            line_2: rgb(0xcbd0d6).into(),
            text: rgb(0x15181c).into(),
            text_2: rgb(0x4b525b).into(),
            text_3: rgb(0x5f666f).into(),
            accent: rgb(0x3563e9).into(),
            accent_ink: rgb(0xffffff).into(),
            accent_soft: rgba(0x3563e91f).into(),
            ok: rgb(0x0c7f63).into(),
            ok_soft: rgba(0x0c7f631c).into(),
            warn: rgb(0x9a5b00).into(),
            warn_soft: rgba(0xd68a0026).into(),
            err: rgb(0xc42b2b).into(),
            err_soft: rgba(0xc42b2b1a).into(),
            info: rgb(0x1f63c6).into(),
            info_soft: rgba(0x1f63c61a).into(),
        }
    }
}
