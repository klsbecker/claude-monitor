//! Software drawing: GDI text masks, 32-bit DIB surfaces, alpha blending, icons, fonts and colors.

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE, TRUE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GdiFlush, GetTextExtentPoint32W, GetTextFaceW, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT, TextOutW,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO, ULW_ALPHA, UpdateLayeredWindow};
use windows::core::{PCWSTR, w};

use crate::registry::read_dword;
use crate::util::{log, wide};

pub type Rgb = (u8, u8, u8);

pub const ORANGE: Rgb = (217, 119, 87);

/// The app glyph; only its alpha is used, tinted like the text.
static GLYPH_ICO: &[u8] = include_bytes!("../res/glyph.ico");

/// Whether Windows uses the light theme for the taskbar.
pub fn light_taskbar() -> bool {
    read_dword(w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"), "SystemUsesLightTheme") == Some(1)
}

pub fn low_color(light: bool) -> Rgb {
    if light { (196, 43, 28) } else { (255, 99, 97) }
}

/// `fg`, or the alert color once `pct` reaches `red`.
pub fn level_color(pct: Option<f64>, red: u32, light: bool, fg: Rgb) -> Rgb {
    match pct {
        Some(p) if p >= red as f64 => low_color(light),
        _ => fg,
    }
}

pub struct Theme {
    pub light: bool,
    pub bg: Rgb,
    pub track: Rgb,
    pub fg: Rgb,
    pub dim: Rgb,
    pub border: Rgb,
}

pub fn theme(light: bool) -> Theme {
    if light {
        Theme { light, bg: (243, 243, 243), track: (214, 214, 214), fg: (0, 0, 0), dim: (96, 96, 96), border: (200, 200, 200) }
    } else {
        Theme { light, bg: (32, 32, 32), track: (70, 70, 70), fg: (255, 255, 255), dim: (160, 160, 160), border: (58, 58, 58) }
    }
}

// ---- masks and icons ----------------------------------------------------------------------------

/// Alpha coverage, `w` x `h`, row-major.
pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub a: Vec<u8>,
}

impl Mask {
    fn empty() -> Mask {
        Mask { w: 0, h: 0, a: Vec::new() }
    }
}

fn le16(d: &[u8], at: usize) -> usize {
    d.get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize)
}

fn le32(d: &[u8], at: usize) -> usize {
    d.get(at..at + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// The glyph at `size` px: the exact 32-bit entry of the .ico when there is one, else the next larger one
/// scaled down.
pub fn glyph(size: i32) -> Mask {
    let d = GLYPH_ICO;
    let mut best: Option<(usize, usize)> = None;
    for i in 0..le16(d, 4) {
        let e = 6 + 16 * i;
        let side = match d.get(e).copied().unwrap_or(0) {
            0 => 256,
            s => s as usize,
        };
        let (bpp, off) = (le16(d, e + 6), le32(d, e + 12));
        if bpp != 32 || d.get(off..off + 4) == Some(b"\x89PNG") {
            continue;
        }
        let better = match best {
            None => true,
            Some((b, _)) => {
                let fits = side >= size as usize;
                let best_fits = b >= size as usize;
                if fits { !best_fits || side < b } else { !best_fits && side > b }
            }
        };
        if better {
            best = Some((side, off));
        }
    }
    let Some((side, off)) = best else {
        return Mask::empty();
    };
    let pixels = off + le32(d, off);
    // BMP rows are bottom-up BGRA; alpha is byte 3.
    let alpha = |x: usize, y: usize| -> f32 { d.get(pixels + ((side - 1 - y) * side + x) * 4 + 3).copied().unwrap_or(0) as f32 };
    let n = size as usize;
    let ratio = side as f32 / n as f32;
    let mut a = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (sx0, sy0) = ((x as f32 * ratio) as usize, (y as f32 * ratio) as usize);
            let (sx1, sy1) =
                ((((x + 1) as f32 * ratio).ceil() as usize).min(side), (((y + 1) as f32 * ratio).ceil() as usize).min(side));
            let mut sum = 0.0;
            let mut count = 0.0;
            for sy in sy0..sy1.max(sy0 + 1) {
                for sx in sx0..sx1.max(sx0 + 1) {
                    sum += alpha(sx, sy);
                    count += 1.0;
                }
            }
            a.push((sum / count) as u8);
        }
    }
    Mask { w: size, h: size, a }
}

/// The glyph in orange as an HICON, for notifications.
pub fn glyph_hicon(size: i32) -> Option<HICON> {
    mask_hicon(&glyph(size), ORANGE)
}

