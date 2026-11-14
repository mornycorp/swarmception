//! The app icon (#178, swarm-core's App UI Substrate › The window): Swarmception's own, drawn here
//! in code from the theme's palette, with no third-party artwork, the way the skeleton draws its
//! generic one. A dark rounded tile holds a little timeline: three rows of tickets' bars, each a
//! capsule in a colour of the theme's series, and a playhead across them in the `link` blue.
//!
//! The window uses [`icon_data`] at run time. A favicon or a Windows `.ico`, if the app ever wants
//! one, is generated from [`rgba`] and checked in, with a test that it still matches.

use std::sync::Arc;

use swarmception_domain::theme::{self as palette, Look, Rgb};

/// The window icon's size; the system scales it for the title bar and the task bar.
pub const WINDOW_SIZE: u32 = 256;
/// The largest icon drawn; a bigger request is clamped.
pub const MAX_SIZE: u32 = 1024;

/// Geometry in unit coordinates (0..1, y down).
const TILE_INSET: f64 = 0.03;
const TILE_RADIUS: f64 = 0.2;
/// A ticket's bar: where it starts and ends, and the series entry it is drawn in.
type Bar = (f64, f64, u64);
/// Each row's centre line, and its bars.
const ROWS: [(f64, [Bar; 2]); 3] =
    [(0.30, [(0.16, 0.50, 0), (0.58, 0.84, 3)]), (0.50, [(0.26, 0.62, 1), (0.70, 0.84, 4)]), (0.70, [(0.16, 0.34, 2), (0.42, 0.76, 5)])];
const BAR_HEIGHT: f64 = 0.12;
/// The playhead: a thin line across the rows.
const PLAYHEAD_X: f64 = 0.55;
const PLAYHEAD_HALF_WIDTH: f64 = 0.012;
const PLAYHEAD_TOP: f64 = 0.17;
const PLAYHEAD_BOTTOM: f64 = 0.83;

/// A colour of the dark look, as 0..1 channels.
fn channels(rgb: Rgb) -> [f64; 3] {
    [rgb.r, rgb.g, rgb.b].map(|c| f64::from(c) / 255.0)
}

/// The colour of one point, or `None` outside the tile.
fn shade(x: f64, y: f64) -> Option<[f64; 3]> {
    if !in_rounded_square(x, y) {
        return None;
    }
    if (x - PLAYHEAD_X).abs() <= PLAYHEAD_HALF_WIDTH && (PLAYHEAD_TOP..=PLAYHEAD_BOTTOM).contains(&y) {
        return Some(channels(palette::LINK.of(Look::Dark)));
    }
    if let Some(rgb) = bar(x, y) {
        return Some(channels(rgb));
    }
    let (top, bottom) = (channels(palette::HEADER.of(Look::Dark)), channels(palette::BASE.of(Look::Dark)));
    let t = ((y - TILE_INSET) / (1.0 - 2.0 * TILE_INSET)).clamp(0.0, 1.0);
    Some([0, 1, 2].map(|i| top[i] + (bottom[i] - top[i]) * t))
}

fn in_rounded_square(x: f64, y: f64) -> bool {
    let half = 0.5 - TILE_INSET;
    let qx = (x - 0.5).abs() - (half - TILE_RADIUS);
    let qy = (y - 0.5).abs() - (half - TILE_RADIUS);
    // `sqrt`, not `hypot`: IEEE rounds it exactly everywhere, so every platform draws the same bytes.
    let (ox, oy) = (qx.max(0.0), qy.max(0.0));
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) <= TILE_RADIUS
}

/// The colour of the bar under a point, if any: a capsule in its series colour.
fn bar(x: f64, y: f64) -> Option<Rgb> {
    let r = BAR_HEIGHT / 2.0;
    ROWS.iter().find_map(|&(cy, bars)| {
        let dy = y - cy;
        bars.iter().find_map(|&(start, end, entry)| {
            let dx = x - x.max(start + r).min(end - r);
            (dx * dx + dy * dy <= r * r).then(|| palette::SERIES.of(Look::Dark).at(entry))
        })
    })
}

/// The icon as straight-alpha RGBA8, `size` × `size` (clamped to 1..=[`MAX_SIZE`]), each pixel
/// supersampled so the edges are smooth.
#[must_use]
pub fn rgba(size: u32) -> Vec<u8> {
    let size = size.clamp(1, MAX_SIZE);
    let samples: u32 = if size <= 64 { 8 } else { 4 };
    let scale = f64::from(size);
    let side = usize::try_from(size).unwrap_or(0);
    let mut out = Vec::with_capacity(side.saturating_mul(side).saturating_mul(4));
    for py in 0..size {
        for px in 0..size {
            out.extend(pixel(px, py, samples, scale));
        }
    }
    out
}

/// Pixel (`px`, `py`) of an icon `scale` pixels wide, from `samples` × `samples` samples: the mean
/// colour of the samples inside the shape, and its coverage as alpha.
fn pixel(px: u32, py: u32, samples: u32, scale: f64) -> [u8; 4] {
    let n = f64::from(samples * samples);
    let (mut acc, mut alpha) = ([0.0f64; 3], 0.0f64);
    for sy in 0..samples {
        for sx in 0..samples {
            let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(samples)) / scale;
            let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(samples)) / scale;
            if let Some(c) = shade(x, y) {
                for (a, v) in acc.iter_mut().zip(c) {
                    *a += v;
                }
                alpha += 1.0;
            }
        }
    }
    // In 0..=255 once clamped: the cast neither wraps nor truncates a whole part.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=255 first")]
    let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let [r, g, b] = if alpha > 0.0 { acc.map(|v| v / alpha) } else { [0.0; 3] }.map(byte);
    [r, g, b, byte(alpha / n)]
}

/// The window icon, for `ViewportBuilder::with_icon` (the title bar and the task bar).
#[must_use]
pub fn icon_data() -> Arc<egui::IconData> {
    Arc::new(egui::IconData { rgba: rgba(WINDOW_SIZE), width: WINDOW_SIZE, height: WINDOW_SIZE })
}

#[cfg(test)]
mod tests;
