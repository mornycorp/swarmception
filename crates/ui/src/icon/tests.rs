use super::*;

fn at(icon: &[u8], size: u32, x: f64, y: f64) -> [u8; 4] {
    let (px, py) = ((x * f64::from(size)) as usize, (y * f64::from(size)) as usize);
    let i = (py * size as usize + px) * 4;
    [icon[i], icon[i + 1], icon[i + 2], icon[i + 3]]
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.r, c.g, c.b]
}

/// The icon is the size asked (clamped), transparent outside its rounded tile, and drawn in the
/// palette's colours: its bars the series', its playhead `link`.
#[test]
fn the_icon_is_drawn_from_the_palette() {
    let icon = rgba(WINDOW_SIZE);
    assert_eq!(icon.len(), (WINDOW_SIZE * WINDOW_SIZE * 4) as usize);
    assert_eq!(at(&icon, WINDOW_SIZE, 0.005, 0.005)[3], 0, "a corner is outside the tile");
    let series = palette::SERIES.of(Look::Dark);
    for (cy, bars) in ROWS {
        for (start, end, entry) in bars {
            let middle = at(&icon, WINDOW_SIZE, (start + end) / 2.0, cy);
            assert_eq!((middle[..3].try_into().unwrap(), middle[3]), (rgb(series.at(entry)), 255), "row {cy}, bar {start}..{end}");
        }
    }
    let playhead = at(&icon, WINDOW_SIZE, PLAYHEAD_X, 0.2);
    assert_eq!(&playhead[..3], &rgb(palette::LINK.of(Look::Dark)));
    let ground = at(&icon, WINDOW_SIZE, 0.5, 0.06);
    assert_eq!(ground[3], 255, "the tile is opaque");
    assert_eq!(rgba(0).len(), 4, "clamped to 1");
    assert_eq!(rgba(MAX_SIZE + 1).len(), (MAX_SIZE * MAX_SIZE * 4) as usize, "clamped to the largest");
}

/// The same bytes every time, so a checked-in copy stays checkable.
#[test]
fn the_icon_is_the_same_every_time() {
    assert_eq!(rgba(64), rgba(64));
    let data = icon_data();
    assert_eq!((data.width, data.height, data.rgba.len()), (WINDOW_SIZE, WINDOW_SIZE, (WINDOW_SIZE * WINDOW_SIZE * 4) as usize));
}