/// Ink coverage of `text`, the full cell height so pieces share a baseline. ClearType text is drawn white on
/// black (black on white for a light taskbar) and the coverage is the RGB average.
pub fn text_mask(text: &str, font: HFONT, light: bool) -> Mask {
    let Some(probe) = Surface::new(1, 1) else {
        return Mask::empty();
    };
    let sz = probe.measure(font, text);
    drop(probe);
    let Some(mut s) = Surface::new(sz.cx.max(1), sz.cy.max(1)) else {
        return Mask::empty();
    };
    let (bg, ink) = if light { ((255, 255, 255), (0, 0, 0)) } else { ((0, 0, 0), (255, 255, 255)) };
    let (w, h) = (s.w, s.h);
    s.canvas().fill(0, 0, w, h, bg);
    s.text(font, 0, 0, text, ink);
    let a = s
        .canvas()
        .px
        .iter()
        .map(|p| {
            let c = (p[0] as u16 + p[1] as u16 + p[2] as u16) / 3;
            (if light { 255 - c } else { c }) as u8
        })
        .collect();
    Mask { w: s.w, h: s.h, a }
}

/// The mask cut down to the pixels that have ink (GDI's text cell includes the font's leading).
pub fn ink_only(m: &Mask) -> Mask {
    let ink = |x: i32, y: i32| m.a[(y * m.w + x) as usize] > 8;
    let rows: Vec<i32> = (0..m.h).filter(|&y| (0..m.w).any(|x| ink(x, y))).collect();
    let cols: Vec<i32> = (0..m.w).filter(|&x| (0..m.h).any(|y| ink(x, y))).collect();
    let (Some(&y0), Some(&y1), Some(&x0), Some(&x1)) = (rows.first(), rows.last(), cols.first(), cols.last()) else {
        return Mask::empty();
    };
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let a = (y0..=y1).flat_map(|y| (x0..=x1).map(move |x| (x, y))).map(|(x, y)| m.a[(y * m.w + x) as usize]).collect();
    Mask { w, h, a }
}

/// The largest font size at which every one of `texts` (ink only) fits `max_w` x `max_h`, and the masks.
pub fn fitted_texts(texts: &[&str], weight: i32, max_w: i32, max_h: i32, light: bool) -> Vec<Mask> {
    let mut px = max_h * 2;
    loop {
        let font = Font::text(px, weight);
        let masks: Vec<Mask> = texts.iter().map(|t| ink_only(&text_mask(t, font.0, light))).collect();
        if masks.iter().all(|m| m.w <= max_w && m.h <= max_h) || px <= 5 {
            return masks;
        }
        px -= 1;
    }
}

/// The notification-area icon: the percentages stacked, session on top and week under it, each in its own
/// color; a single one fills the square.
pub fn stacked_hicon(lines: &[(String, Rgb)], size: i32) -> Option<HICON> {
    let light = light_taskbar();
    let n = lines.len().max(1) as i32;
    let gap = if n > 1 { 1 } else { 0 };
    let line_h = (size - gap * (n - 1)) / n;
    // One font size for all lines, so the two numbers read as a pair.
    let texts: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    let masks: Vec<(Mask, Rgb)> =
        fitted_texts(&texts, 700, size, line_h, light).into_iter().zip(lines.iter().map(|l| l.1)).collect();
    let mut px = vec![[0u8; 4]; (size * size) as usize];
    let mut put = |m: &Mask, ox: i32, oy: i32, (r, g, b): Rgb| {
        for y in 0..m.h {
            for x in 0..m.w {
                let (tx, ty) = (x + ox, y + oy);
                let a = m.a[(y * m.w + x) as usize];
                if a > 0 && tx >= 0 && ty >= 0 && tx < size && ty < size {
                    px[(ty * size + tx) as usize] = [b, g, r, a];
                }
            }
        }
    };
    // Each line keeps its own slot (bottom-aligned), so the two numbers sit at fixed heights.
    for (i, (m, color)) in masks.iter().enumerate() {
        let slot_top = i as i32 * (line_h + gap);
        put(m, (size - m.w) / 2, slot_top + (line_h - m.h), *color);
    }
    pixels_hicon(size, size, &px)
}

pub fn mask_hicon(m: &Mask, (r, g, b): Rgb) -> Option<HICON> {
    let px: Vec<[u8; 4]> = m.a.iter().map(|a| [b, g, r, *a]).collect();
    pixels_hicon(m.w, m.h, &px)
}

/// An icon from straight (not premultiplied) BGRA pixels.
pub fn pixels_hicon(w: i32, h: i32, pixels: &[[u8; 4]]) -> Option<HICON> {
    unsafe {
        let bmi = dib_info(w, h);
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        std::slice::from_raw_parts_mut(bits as *mut [u8; 4], (w * h) as usize).copy_from_slice(pixels);
        let mask = CreateBitmap(w, h, 1, 1, None);
        let info = ICONINFO { fIcon: TRUE, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: bmp };
        let icon = CreateIconIndirect(&info).ok();
        let _ = DeleteObject(bmp.into());
        let _ = DeleteObject(mask.into());
        icon
    }
}

