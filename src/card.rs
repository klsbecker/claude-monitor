//! The hover card: big numbers, progress bars and the usage charts of the session and the week.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GA_ROOT, GetAncestor, GetWindowRect, HWND_TOPMOST, IsWindow, KillTimer, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SetTimer, SetWindowPos, ShowWindow, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WindowFromPoint,
};
use windows::core::w;

use crate::api::{SESSION, SESSION_SECS, WEEK, WEEK_SECS, find_limit};
use crate::app::{App, CARD_CLASS, CARD_TIMER, cursor_pos};
use crate::chart::{self, Forecast};
use crate::gfx::{Font, ORANGE, Surface, blend, glyph, level_color, light_taskbar, low_color, premul, round_rect_cov, theme};
use crate::history::Sample;
use crate::i18n::{fill, tr};
use crate::time::{day_month, hm, now_unix, parse_iso_unix, unix_to_local, weekday, when};
use crate::util::log;

/// How long the card stays after the cursor leaves the strip and the card.
const GRACE: Duration = Duration::from_millis(300);

/// Which value of a sample a section plots.
type Pick = fn(&Sample) -> Option<f64>;

/// One limit's block on the card.
struct Section {
    label: &'static str,
    pct: Option<f64>,
    red: u32,
    t0: i64,
    t1: i64,
    known_reset: bool,
    points: Vec<(i64, f64)>,
    forecast: Option<Forecast>,
}

