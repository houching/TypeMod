use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Graphics::Gdi::LOGFONTW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    NONCLIENTMETRICSW, SPI_GETICONTITLELOGFONT, SPI_GETNONCLIENTMETRICS, SPI_SETICONTITLELOGFONT,
    SPI_SETNONCLIENTMETRICS, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
};

use crate::app_state::SystemFontSizes;

pub fn system_dpi() -> f64 {
    let dpi = unsafe { GetDpiForSystem() };
    if dpi == 0 { 96.0 } else { dpi as f64 }
}

pub fn pt_to_height(pt: i32) -> i32 {
    pt_to_height_dpi(pt, 96.0)
}

pub fn height_to_pt(height: i32) -> i32 {
    height_to_pt_dpi(height, 96.0)
}

pub fn pt_to_height_dpi(pt: i32, dpi: f64) -> i32 {
    -((pt as f64 * dpi) / 72.0).round() as i32
}

pub fn height_to_pt_dpi(height: i32, dpi: f64) -> i32 {
    if height == 0 {
        return 9;
    }
    ((height.abs() as f64 * 72.0) / dpi).round() as i32
}

pub fn get_current_metrics() -> anyhow::Result<SystemFontSizes> {
    let dpi = system_dpi();
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };

    unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            Default::default(),
        )?;
    }

    let mut lf_icon = LOGFONTW::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETICONTITLELOGFONT,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf_icon as *mut LOGFONTW as *mut c_void),
            Default::default(),
        )?;
    }

    Ok(SystemFontSizes {
        caption: height_to_pt_dpi(ncm.lfCaptionFont.lfHeight, dpi).clamp(6, 24),
        sm_caption: height_to_pt_dpi(ncm.lfSmCaptionFont.lfHeight, dpi).clamp(6, 24),
        menu: height_to_pt_dpi(ncm.lfMenuFont.lfHeight, dpi).clamp(6, 24),
        status: height_to_pt_dpi(ncm.lfStatusFont.lfHeight, dpi).clamp(6, 24),
        message: height_to_pt_dpi(ncm.lfMessageFont.lfHeight, dpi).clamp(6, 24),
        icon: height_to_pt_dpi(lf_icon.lfHeight, dpi).clamp(6, 24),
    })
}

pub fn apply_system_metrics(sizes: &SystemFontSizes) -> anyhow::Result<()> {
    let dpi = system_dpi();
    let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;

    let mut ncm = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };

    unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            Default::default(),
        )?;
    }

    ncm.lfCaptionFont.lfHeight = pt_to_height_dpi(sizes.caption.clamp(6, 24), dpi);
    ncm.lfSmCaptionFont.lfHeight = pt_to_height_dpi(sizes.sm_caption.clamp(6, 24), dpi);
    ncm.lfMenuFont.lfHeight = pt_to_height_dpi(sizes.menu.clamp(6, 24), dpi);
    ncm.lfStatusFont.lfHeight = pt_to_height_dpi(sizes.status.clamp(6, 24), dpi);
    ncm.lfMessageFont.lfHeight = pt_to_height_dpi(sizes.message.clamp(6, 24), dpi);

    unsafe {
        SystemParametersInfoW(
            SPI_SETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            flags,
        )?;
    }

    let mut lf_icon = LOGFONTW::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETICONTITLELOGFONT,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf_icon as *mut LOGFONTW as *mut c_void),
            Default::default(),
        )?;
    }

    lf_icon.lfHeight = pt_to_height_dpi(sizes.icon.clamp(6, 24), dpi);

    unsafe {
        SystemParametersInfoW(
            SPI_SETICONTITLELOGFONT,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf_icon as *mut LOGFONTW as *mut c_void),
            flags,
        )?;
    }

    Ok(())
}

pub fn apply_entire_size(pt: i32) -> anyhow::Result<SystemFontSizes> {
    let pt = pt.clamp(6, 24);
    let sizes = SystemFontSizes {
        caption: pt,
        sm_caption: pt,
        menu: pt,
        status: pt,
        message: pt,
        icon: pt,
    };
    apply_system_metrics(&sizes)?;
    Ok(sizes)
}

fn write_face_name(buf: &mut [u16; 32], face: &str) {
    buf.fill(0);
    for (dst, src) in buf.iter_mut().zip(face.encode_utf16().take(31)) {
        *dst = src;
    }
}

/// Updates all Windows `NONCLIENTMETRICSW` and icon title `LOGFONTW` face names
/// (e.g. from `"Segoe UI Variable Text"` to `"Segoe UI"`) so Desktop & File Explorer
/// resolve to the patched `Segoe UI` font files.
pub fn set_system_font_face(face_name: &str) -> anyhow::Result<()> {
    let flags = SPIF_UPDATEINIFILE | SPIF_SENDCHANGE;

    let mut ncm = NONCLIENTMETRICSW {
        cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            Default::default(),
        )?;
    }

    write_face_name(&mut ncm.lfCaptionFont.lfFaceName, face_name);
    write_face_name(&mut ncm.lfSmCaptionFont.lfFaceName, face_name);
    write_face_name(&mut ncm.lfMenuFont.lfFaceName, face_name);
    write_face_name(&mut ncm.lfStatusFont.lfFaceName, face_name);
    write_face_name(&mut ncm.lfMessageFont.lfFaceName, face_name);

    unsafe {
        SystemParametersInfoW(
            SPI_SETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut NONCLIENTMETRICSW as *mut c_void),
            flags,
        )?;
    }

    let mut lf_icon = LOGFONTW::default();
    unsafe {
        SystemParametersInfoW(
            SPI_GETICONTITLELOGFONT,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf_icon as *mut LOGFONTW as *mut c_void),
            Default::default(),
        )?;
    }
    write_face_name(&mut lf_icon.lfFaceName, face_name);
    unsafe {
        SystemParametersInfoW(
            SPI_SETICONTITLELOGFONT,
            size_of::<LOGFONTW>() as u32,
            Some(&mut lf_icon as *mut LOGFONTW as *mut c_void),
            flags,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pt_height_roundtrip() {
        for pt in [8, 9, 10, 11, 12, 14, 16] {
            let h = pt_to_height(pt);
            assert!(h < 0);
            assert_eq!(height_to_pt(h), pt);
        }
    }

    #[test]
    fn dpi_aware_pt_height_roundtrip() {
        for dpi in [96.0, 144.0, 192.0] {
            for pt in [8, 9, 10, 12, 14] {
                let h = pt_to_height_dpi(pt, dpi);
                assert!(h < 0);
                assert_eq!(height_to_pt_dpi(h, dpi), pt);
            }
            assert_eq!(height_to_pt_dpi(0, dpi), 9);
        }
        assert_eq!(pt_to_height_dpi(9, 144.0), -18);
        assert_eq!(pt_to_height_dpi(9, 192.0), -24);
    }
}
