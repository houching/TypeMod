use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::ui::theme::{ThemeMode, VantageTheme};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavTab {
    #[default]
    Home,
    Preview,
    Resizer,
    Settings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecondaryScript {
    #[default]
    Khmer,
    Arabic,
    Thai,
    Lao,
    Myanmar,
    Vietnamese,
    Cjk,
}

impl SecondaryScript {
    pub const ALL: [Self; 7] = [
        Self::Khmer,
        Self::Arabic,
        Self::Thai,
        Self::Lao,
        Self::Myanmar,
        Self::Vietnamese,
        Self::Cjk,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Khmer => "Khmer",
            Self::Arabic => "Arabic",
            Self::Thai => "Thai",
            Self::Lao => "Lao",
            Self::Myanmar => "Myanmar",
            Self::Vietnamese => "Vietnamese",
            Self::Cjk => "CJK",
        }
    }

    pub fn ranges(self) -> &'static [(u32, u32)] {
        match self {
            Self::Khmer => &[(0x1780, 0x17FF), (0x19E0, 0x19FF)],
            Self::Arabic => &[
                (0x0600, 0x06FF),
                (0x0750, 0x077F),
                (0x08A0, 0x08FF),
                (0xFB50, 0xFDFF),
                (0xFE70, 0xFEFF),
            ],
            Self::Thai => &[(0x0E00, 0x0E7F)],
            Self::Lao => &[(0x0E80, 0x0EFF)],
            Self::Myanmar => &[(0x1000, 0x109F)],
            Self::Vietnamese => &[(0x1E00, 0x1EFF), (0x0102, 0x0103), (0x0110, 0x0111)],
            Self::Cjk => &[(0x3000, 0x30FF), (0x4E00, 0x9FFF)],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergingMode {
    #[default]
    Visual,
    GridSync,
}

impl MergingMode {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Visual => "visual",
            Self::GridSync => "grid_sync",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Visual => "Match Segoe Size (Recommended)",
            Self::GridSync => "Native Proportions (Grid Sync)",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FontFamilySelection {
    pub regular: Option<PathBuf>,
    pub light: Option<PathBuf>,
    pub semilight: Option<PathBuf>,
    pub semibold: Option<PathBuf>,
    pub bold: Option<PathBuf>,
    pub black: Option<PathBuf>,
    pub is_variable: bool,
    pub display_name: Option<String>,
}

impl FontFamilySelection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn has_regular(&self) -> bool {
        self.regular.as_ref().is_some_and(|p| p.exists())
    }
}

fn default_ui_scale_pct() -> u16 {
    100
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedConfig {
    pub theme: ThemeMode,
    #[serde(default = "default_ui_scale_pct")]
    pub ui_scale_pct: u16,
    pub merging_mode: MergingMode,
    pub enable_italic: bool,
    pub clone_segoe: bool,
    pub save_log: bool,
    pub save_fonts: bool,
    pub show_log_drawer: bool,
    pub rail_pinned: bool,
    pub latin: FontFamilySelection,
    #[serde(default)]
    pub secondary_script: SecondaryScript,
    #[serde(default)]
    pub secondary_scripts: Vec<SecondaryScript>,
    #[serde(default)]
    pub custom_fontforge_path: Option<PathBuf>,
    pub khmer: FontFamilySelection,
    pub latin_italic: FontFamilySelection,
    pub khmer_italic: FontFamilySelection,
    #[serde(default)]
    pub extra_script_fonts: HashMap<SecondaryScript, FontFamilySelection>,
    #[serde(default)]
    pub extra_script_italic_fonts: HashMap<SecondaryScript, FontFamilySelection>,
}

impl Default for PersistedConfig {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            ui_scale_pct: default_ui_scale_pct(),
            merging_mode: MergingMode::Visual,
            enable_italic: false,
            clone_segoe: false,
            save_log: false,
            save_fonts: false,
            show_log_drawer: false,
            rail_pinned: true,
            latin: FontFamilySelection::default(),
            secondary_script: SecondaryScript::Khmer,
            secondary_scripts: vec![SecondaryScript::Khmer],
            custom_fontforge_path: None,
            khmer: FontFamilySelection::default(),
            latin_italic: FontFamilySelection::default(),
            khmer_italic: FontFamilySelection::default(),
            extra_script_fonts: HashMap::new(),
            extra_script_italic_fonts: HashMap::new(),
        }
    }
}

impl PersistedConfig {
    pub fn active_scripts(&self) -> Vec<SecondaryScript> {
        if self.secondary_scripts.is_empty() {
            vec![self.secondary_script]
        } else {
            self.secondary_scripts.clone()
        }
    }

    pub fn active_script_ranges(&self) -> Vec<(u32, u32)> {
        let mut ranges = Vec::new();
        for script in self.active_scripts() {
            ranges.extend_from_slice(script.ranges());
        }
        ranges
    }

    pub fn active_scripts_summary(&self) -> String {
        let active = self.active_scripts();
        match active.len() {
            0 => SecondaryScript::Khmer.label().to_string(),
            1 => active[0].label().to_string(),
            2 => format!("{}, {}", active[0].label(), active[1].label()),
            n => format!("{}, {} +{}", active[0].label(), active[1].label(), n - 2),
        }
    }

    pub fn toggle_secondary_script(&mut self, script: SecondaryScript) {
        let mut list = self.active_scripts();
        if let Some(pos) = list.iter().position(|&s| s == script) {
            if list.len() > 1 {
                list.remove(pos);
            }
        } else {
            list.push(script);
        }
        self.secondary_script = list[0];
        self.secondary_scripts = list;
    }

    pub fn script_font(&self, script: SecondaryScript) -> Option<&FontFamilySelection> {
        let primary = self.active_scripts().into_iter().next().unwrap_or(SecondaryScript::Khmer);
        if script == primary {
            Some(&self.khmer)
        } else {
            self.extra_script_fonts.get(&script)
        }
    }

    pub fn script_font_mut(&mut self, script: SecondaryScript) -> &mut FontFamilySelection {
        let primary = self.active_scripts().into_iter().next().unwrap_or(SecondaryScript::Khmer);
        if script == primary {
            &mut self.khmer
        } else {
            self.extra_script_fonts.entry(script).or_default()
        }
    }

    pub fn script_italic_font(&self, script: SecondaryScript) -> Option<&FontFamilySelection> {
        let primary = self.active_scripts().into_iter().next().unwrap_or(SecondaryScript::Khmer);
        if script == primary {
            Some(&self.khmer_italic)
        } else {
            self.extra_script_italic_fonts.get(&script)
        }
    }

    pub fn script_italic_font_mut(&mut self, script: SecondaryScript) -> &mut FontFamilySelection {
        let primary = self.active_scripts().into_iter().next().unwrap_or(SecondaryScript::Khmer);
        if script == primary {
            &mut self.khmer_italic
        } else {
            self.extra_script_italic_fonts.entry(script).or_default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemFontState {
    Original,
    Modded,
    PendingRestoreReboot,
}

impl SystemFontState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "STOCK SEGOE UI",
            Self::Modded => "MODDED ACTIVE",
            Self::PendingRestoreReboot => "REBOOT PENDING",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemFontSizes {
    pub caption: i32,
    pub sm_caption: i32,
    pub menu: i32,
    pub status: i32,
    pub message: i32,
    pub icon: i32,
}

impl Default for SystemFontSizes {
    fn default() -> Self {
        Self {
            caption: 9,
            sm_caption: 9,
            menu: 9,
            status: 9,
            message: 9,
            icon: 9,
        }
    }
}

pub struct AppState {
    pub config: PersistedConfig,
    pub active_tab: NavTab,
    pub is_admin: bool,
    pub system_state: SystemFontState,
    pub clones_installed: bool,
    pub fontforge_path: Option<PathBuf>,
    pub is_busy: bool,
    pub cancel_flag: Option<Arc<AtomicBool>>,
    pub progress: f32,
    pub status_text: String,
    pub build_log: Vec<String>,
    pub last_error: Option<String>,
    pub last_notice: Option<String>,
    pub sizes: SystemFontSizes,
    pub uniform_pt: i32,
}

impl AppState {
    pub fn new(state_override: Option<&Path>) -> Self {
        let config = state_override
            .and_then(|p| Self::load_config_from(p).ok())
            .or_else(|| Self::load_default_config().ok())
            .unwrap_or_default();

        Self::from_config(config)
    }

    pub fn from_config(config: PersistedConfig) -> Self {
        Self {
            config,
            active_tab: NavTab::Home,
            is_admin: false,
            system_state: SystemFontState::Original,
            clones_installed: false,
            fontforge_path: None,
            is_busy: false,
            cancel_flag: None,
            progress: 0.0,
            status_text: "Ready".to_string(),
            build_log: Vec::new(),
            last_error: None,
            last_notice: None,
            sizes: SystemFontSizes::default(),
            uniform_pt: 9,
        }
    }

    pub fn theme(&self) -> VantageTheme {
        VantageTheme::from_mode(self.config.theme)
    }

    pub fn clamped_ui_scale_pct(&self) -> u16 {
        self.config.ui_scale_pct.clamp(80, 200)
    }

    pub fn ui_rem_size(&self) -> f32 {
        16.0 * (self.clamped_ui_scale_pct() as f32 / 100.0)
    }

    pub fn app_docs_dir() -> PathBuf {
        let home = std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join("Documents").join("SegoeUIChanger")
    }

    pub fn config_path() -> PathBuf {
        Self::app_docs_dir().join("config.json")
    }

    pub fn save_config(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.config)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn save_temp_state(&self) -> anyhow::Result<PathBuf> {
        let dir = Self::app_docs_dir();
        fs::create_dir_all(&dir)?;
        let temp_path = dir.join(format!("temp_state_{}.json", std::process::id()));
        let json = serde_json::to_string_pretty(&self.config)?;
        fs::write(&temp_path, json)?;
        Ok(temp_path)
    }

    pub fn load_default_config() -> anyhow::Result<PersistedConfig> {
        Self::load_config_from(&Self::config_path())
    }

    pub fn load_config_from(path: &Path) -> anyhow::Result<PersistedConfig> {
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        self.build_log.push(line.into());
        if self.build_log.len() > 2000 {
            let drain_count = self.build_log.len() - 2000;
            self.build_log.drain(0..drain_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_scale_clamps_between_80_and_200() {
        let mut s = AppState::from_config(PersistedConfig::default());
        assert_eq!(s.clamped_ui_scale_pct(), 100);
        s.config.ui_scale_pct = 50;
        assert_eq!(s.clamped_ui_scale_pct(), 80);
        s.config.ui_scale_pct = 260;
        assert_eq!(s.clamped_ui_scale_pct(), 200);
        assert!((s.ui_rem_size() - 32.0).abs() < f32::EPSILON);
    }
}