/// A top-down 32-bit DIB of `w` x `h`.
fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

// ---- blending -----------------------------------------------------------------------------------

pub fn premul((r, g, b): Rgb, a: u8) -> [u8; 4] {
    let m = |c: u8| (c as u16 * a as u16 / 255) as u8;
    [m(b), m(g), m(r), a]
}

/// Premultiplied `src` over premultiplied `dst`.
pub fn over(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let inv = 255 - src[3] as u16;
    let ch = |i: usize| (src[i] as u16 + dst[i] as u16 * inv / 255).min(255) as u8;
    [ch(0), ch(1), ch(2), ch(3)]
}

/// Blends `color` over an opaque pixel.
pub fn blend(p: &mut [u8; 4], (r, g, b): Rgb, a: f32) {
    let a = a.clamp(0.0, 1.0);
    let mix = |dst: u8, src: u8| (dst as f32 + (src as f32 - dst as f32) * a).round() as u8;
    *p = [mix(p[0], b), mix(p[1], g), mix(p[2], r), p[3]];
}

/// Coverage of a rounded rectangle `(x, y, w, h)` at pixel (x, y).
pub fn round_rect_cov(x: i32, y: i32, (rx, ry, rw, rh): (f32, f32, f32, f32), radius: f32) -> f32 {
    let (fx, fy) = (x as f32 + 0.5 - rx, y as f32 + 0.5 - ry);
    let dx = (radius - fx).max(fx - (rw - radius)).max(0.0);
    let dy = (radius - fy).max(fy - (rh - radius)).max(0.0);
    if dx > 0.0 && dy > 0.0 {
        (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0)
    } else {
        let edge = fx.min(fy).min(rw - fx).min(rh - fy);
        (edge + 0.5).clamp(0.0, 1.0)
    }
}

/// Pixels of a 32-bit DIB: `w` wide, row-major, BGRA.
pub struct Canvas<'a> {
    pub px: &'a mut [[u8; 4]],
    pub w: i32,
    pub h: i32,
}

impl Canvas<'_> {
    fn at(&mut self, x: i32, y: i32) -> Option<&mut [u8; 4]> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        self.px.get_mut((y * self.w + x) as usize)
    }

    pub fn fill(&mut self, x0: i32, y0: i32, w: i32, h: i32, color: Rgb) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                if let Some(p) = self.at(x, y) {
                    *p = [color.2, color.1, color.0, 255];
                }
            }
        }
    }

    /// A mask tinted with `color`, over transparent-capable (premultiplied) pixels.
    pub fn mask_alpha(&mut self, m: &Mask, x0: i32, y0: i32, color: Rgb, alpha: f32) {
        for y in 0..m.h {
            for x in 0..m.w {
                let a = (m.a[(y * m.w + x) as usize] as f32 * alpha) as u8;
                if a > 0
                    && let Some(p) = self.at(x0 + x, y0 + y)
                {
                    *p = over(*p, premul(color, a));
                }
            }
        }
    }

    /// A mask tinted with `color`, over opaque pixels.
    pub fn mask_over(&mut self, m: &Mask, x0: i32, y0: i32, color: Rgb) {
        for y in 0..m.h {
            for x in 0..m.w {
                let a = m.a[(y * m.w + x) as usize];
                if a > 0
                    && let Some(p) = self.at(x0 + x, y0 + y)
                {
                    blend(p, color, a as f32 / 255.0);
                }
            }
        }
    }

    /// Rounded rectangle over opaque pixels.
    pub fn round_rect(&mut self, rect: (f32, f32, f32, f32), radius: f32, color: Rgb) {
        let (x0, y0) = (rect.0.floor() as i32, rect.1.floor() as i32);
        let (x1, y1) = ((rect.0 + rect.2).ceil() as i32, (rect.1 + rect.3).ceil() as i32);
        for y in y0..y1 {
            for x in x0..x1 {
                let cov = round_rect_cov(x, y, rect, radius);
                if let Some(p) = self.at(x, y) {
                    blend(p, color, cov);
                }
            }
        }
    }

    /// Covers `[top, bottom)` of column `x` (fractional ends) with `color` at `alpha`.
    pub fn vspan(&mut self, x: i32, top: f32, bottom: f32, color: Rgb, alpha: f32) {
        if bottom <= top {
            return;
        }
        for y in top.floor() as i32..bottom.ceil() as i32 {
            let cov = (bottom.min(y as f32 + 1.0) - top.max(y as f32)).clamp(0.0, 1.0);
            if let Some(p) = self.at(x, y) {
                blend(p, color, cov * alpha);
            }
        }
    }
}

