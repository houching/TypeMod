use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Graphics::Gdi::LOGFONTW;
use windows::Win32::UI::WindowsAndMessaging::{
    NONCLIENTMETRICSW, SPI_GETICONTITLELOGFONT, SPI_GETNONCLIENTMETRICS, SPI_SETICONTITLELOGFONT,
    SPI_SETNONCLIENTMETRICS, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
};

use crate::app_state::SystemFontSizes;

pub fn pt_to_height(pt: i32) -> i32 {
    -((pt as f64 * 96.0) / 72.0).round() as i32
}

pub fn height_to_pt(height: i32) -> i32 {
    ((height.abs() as f64 * 72.0) / 96.0).round() as i32
}

pub fn get_current_metrics() -> anyhow::Result<SystemFontSizes> {
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
        caption: height_to_pt(ncm.lfCaptionFont.lfHeight),
        sm_caption: height_to_pt(ncm.lfSmCaptionFont.lfHeight),
        menu: height_to_pt(ncm.lfMenuFont.lfHeight),
        status: height_to_pt(ncm.lfStatusFont.lfHeight),
        message: height_to_pt(ncm.lfMessageFont.lfHeight),
        icon: height_to_pt(lf_icon.lfHeight),
    })
}

pub fn apply_system_metrics(sizes: &SystemFontSizes) -> anyhow::Result<()> {
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

    ncm.lfCaptionFont.lfHeight = pt_to_height(sizes.caption);
    ncm.lfSmCaptionFont.lfHeight = pt_to_height(sizes.sm_caption);
    ncm.lfMenuFont.lfHeight = pt_to_height(sizes.menu);
    ncm.lfStatusFont.lfHeight = pt_to_height(sizes.status);
    ncm.lfMessageFont.lfHeight = pt_to_height(sizes.message);

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

    lf_icon.lfHeight = pt_to_height(sizes.icon);

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
}
