//! The usage charts on the card: the curve, its forecast and the reading under the cursor.

use crate::gfx::{Canvas, ORANGE, Theme};

/// A forecast: usage at the reset if the current pace holds, and when it reaches 100 % (if before the reset).
pub type Forecast = (f64, Option<i64>);

/// Plots `points` (unix seconds, %) over `[t0, t1]` and 0..100 %, with the forecast dashed.
pub fn draw(
    c: &mut Canvas,
    (x, y, w, h): (i32, i32, i32, i32),
    (t0, t1, now): (i64, i64, i64),
    points: &[(i64, f64)],
    forecast: Option<Forecast>,
    th: &Theme,
    scale: f32,
) {
    let span = (t1 - t0).max(1) as f32;
    let fx = |t: i64| x as f32 + (t - t0) as f32 / span * w as f32;
    let fy = |v: f64| y as f32 + (1.0 - (v.clamp(0.0, 100.0) / 100.0) as f32) * h as f32;
    let dash = (4.0 * scale).max(2.0) as i32;
    let now_x = fx(now).round() as i32;
    for yy in y..y + h {
        if (yy - y) / dash % 2 == 0 {
            c.vspan(now_x, yy as f32, yy as f32 + 1.0, th.dim, 0.6);
        }
    }
    let mut pts: Vec<(f32, f32)> = points.iter().filter(|p| p.0 >= t0 && p.0 <= t1).map(|p| (fx(p.0), fy(p.1))).collect();
    let Some(&(last_x, last_y)) = pts.last() else {
        return;
    };
    let solid = pts.len();
    if let Some((v, full)) = forecast {
        // Past 100 % the forecast stays flat, the same line `hover_point` reads.
        if let Some(full) = full.filter(|&f| f < t1) {
            pts.push((fx(full), fy(100.0)));
        }
        pts.push((fx(t1), fy(v.min(100.0))));
    }
    let stroke = 1.6 * scale;
    let first_x = pts[0].0;
    let end_x = pts[pts.len() - 1].0;
    let curve = |cx: f32| -> f32 {
        if cx <= pts[0].0 {
            return pts[0].1;
        }
        for pair in pts.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if cx <= b.0 {
                let f = if b.0 > a.0 { (cx - a.0) / (b.0 - a.0) } else { 1.0 };
                return a.1 + (b.1 - a.1) * f;
            }
        }
        pts[pts.len() - 1].1
    };
    let single = solid == 1 && forecast.is_none();
    for col in first_x.floor() as i32..=end_x.ceil().max(first_x.floor() + if single { 1.0 } else { 0.0 }) as i32 {
        if col < x || col >= x + w {
            continue;
        }
        let in_forecast = col as f32 > last_x;
        if in_forecast && (col - last_x as i32) / dash % 2 == 1 {
            continue;
        }
        let (l, r) = ((col as f32).max(first_x), (col as f32 + 1.0).min(end_x.max(first_x + 1.0)));
        let (ya, yb) = if single { (last_y, last_y) } else { (curve(l), curve(r)) };
        let alpha = if in_forecast { 0.55 } else { 1.0 };
        c.vspan(col, ya.min(yb) - stroke / 2.0, ya.max(yb) + stroke / 2.0, ORANGE, alpha);
    }
}

/// The reading under the cursor at time `at`: the nearest sample, or past the last one the dashed forecast
/// line's value (flagged `true`).
pub fn hover_point(points: &[(i64, f64)], t0: i64, t1: i64, forecast: Option<Forecast>, at: i64) -> Option<(i64, f64, bool)> {
    let inside: Vec<(i64, f64)> = points.iter().copied().filter(|p| p.0 >= t0 && p.0 <= t1).collect();
    let last = *inside.last()?;
    if let Some((end, _)) = forecast.filter(|_| at > last.0 && t1 > last.0) {
        let f = (at - last.0) as f64 / (t1 - last.0) as f64;
        return Some((at, (last.1 + (end - last.1) * f).min(100.0), true));
    }
    inside.into_iter().min_by_key(|p| (p.0 - at).abs()).map(|(t, v)| (t, v, false))
}

/// Usage at the reset if the current pace holds, from the samples inside the window.
pub fn forecast(points: &[(i64, f64)], t0: i64, t1: i64) -> Option<Forecast> {
    let inside: Vec<&(i64, f64)> = points.iter().filter(|p| p.0 >= t0 && p.0 <= t1).collect();
    let (first, last) = (inside.first()?, inside.last()?);
    if last.0 - first.0 < 15 * 60 || last.1 < first.1 {
        return None;
    }
    let rate = (last.1 - first.1) / (last.0 - first.0) as f64;
    let at_reset = last.1 + rate * (t1 - last.0) as f64;
    let full = (at_reset >= 100.0 && rate > 0.0).then(|| last.0 + ((100.0 - last.1) / rate) as i64);
    Some((at_reset, full))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forecast_extrapolates_the_pace() {
        let points = [(0, 0.0), (3600, 10.0)];
        assert_eq!(forecast(&points, 0, 7200), Some((20.0, None)));
        let (at_reset, full) = forecast(&points, 0, 50 * 3600).unwrap();
        assert!(at_reset > 100.0);
        assert_eq!(full, Some(10 * 3600));
    }

    #[test]
    fn forecast_needs_a_rising_quarter_hour() {
        assert_eq!(forecast(&[(0, 0.0), (600, 5.0)], 0, 7200), None);
        assert_eq!(forecast(&[(0, 10.0), (3600, 5.0)], 0, 7200), None);
    }

    #[test]
    fn hover_reads_samples_then_forecast() {
        let points = [(0, 0.0), (100, 10.0), (200, 20.0)];
        assert_eq!(hover_point(&points, 0, 400, None, 90), Some((100, 10.0, false)));
        assert_eq!(hover_point(&points, 0, 400, Some((40.0, None)), 300), Some((300, 30.0, true)));
    }
}