// ---- GDI ----------------------------------------------------------------------------------------

/// A memory DC with a top-down 32-bit DIB selected.
pub struct Surface {
    dc: HDC,
    bmp: HBITMAP,
    old_bmp: HGDIOBJ,
    bits: *mut c_void,
    pub w: i32,
    pub h: i32,
}

impl Surface {
    pub fn new(w: i32, h: i32) -> Option<Surface> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let bmi = dib_info(w, h);
            let mut bits: *mut c_void = std::ptr::null_mut();
            match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bmp) if !bits.is_null() => {
                    let old_bmp = SelectObject(dc, bmp.into());
                    SetBkMode(dc, TRANSPARENT);
                    Some(Surface { dc, bmp, old_bmp, bits, w, h })
                }
                _ => {
                    let _ = DeleteDC(dc);
                    None
                }
            }
        }
    }

    pub fn canvas(&mut self) -> Canvas<'_> {
        let _ = unsafe { GdiFlush() };
        let px = unsafe { std::slice::from_raw_parts_mut(self.bits as *mut [u8; 4], (self.w * self.h) as usize) };
        Canvas { px, w: self.w, h: self.h }
    }

    pub fn text(&self, font: HFONT, x: i32, y: i32, text: &str, color: Rgb) {
        let t: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            SelectObject(self.dc, font.into());
            SetTextColor(self.dc, COLORREF(color.0 as u32 | (color.1 as u32) << 8 | (color.2 as u32) << 16));
            let _ = TextOutW(self.dc, x, y, &t);
        }
    }

    pub fn measure(&self, font: HFONT, text: &str) -> SIZE {
        let t: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        unsafe {
            SelectObject(self.dc, font.into());
            let _ = GetTextExtentPoint32W(self.dc, &t, &mut sz);
        }
        sz
    }

    /// Shows the DIB on a layered window; `at` positions a top-level window.
    pub fn present(&self, hwnd: HWND, at: Option<POINT>) {
        let size = SIZE { cx: self.w, cy: self.h };
        let src = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let at_ptr = at.as_ref().map(|p| p as *const POINT);
        if let Err(e) = unsafe {
            UpdateLayeredWindow(hwnd, None, at_ptr, Some(&size), Some(self.dc), Some(&src), COLORREF(0), Some(&blend), ULW_ALPHA)
        } {
            log(&format!("UpdateLayeredWindow: {}", e.message()));
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old_bmp);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// A GDI font, deleted on drop.
pub struct Font(pub HFONT);

impl Font {
    /// A ClearType font in `face`, or in `fallback` when `face` is not installed (GDI would otherwise
    /// substitute another face silently).
    fn new(px: i32, weight: i32, face: &str, fallback: &str) -> Font {
        for f in [face, fallback] {
            let name = wide(f);
            let h = unsafe {
                CreateFontW(
                    -px,
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    CLEARTYPE_QUALITY,
                    0,
                    PCWSTR(name.as_ptr()),
                )
            };
            if f == fallback {
                return Font(h);
            }
            let mut got = [0u16; 64];
            unsafe {
                let dc = CreateCompatibleDC(None);
                let old = SelectObject(dc, h.into());
                let n = GetTextFaceW(dc, Some(&mut got)).max(1) as usize - 1;
                SelectObject(dc, old);
                let _ = DeleteDC(dc);
                if String::from_utf16_lossy(&got[..n.min(63)]).eq_ignore_ascii_case(f) {
                    return Font(h);
                }
                let _ = DeleteObject(h.into());
            }
        }
        unreachable!()
    }

    pub fn text(px: i32, weight: i32) -> Font {
        Font::new(px, weight, "Segoe UI Variable Text", "Segoe UI")
    }

    pub fn display(px: i32, weight: i32) -> Font {
        Font::new(px, weight, "Segoe UI Variable Display", "Segoe UI")
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_over_is_opaque_for_opaque_source() {
        assert_eq!(over([10, 20, 30, 40], premul((255, 0, 0), 255)), [0, 0, 255, 255]);
        assert_eq!(over([10, 20, 30, 40], [0, 0, 0, 0]), [10, 20, 30, 40]);
    }

    #[test]
    fn rounded_rect_coverage() {
        let r = (0.0, 0.0, 10.0, 10.0);
        assert_eq!(round_rect_cov(5, 5, r, 3.0), 1.0);
        assert_eq!(round_rect_cov(20, 5, r, 3.0), 0.0);
        assert!(round_rect_cov(0, 0, r, 3.0) < 0.5);
    }
}
