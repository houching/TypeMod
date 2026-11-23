use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub struct EmbeddedAssets;

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: Option<&'static [u8]> = match path {
            "icons/type.svg" => Some(include_bytes!("../../assets/icons/type.svg")),
            "icons/eye.svg" => Some(include_bytes!("../../assets/icons/eye.svg")),
            "icons/sliders-horizontal.svg" => {
                Some(include_bytes!("../../assets/icons/sliders-horizontal.svg"))
            }
            "icons/settings.svg" => Some(include_bytes!("../../assets/icons/settings.svg")),
            "icons/sun.svg" => Some(include_bytes!("../../assets/icons/sun.svg")),
            "icons/moon.svg" => Some(include_bytes!("../../assets/icons/moon.svg")),
            "icons/folder-open.svg" => Some(include_bytes!("../../assets/icons/folder-open.svg")),
            "icons/search.svg" => Some(include_bytes!("../../assets/icons/search.svg")),
            "icons/play.svg" => Some(include_bytes!("../../assets/icons/play.svg")),
            "icons/rotate-ccw.svg" => Some(include_bytes!("../../assets/icons/rotate-ccw.svg")),
            "icons/x.svg" => Some(include_bytes!("../../assets/icons/x.svg")),
            "icons/download.svg" => Some(include_bytes!("../../assets/icons/download.svg")),
            "icons/upload.svg" => Some(include_bytes!("../../assets/icons/upload.svg")),
            "icons/shield-check.svg" => Some(include_bytes!("../../assets/icons/shield-check.svg")),
            "icons/refresh-cw.svg" => Some(include_bytes!("../../assets/icons/refresh-cw.svg")),
            "icons/panel-left.svg" => Some(include_bytes!("../../assets/icons/panel-left.svg")),
            "icons/copy.svg" => Some(include_bytes!("../../assets/icons/copy.svg")),
            _ => None,
        };
        Ok(bytes.map(Cow::Borrowed))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}