fn inside(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

impl App {
    pub fn show_card(&mut self) {
        unsafe {
            if self.card.is_invalid() || !IsWindow(Some(self.card)).as_bool() {
                match CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                    CARD_CLASS,
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(self.hinst),
                    None,
                ) {
                    Ok(card) => self.card = card,
                    Err(e) => {
                        log(&format!("card: {}", e.message()));
                        return;
                    }
                }
            }
        }
        let Some(surface) = self.render_card() else {
            return;
        };
        let (w, h) = (surface.w, surface.h);
        let anchor = self.anchor_rect();
        let mut bar = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.taskbar, &mut bar);
            let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(MonitorFromWindow(self.taskbar, MONITOR_DEFAULTTONEAREST), &mut mi);
            let work = mi.rcWork;
            let margin = (8.0 * self.dpi() as f32 / 96.0) as i32;
            // In icon mode the icon may sit in the overflow flyout, above the taskbar: the card then goes past
            // that whole window, so it never covers the icon or the flyout under the cursor.
            let mut avoid = bar;
            if self.settings.tray_icon {
                let root = GetAncestor(WindowFromPoint(self.tray_point), GA_ROOT);
                let mut r = RECT::default();
                if !root.is_invalid() && GetWindowRect(root, &mut r).is_ok() && r.right > r.left {
                    avoid = RECT {
                        left: avoid.left.min(r.left),
                        top: avoid.top.min(r.top),
                        right: avoid.right.max(r.right),
                        bottom: avoid.bottom.max(r.bottom),
                    };
                }
            }
            let x = ((anchor.left + anchor.right) / 2 - w / 2).clamp(work.left + margin, work.right - w - margin);
            let y = if bar.top <= work.top { avoid.bottom + margin } else { avoid.top - h - margin };
            let y = y.clamp(work.top, (work.bottom - h).max(work.top));
            surface.present(self.card, Some(POINT { x, y }));
            self.card_pos = POINT { x, y };
            if !self.card_shown {
                let _ = SetWindowPos(
                    self.card,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                SetTimer(Some(self.main), CARD_TIMER, 30, None);
            }
        }
        self.card_shown = true;
    }

    pub fn hide_card(&mut self) {
        if self.card_shown && !self.card.is_invalid() {
            unsafe {
                let _ = KillTimer(Some(self.main), CARD_TIMER);
                let _ = ShowWindow(self.card, SW_HIDE);
            }
        }
        self.card_shown = false;
        self.card_mouse = None;
        self.card_left_at = None;
    }

    /// Keeps the card open while the cursor is on the strip or the card (with a short grace period to cross
    /// the gap between them) and redraws it as the cursor moves over the charts.
    pub fn card_tick(&mut self) {
        if !self.card_shown {
            return;
        }
        let pt = cursor_pos();
        let mut card = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.card, &mut card);
        }
        let on_card = inside(&card, pt);
        let on_anchor = if self.settings.tray_icon {
            // NIN_POPUPCLOSE does not always come, so the cursor must also still be near the icon.
            let a = self.anchor_rect();
            let slack = (12.0 * self.dpi() as f32 / 96.0) as i32;
            let near = RECT { left: a.left - slack, top: a.top - slack, right: a.right + slack, bottom: a.bottom + slack };
            self.tray_hovering && inside(&near, pt)
        } else {
            inside(&self.anchor_rect(), pt)
        };
        if on_card || on_anchor {
            self.card_left_at = None;
        } else {
            let left = *self.card_left_at.get_or_insert_with(Instant::now);
            if left.elapsed() >= GRACE {
                self.hide_card();
            }
            return;
        }
        let mouse = on_card.then(|| (pt.x - self.card_pos.x, pt.y - self.card_pos.y));
        if mouse != self.card_mouse {
            self.card_mouse = mouse;
            self.show_card();
        }
    }

    fn section(&self, items: &[crate::api::Limit], history: &[Sample], key: &str, now: i64) -> Section {
        let (label, span, pick): (&'static str, i64, Pick) = match key {
            SESSION => (tr().session, SESSION_SECS, |s| s.1),
            _ => (tr().week, WEEK_SECS, |s| s.2),
        };
        let limit = find_limit(items, key);
        let known_reset = limit.as_ref().is_some_and(|l| l.reset.is_some());
        let t1 = limit.as_ref().and_then(|l| l.reset.as_deref()).and_then(parse_iso_unix).unwrap_or(now);
        let t0 = t1 - span;
        let mut points: Vec<(i64, f64)> = history.iter().filter_map(|s| pick(s).map(|v| (s.0, v))).collect();
        // Every window starts at 0 %, so the curve has a start even before the first reading.
        if known_reset {
            points.retain(|p| p.0 > t0);
            points.insert(0, (t0, 0.0));
        }
        let forecast = if known_reset { chart::forecast(&points, t0, t1) } else { None };
        let red = self.settings.limit(key).red;
        Section { label, pct: limit.map(|l| l.pct), red, t0, t1, known_reset, points, forecast }
    }

    fn render_card(&self) -> Option<Surface> {
        let scale = self.dpi() as f32 / 96.0;
        let px = |v: f32| (v * scale).round() as i32;
        let th = theme(light_taskbar());
        let (items, error, updated, history) = {
            let s = self.shared.lock().unwrap();
            (s.items.clone().unwrap_or_default(), s.error.clone(), s.updated, s.history.clone())
        };
        let now = now_unix();
        let sections = [self.section(&items, &history, SESSION, now), self.section(&items, &history, WEEK, now)];

        let (w, pad) = (px(300.0), px(16.0));
        let (title_h, label_h, big_h, bar_h, chart_h, axis_h, line_h, gap) =
            (px(26.0), px(18.0), px(40.0), px(4.0), px(52.0), px(20.0), px(18.0), px(18.0));
        let section_h = label_h + big_h + px(6.0) + bar_h + px(14.0) + chart_h + axis_h + line_h;
        let error_h = if error.is_some() { line_h } else { 0 };
        let n = sections.len() as i32;
        let h = pad + title_h + px(10.0) + n * section_h + (n - 1) * gap + error_h + pad;
        let mut surface = Surface::new(w, h)?;

        let title_font = Font::text(px(15.0), 600);
        let big_font = Font::display(px(30.0), 600);
        let small = Font::text(px(12.0), 400);
        surface.canvas().fill(0, 0, w, h, th.bg);

        let inner = w - 2 * pad;
        let mut y = pad;
        let icon = glyph(px(16.0));
        surface.canvas().mask_over(&icon, pad, y + (title_h - icon.h) / 2, ORANGE);
        let ts = surface.measure(title_font.0, "Claude");
        surface.text(title_font.0, pad + icon.w + px(8.0), y + (title_h - ts.cy) / 2, "Claude", th.fg);
        if let Some(t) = updated {
            let text = fill(tr().at_time, &[("time", &hm(&t))]);
            let us = surface.measure(small.0, &text);
            surface.text(small.0, pad + inner - us.cx, y + (title_h - us.cy) / 2, &text, th.dim);
        }
        y += title_h + px(10.0);

        for (i, s) in sections.iter().enumerate() {
            if i > 0 {
                y += gap;
            }
            surface.text(small.0, pad, y, s.label, th.dim);
            if s.known_reset {
                let reset = fill(tr().resets, &[("when", &when(s.t1))]);
                let rs = surface.measure(small.0, &reset);
                surface.text(small.0, pad + inner - rs.cx, y, &reset, th.dim);
            }
            y += label_h;

            let big = s.pct.map_or("--".to_owned(), |p| format!("{p:.0}%"));
            let color = level_color(s.pct, s.red, th.light, th.fg);
            surface.text(big_font.0, pad - px(1.0), y, &big, color);
            y += big_h + px(6.0);

            {
                let mut c = surface.canvas();
                let r = bar_h as f32 / 2.0;
                c.round_rect((pad as f32, y as f32, inner as f32, bar_h as f32), r, th.track);
                let fill_w = (inner as f64 * s.pct.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0) as f32;
                if fill_w > 0.0 {
                    let accent = if s.pct.is_some_and(|p| p >= s.red as f64) { low_color(th.light) } else { ORANGE };
                    c.round_rect((pad as f32, y as f32, fill_w.max(bar_h as f32), bar_h as f32), r, accent);
                }
            }
            y += bar_h + px(14.0);

            {
                let mut c = surface.canvas();
                chart::draw(&mut c, (pad, y, inner, chart_h), (s.t0, s.t1, now), &s.points, s.forecast, &th, scale);
            }
            let hovered = self.card_mouse.filter(|&(mx, my)| {
                mx >= pad - px(4.0) && mx <= pad + inner + px(4.0) && my >= y - px(6.0) && my < y + chart_h + axis_h
            });
            if let Some((mx, _)) = hovered {
                let span = (s.t1 - s.t0).max(1);
                let at = s.t0 + ((mx - pad).clamp(0, inner) as f64 / inner as f64 * span as f64) as i64;
                if let Some((t, v, predicted)) = chart::hover_point(&s.points, s.t0, s.t1, s.forecast, at) {
                    let fx = pad as f32 + (t - s.t0) as f32 / span as f32 * inner as f32;
                    let fy = y as f32 + (1.0 - (v.clamp(0.0, 100.0) / 100.0) as f32) * chart_h as f32;
                    {
                        let mut c = surface.canvas();
                        c.vspan(fx.round() as i32, y as f32, (y + chart_h) as f32, th.fg, 0.35);
                        let d = 7.0 * scale;
                        c.round_rect((fx - d / 2.0, fy - d / 2.0, d, d), d / 2.0, th.bg);
                        let d2 = 5.0 * scale;
                        c.round_rect((fx - d2 / 2.0, fy - d2 / 2.0, d2, d2), d2 / 2.0, ORANGE);
                    }
                    let st = unix_to_local(t);
                    let when_text =
                        if span > 86400 { format!("{} {} {}", weekday(&st), day_month(&st), hm(&st)) } else { hm(&st) };
                    let value =
                        if predicted { fill(tr().forecast_value, &[("pct", &format!("{v:.0}"))]) } else { format!("{v:.0}%") };
                    let text = format!("{when_text}  ·  {value}");
                    let ts = surface.measure(small.0, &text);
                    let (pw, ph) = (ts.cx + px(12.0), ts.cy + px(4.0));
                    let left = (fx as i32 - pw / 2).clamp(pad, pad + inner - pw);
                    let top = y + chart_h + px(1.0);
                    surface.canvas().round_rect((left as f32, top as f32, pw as f32, ph as f32), ph as f32 / 2.0, th.track);
                    surface.text(small.0, left + px(6.0), top + px(2.0), &text, th.fg);
                }
            }
            y += chart_h;

            let axis_label = |t: i64| {
                let d = unix_to_local(t);
                if s.t1 - s.t0 > 86400 { day_month(&d) } else { hm(&d) }
            };
            if hovered.is_none() {
                let (left, right) = (axis_label(s.t0), axis_label(s.t1));
                let ls = surface.measure(small.0, &left);
                let rs = surface.measure(small.0, &right);
                let ns = surface.measure(small.0, tr().now);
                surface.text(small.0, pad, y + px(4.0), &left, th.dim);
                surface.text(small.0, pad + inner - rs.cx, y + px(4.0), &right, th.dim);
                let now_x = pad + ((now - s.t0) as f32 / (s.t1 - s.t0).max(1) as f32 * inner as f32) as i32 - ns.cx / 2;
                if now_x > pad + ls.cx + px(8.0) && now_x + ns.cx < pad + inner - rs.cx - px(8.0) {
                    surface.text(small.0, now_x, y + px(4.0), tr().now, th.dim);
                }
            }
            y += axis_h;

            let note = match s.forecast {
                Some((_, Some(full))) => fill(tr().pace_full, &[("when", &when(full))]),
                Some((at, None)) => fill(tr().pace_at_reset, &[("pct", &format!("{at:.0}"))]),
                None => String::new(),
            };
            surface.text(small.0, pad, y, &note, th.dim);
            y += line_h;
        }
        if let Some(e) = &error {
            surface.text(small.0, pad, y, &fill(tr().error, &[("error", e)]), low_color(th.light));
        }

        // GDI zeroes alpha where it draws: make the card opaque, with rounded corners and a 1px border.
        let radius = 8.0 * scale;
        let rect = (0.0, 0.0, w as f32, h as f32);
        let inner_rect = (1.0, 1.0, w as f32 - 2.0, h as f32 - 2.0);
        let c = surface.canvas();
        for py in 0..h {
            for qx in 0..w {
                let i = (py * w + qx) as usize;
                let outer = round_rect_cov(qx, py, rect, radius);
                let within = round_rect_cov(qx, py, inner_rect, radius - 1.0);
                let p = c.px[i];
                let mut rgb = (p[2], p[1], p[0]);
                if within < 1.0 {
                    let mut q = [p[0], p[1], p[2], 255];
                    blend(&mut q, th.border, 1.0 - within);
                    rgb = (q[2], q[1], q[0]);
                }
                c.px[i] = premul(rgb, (outer * 255.0) as u8);
            }
        }
        Some(surface)
    }
}
