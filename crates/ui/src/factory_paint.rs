//! Drawing the factory's floor from a [`FactoryScene`], with egui's painter: five stations left to
//! right (up next and the shelf, the benches, the dock, the gate, main), and the key along the
//! bottom in a row, two when the pane is narrow (#31). Every ticket is the same chip at every
//! station ([`crate::factory_chip`]): its fill its own colour, its border its phase, only its size
//! changing. The benches are short strips (the name, model and helper on one line, the status
//! under it), in more columns when many workers would not fit in one; the gate is its runs' list
//! (✔/✖, cause, age) under what it is gating. Every chip, bench, run and brick has its tooltip,
//! the scene's own lines in colour ([`crate::factory_tip`]). [`Drawn`] records what each station
//! drew, for a test to check against `factory.at`.
//!
//! Hovering a ticket rings it at every station it is drawn at (and the shell selects its bars on
//! the timeline); clicking it traces its journey through the stations, with the time at each.
//! Chips glide and resize between stations; nothing else moves by the wall clock: the gears and
//! the glow of what just happened follow *t*, so a still *t* draws a still factory.

use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};
use swarmception_domain::factory::{Bench, BenchStatus, Block, Crate, FactoryScene, GateRun, Outcome, Shift, Station, short};
use swarmception_domain::timeline::Link;

use crate::factory_chip::{CLAIMED, Chip, GATING, Look, Motion, PASSED, SENT_BACK, STOPPED, Size, WAITING, WORKING, ink, look, paint_chip, rgb};
use crate::{factory_tip, icons};

/// What each station drew: the crates by issue, the benches by key, the runs by sub.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawn {
    pub upcoming: Vec<u64>,
    pub shelf: Vec<u64>,
    /// Each bench's key and the crates it drew.
    pub benches: Vec<(String, Vec<u64>)>,
    pub dock: Vec<u64>,
    pub gate: Vec<u64>,
    /// The runs going on, then the runs before *t*.
    pub runs: Vec<String>,
    pub blocks: usize,
    pub done: Vec<u64>,
    /// How many crates a station had no room for, all told.
    pub hidden: usize,
    /// Every ticket chip drawn (bricks too), and where.
    pub chips: Vec<(u64, Rect)>,
    /// The tickets drawn ringed because the ticket hovered last frame is theirs.
    pub highlighted: Vec<u64>,
    /// The ticket under the pointer this frame.
    pub hovered: Option<u64>,
    /// A ticket clicked this frame; `Some(None)` for a click on the floor away from any.
    pub clicked: Option<Option<u64>>,
    /// A page chosen from a chip's context menu this frame: its issue, or its PR (#17).
    pub open: Option<Link>,
    /// The journey traced, its legs' labels in order.
    pub journey: Vec<String>,
    /// What each brick in main's wall says, oldest first: the bricks in view.
    pub marks: Vec<String>,
    /// Each brick in view and where it belongs in the wall (where it lands once flown in).
    pub bricks: Vec<(String, Rect)>,
    /// The bricks scrolled out of view.
    pub wall_hidden: usize,
}

/// What the floor is asked to show besides the scene: the ticket hovered last frame, the
/// ticket whose journey is traced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub hovered: Option<u64>,
    pub traced: Option<u64>,
}

/// How main's wall is scrolled (#32): the points down from its newest row, 0 when it fits.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WallView {
    pub scroll: f32,
}

/// The gap between the wall's bricks, and the width of its scrollbar.
const BRICK_GAP: f32 = 4.0;
const SCROLLBAR: f32 = 7.0;
/// The wall's brick sizes, the largest whose rows fit taken: the width of a ticket that took the
/// median claim → merge time, and the height.
const BRICK_SIZES: [(f32, f32); 3] = [(46.0, 26.0), (40.0, 23.0), (34.0, 20.0)];

/// Where a brick goes in the wall: its row (0 on the ground), its left from the wall's, its width.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    row: usize,
    x: f32,
    width: f32,
}

/// The wall laid out: each brick's place, how many rows, and the bricks' height.
struct Wall {
    places: Vec<Place>,
    rows: usize,
    height: f32,
}

/// The size of a brick's number, for a brick `height` tall.
fn brick_font(height: f32) -> f32 {
    (height * 0.45).clamp(8.0, 12.0)
}

/// A brick's width for a ticket that took `took` seconds when the median took `median`: in
/// proportion, `base` at the median, no narrower than `least` (what its number needs; a merge no
/// ticket names is that narrow), no wider than `most`, so one long ticket can't take the row.
fn brick_width(took: Option<i64>, median: Option<i64>, (least, base, most): (f32, f32, f32)) -> f32 {
    match (took, median) {
        (Some(took), Some(median)) if median > 0 => (base * took.max(0) as f32 / median as f32).clamp(least, most.max(least)),
        _ => least,
    }
}

/// `blocks` laid in rows `room.x` wide from the ground up, the oldest first, at the largest of
/// [`BRICK_SIZES`] whose rows fit in `room.y`; else at the smallest, to scroll. `labels`: each
/// brick's number's width at 10 points, so no brick is too narrow to say it.
fn wall_layout(blocks: &[Block], labels: &[f32], room: Vec2) -> Wall {
    let mut took: Vec<i64> = blocks.iter().filter_map(|b| b.took).collect();
    took.sort_unstable();
    let median = took.get(took.len() / 2).copied();
    let mut wall = Wall { places: Vec::new(), rows: 0, height: 20.0 };
    for (base, height) in BRICK_SIZES {
        let most = (base * 4.0).min(room.x * 0.6);
        let widths: Vec<f32> = blocks
            .iter()
            .enumerate()
            .map(|(n, b)| {
                let least = (labels.get(n).copied().unwrap_or(0.0) * brick_font(height) / 10.0 + 8.0).max(base * 0.6);
                brick_width(b.took, median, (least, base, most)).min(room.x.max(1.0))
            })
            .collect();
        let places = pack(&widths, room.x);
        let rows = places.last().map_or(0, |p| p.row + 1);
        wall = Wall { places, rows, height };
        if rows as f32 * (height + BRICK_GAP) - BRICK_GAP <= room.y {
            break;
        }
    }
    wall
}

/// A sent-back brick's mark at `rect`'s top right corner: a red corner, larger with the count in
/// it from two; nothing for a ticket never sent back.
fn sent_back_corner(painter: &egui::Painter, rect: Rect, times: usize) {
    if times == 0 {
        return;
    }
    let corner = if times > 1 { (rect.height() * 0.72).clamp(11.0, 17.0) } else { (rect.height() * 0.55).clamp(8.0, 13.0) };
    let tr = rect.right_top();
    let triangle = vec![tr, tr + vec2(0.0, corner), tr - vec2(corner, 0.0)];
    painter.add(egui::Shape::convex_polygon(triangle, SENT_BACK, Stroke::new(1.0, SENT_BACK.lerp_to_gamma(Color32::BLACK, 0.3))));
    if times > 1 {
        let at = tr + vec2(-corner * 0.31, corner * 0.33);
        painter.text(at, Align2::CENTER_CENTER, times.to_string(), FontId::proportional(corner * 0.62), Color32::BLACK);
    }
}

/// Bricks of `widths` packed into rows `width` wide, left to right, a new row when the next
/// brick doesn't fit.
fn pack(widths: &[f32], width: f32) -> Vec<Place> {
    let (mut row, mut x) = (0, 0.0);
    widths
        .iter()
        .map(|w| {
            if x > 0.0 && x + w > width {
                row += 1;
                x = 0.0;
            }
            let place = Place { row, x, width: *w };
            x += w + BRICK_GAP;
            place
        })
        .collect()
}

/// An idle bench: a quiet sea green (hue 150°, saturation 0.34, value 0.76), softer than main's.
const IDLE: Color32 = Color32::from_rgb(0x80, 0xc2, 0xa1);

/// A row of the key along the bottom.
const LEGEND_ROW: f32 = 17.0;
/// A run in the gate's list.
const RUN_LINE: f32 = 17.0;
/// The shortest bench that takes its two lines; shorter, it says it all on one.
const BENCH_TWO_LINES: f32 = 38.0;
/// The bench strips' heights: the least before taking another column, and the most.
const BENCH_LEAST: f32 = 42.0;
const BENCH_MOST: f32 = 56.0;
/// The narrowest a column of benches gets.
const BENCH_NARROWEST: f32 = 230.0;

/// A piece of the key, placed: its row, where it starts and how wide it may be, its swatch (none
/// for the key's title) and its words.
struct KeyItem {
    row: usize,
    x: f32,
    width: f32,
    swatch: Option<([u8; 3], Look)>,
    label: &'static str,
}

/// How many columns `n` benches take in `room`, and each strip's height: one column while every
/// strip gets [`BENCH_LEAST`], more (each at least [`BENCH_NARROWEST`] wide, three at most) when
/// not; the strips no taller than [`BENCH_MOST`].
fn bench_grid(n: usize, room: Vec2) -> (usize, f32) {
    let most = ((room.x / BENCH_NARROWEST).floor() as usize).clamp(1, 3);
    let cols = (1..=most).find(|c| n.div_ceil(*c) as f32 * BENCH_LEAST <= room.y).unwrap_or(most);
    let rows = n.div_ceil(cols).max(1);
    (cols, (room.y / rows as f32).min(BENCH_MOST))
}

/// The chips a bench strip `cell` draws, the one worked on first: small on a one-line strip,
/// medium then small on a narrow one, else large then medium.
fn bench_sizes(cell: Rect, compact: bool) -> &'static [Size] {
    if compact {
        &[Size::Small, Size::Small]
    } else if cell.width() < 330.0 {
        &[Size::Medium, Size::Small]
    } else {
        &[Size::Large, Size::Medium, Size::Medium]
    }
}

/// A line of `parts`, each in its colour, cut to `width` with an ellipsis.
fn pieces(painter: &egui::Painter, pos: Pos2, anchor: Align2, parts: &[(String, Color32)], font: FontId, width: f32) -> Rect {
    let mut job = egui::text::LayoutJob::default();
    for (text, colour) in parts {
        job.append(text, 0.0, egui::TextFormat { font_id: font.clone(), color: *colour, ..egui::TextFormat::default() });
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, Color32::PLACEHOLDER);
    rect
}

const fn bench_colour(status: BenchStatus) -> Color32 {
    match status {
        BenchStatus::Working => WORKING,
        BenchStatus::Claimed => CLAIMED,
        BenchStatus::SentBack => SENT_BACK,
        BenchStatus::Stopped | BenchStatus::Off => STOPPED,
        BenchStatus::Idle => IDLE,
        BenchStatus::Paused => CLAIMED,
    }
}

const fn outcome_colour(outcome: Outcome) -> Color32 {
    match outcome {
        Outcome::Running => WORKING,
        Outcome::Passed => PASSED,
        Outcome::Failed => SENT_BACK,
        Outcome::Unknown => STOPPED,
    }
}

/// The theme's colours for the floor.
struct Palette {
    text: Color32,
    weak: Color32,
    card: Color32,
    edge: Color32,
    cell: Color32,
}

impl Palette {
    fn of(visuals: &egui::Visuals) -> Self {
        Self {
            text: visuals.text_color(),
            weak: visuals.weak_text_color(),
            card: visuals.faint_bg_color,
            edge: visuals.widgets.noninteractive.bg_stroke.color,
            cell: visuals.extreme_bg_color,
        }
    }
}

/// Draws `scene` in `rect`, with `marks`: what each station drew.
pub fn paint(ui: &egui::Ui, rect: Rect, scene: &FactoryScene, state: (&mut Motion, &mut WallView), marks: Marks) -> Drawn {
    let (motion, wall) = state;
    motion.begin(ui.input(|i| i.time), rect);
    let floor_click = ui.interact(rect, ui.scope_id().with("factory-floor"), egui::Sense::click()).clicked();
    let mut floor = Floor { ui, painter: ui.painter_at(rect), pal: Palette::of(ui.visuals()), motion, drawn: Drawn::default(), scene, marks, wall };
    let key = floor.legend_layout(rect.width() - 8.0);
    let rows = key.iter().map(|item| item.row).max().map_or(1, |r| r + 1);
    let (stations, under) = rect.split_top_bottom_at_y(rect.max.y - LEGEND_ROW * rows as f32 - 6.0);
    let columns = columns(stations.shrink2(vec2(2.0, 2.0)));
    floor.intake(columns[0]);
    floor.benches(columns[1]);
    floor.dock(columns[2]);
    floor.gate(columns[3]);
    floor.shipping(columns[4]);
    for pair in columns.windows(2) {
        floor.arrow(pair[0].right(), pair[1].left(), pair[0].center().y);
    }
    floor.legend(under.shrink2(vec2(4.0, 3.0)), &key);
    floor.journey(&columns);
    let mut drawn = floor.drawn;
    if floor_click && drawn.clicked.is_none() {
        drawn.clicked = Some(None);
    }
    motion.end(ui.ctx());
    drawn
}

/// The stations' columns, with a gap for the arrows between.
fn columns(rect: Rect) -> [Rect; 5] {
    const SHARES: [f32; 5] = [0.10, 0.37, 0.12, 0.19, 0.22];
    const GAP: f32 = 14.0;
    let width = (rect.width() - GAP * 4.0).max(0.0);
    let mut x = rect.left();
    SHARES.map(|share| {
        let column = Rect::from_min_size(pos2(x, rect.top()), vec2(width * share, rect.height()));
        x += width * share + GAP;
        column
    })
}

/// A line of text cut to `width` with an ellipsis.
/// `style`: the font and the colour.
fn fit(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, style: (FontId, Color32), width: f32) -> Rect {
    let (font, colour) = style;
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, colour);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(1.0));
    let galley = painter.layout_job(job);
    let rect = anchor.anchor_size(pos, galley.size());
    painter.galley(rect.min, galley, colour);
    rect
}

/// A model's short name: `claude-opus-5-5` as `opus-5-5`.
fn model_name(model: &str) -> &str {
    model.strip_prefix("claude-").unwrap_or(model)
}

struct Floor<'a> {
    ui: &'a egui::Ui,
    painter: egui::Painter,
    pal: Palette,
    motion: &'a mut Motion,
    drawn: Drawn,
    scene: &'a FactoryScene,
    marks: Marks,
    wall: &'a mut WallView,
}

impl Floor<'_> {
    fn font(size: f32) -> FontId {
        FontId::proportional(size)
    }

    /// How fresh something `age` seconds old is: 1 just now, 0 at `recent` and older.
    fn fresh(&self, age: i64) -> f32 {
        let recent = self.scene.recent.max(1) as f32;
        (1.0 - age.max(0) as f32 / recent).clamp(0.0, 1.0)
    }

    fn tooltip(&self, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug, lines: &[String]) {
        let response = self.ui.interact(rect, self.ui.scope_id().with(("factory", id)), egui::Sense::hover());
        factory_tip::show(response, lines);
    }

    /// A station's card: its title and count; the room inside.
    fn card(&self, rect: Rect, title: &str, count: &str) -> Rect {
        self.painter.rect(rect, 6.0, self.pal.card, Stroke::new(1.0, self.pal.edge), StrokeKind::Inside);
        let top = rect.left_top() + vec2(8.0, 6.0);
        let title_rect = self.painter.text(top, Align2::LEFT_TOP, title, FontId::proportional(11.0), self.pal.weak);
        let width = rect.right() - title_rect.right() - 14.0;
        fit(&self.painter, pos2(rect.right() - 8.0, top.y), Align2::RIGHT_TOP, count, (FontId::proportional(11.0), self.pal.text), width);
        Rect::from_min_max(pos2(rect.left() + 6.0, rect.top() + 22.0), pos2(rect.right() - 6.0, rect.bottom() - 6.0))
    }

    /// "+N more", at the bottom of `room`.
    fn more(&mut self, room: Rect, hidden: usize) {
        if hidden > 0 {
            self.drawn.hidden += hidden;
            self.painter.text(room.center_bottom(), Align2::CENTER_BOTTOM, format!("+{hidden} more"), Self::font(10.5), self.pal.weak);
        }
    }

    /// The ring a ticket's chips get while it is hovered, at every station.
    fn ring(&self) -> Color32 {
        self.pal.text
    }

    /// Draws ticket `c` as its chip, gliding to `centre` and `size`: its fill its colour, its
    /// border its phase, a glow while its phase is fresh, a ring while it is hovered anywhere.
    /// Hovering or clicking it is noted in [`Drawn`].
    fn chip(&mut self, c: &Crate, centre: Pos2, size: Size) -> Rect {
        let rect = self.motion.place(c.issue, Rect::from_center_size(centre, size.dims()));
        let look = look(c.state);
        let glow = if look == Look::Hollow { 0.0 } else { self.fresh(c.age) };
        let highlight = self.marks.hovered.filter(|h| *h == c.issue).map(|_| self.ring());
        paint_chip(&self.painter, rect, &Chip { colour: c.colour, look, label: &c.label, ground: self.pal.card, highlight, glow });
        self.pointed(rect, c.issue, c.pr, &c.tooltip);
        rect
    }

    /// The tooltip of ticket `issue`'s chip at `rect`, whether the pointer is on it or clicks it,
    /// and its context menu: Open Issue, and Open PR when it has one (`pr`).
    fn pointed(&mut self, rect: Rect, issue: u64, pr: Option<u64>, tooltip: &[String]) {
        let id = self.ui.scope_id().with(("factory-chip", issue, self.drawn.chips.len()));
        let response = self.ui.interact(rect, id, egui::Sense::click());
        if response.hovered() {
            self.drawn.hovered = Some(issue);
        }
        if response.clicked() {
            self.drawn.clicked = Some(Some(issue));
        }
        let response = factory_tip::show(response, tooltip);
        let mut chosen = None;
        response.context_menu(|ui| {
            for link in [Some(Link::Issue(issue)), pr.filter(|pr| *pr != issue).map(Link::Pull)].into_iter().flatten() {
                if ui.button(link.label()).clicked() {
                    chosen = Some(link);
                    ui.close();
                }
            }
        });
        if chosen.is_some() {
            self.drawn.open = chosen;
        }
        self.drawn.chips.push((issue, rect));
        if self.marks.hovered == Some(issue) {
            self.drawn.highlighted.push(issue);
        }
    }

    /// The arrow between two stations.
    fn arrow(&self, from: f32, to: f32, y: f32) {
        let colour = self.pal.weak.gamma_multiply(0.8);
        let (a, b) = (pos2(from + 2.0, y), pos2(to - 2.0, y));
        self.painter.line_segment([a, b], Stroke::new(2.0, colour));
        let tip = vec2(5.0, 5.0);
        self.painter.add(egui::Shape::convex_polygon(vec![b + vec2(1.0, 0.0), b - vec2(tip.x, tip.y), b - vec2(tip.x, -tip.y)], colour, Stroke::NONE));
    }

    /// Up next, over the shelf.
    fn intake(&mut self, column: Rect) {
        let scene = self.scene;
        let shelf_share = if scene.shelf.is_empty() { 0.25 } else { 0.42 };
        let (top, bottom) = column.split_top_bottom_at_fraction(1.0 - shelf_share);
        let room = self.card(top.shrink2(vec2(0.0, 0.0)).with_max_y(top.bottom() - 3.0), "UP NEXT", &scene.counts.upcoming.to_string());
        let rows = ((room.height() - 12.0) / 27.0).floor().max(0.0) as usize;
        for (n, c) in scene.upcoming.iter().take(rows).enumerate() {
            let centre = pos2(room.left() + Size::Medium.dims().x / 2.0 + 4.0, room.top() + 12.0 + 27.0 * n as f32);
            let crate_rect = self.chip(c, centre, Size::Medium);
            let text_width = room.right() - crate_rect.right() - 4.0;
            fit(
                &self.painter,
                crate_rect.right_center() + vec2(5.0, 0.0),
                Align2::LEFT_CENTER,
                &format!("in {}", short(c.age)),
                (Self::font(10.5), self.pal.weak),
                text_width,
            );
            self.drawn.upcoming.push(c.issue);
        }
        let shown = self.drawn.upcoming.len();
        self.more(room, scene.counts.upcoming.saturating_sub(shown));
        let room = self.card(bottom.with_min_y(bottom.top() + 3.0), "SHELF", &scene.counts.shelved.to_string());
        let shelf: Vec<&Crate> = scene.shelf.iter().collect();
        let shown = self.grid(room, &shelf, Size::Small);
        self.drawn.shelf = shelf.iter().take(shown).map(|c| c.issue).collect();
        self.more(room, scene.counts.shelved.saturating_sub(shown));
    }

    /// Small crates in rows across `room`, as many as fit with a line left for "+N more": how many.
    fn grid(&mut self, room: Rect, crates: &[&Crate], size: Size) -> usize {
        let step = size.dims() + vec2(8.0, 8.0);
        let per_row = ((room.width() + 6.0) / step.x).floor().max(1.0) as usize;
        let rows = ((room.height() - 12.0 + 6.0) / step.y).floor().max(0.0) as usize;
        let room_for = per_row * rows;
        let shown = if crates.len() > room_for { room_for.saturating_sub(1) } else { crates.len() };
        for (n, c) in crates.iter().take(shown).enumerate() {
            let (col, row) = ((n % per_row) as f32, (n / per_row) as f32);
            let centre = room.left_top() + vec2(size.dims().x / 2.0 + 4.0 + col * step.x, size.dims().y / 2.0 + 4.0 + row * step.y);
            self.chip(c, centre, size);
        }
        shown
    }

    fn benches(&mut self, column: Rect) {
        let scene = self.scene;
        let working = scene.benches.iter().filter(|b| b.status == BenchStatus::Working).count();
        let room = self.card(column, "BENCHES", &format!("{working} working · {} crates", scene.counts.at_benches));
        let n = scene.benches.len();
        if n == 0 {
            self.painter.text(room.center(), Align2::CENTER_CENTER, "no workers yet", Self::font(11.0), self.pal.weak);
            return;
        }
        let (cols, height) = bench_grid(n, room.size());
        let rows = n.div_ceil(cols);
        let width = room.width() / cols as f32;
        for (i, bench) in scene.benches.iter().enumerate() {
            let (col, row) = ((i / rows) as f32, (i % rows) as f32);
            let cell = Rect::from_min_size(room.left_top() + vec2(col * width, row * height), vec2(width, height)).shrink2(vec2(2.5, 2.0));
            self.bench(cell, bench);
        }
    }

    /// A bench's strip: the worker, its name, model and helper on one line and its status under
    /// it (all on one line when the strip is too short for two), its tickets at the right.
    fn bench(&mut self, cell: Rect, bench: &Bench) {
        let status = bench_colour(bench.status);
        let lit = bench.status == BenchStatus::Working;
        let edge = if lit { Stroke::new(1.5, status) } else { Stroke::new(1.0, self.pal.edge) };
        self.painter.rect(cell, 5.0, self.pal.cell, edge, StrokeKind::Inside);
        let compact = cell.height() < BENCH_TWO_LINES;
        let radius = if compact { 8.0 } else { (cell.height() / 2.0 - 7.0).clamp(8.0, 13.0) };
        let avatar = pos2(cell.left() + 7.0 + radius, cell.center().y);
        self.actor(avatar, radius, bench);
        let sizes = bench_sizes(cell, compact);
        let shown = bench.crates.len().min(sizes.len());
        let crates_width: f32 = sizes.iter().take(shown).map(|s| s.dims().x + 8.0).sum::<f32>() + if shown > 0 { 4.0 } else { 0.0 };
        let text = Rect::from_min_max(pos2(avatar.x + radius + 8.0, cell.top()), pos2(cell.right() - crates_width - 4.0, cell.bottom()));
        let line = self.bench_text(text, bench, compact);
        self.helper(bench, line, text.right());
        self.tooltip(cell, ("bench", &bench.key), &bench.tooltip);
        self.bench_crates(cell, bench, sizes);
    }

    /// The worker's name, its model and effort beside it, and its status line under them (beside
    /// them when `compact`), in `room`: the first line's last piece, for the helper to follow.
    fn bench_text(&self, room: Rect, bench: &Bench, compact: bool) -> Rect {
        let width = room.width().max(10.0);
        let line = status_line(bench);
        let status = (Self::font(11.5), bench_colour(bench.status));
        let name_font = FontId::proportional(13.0);
        if compact {
            let name = fit(&self.painter, room.left_center(), Align2::LEFT_CENTER, &bench.actor.name, (name_font, self.pal.text), width * 0.45);
            let at = name.right_center() + vec2(8.0, 0.0);
            return fit(&self.painter, at, Align2::LEFT_CENTER, &line, status, room.right() - at.x);
        }
        let first = pos2(room.left(), room.center().y - 1.0);
        let name = fit(&self.painter, first, Align2::LEFT_BOTTOM, &bench.actor.name, (name_font, self.pal.text), width);
        let mut last = name;
        if let Some(model) = bench.actor.model.as_deref() {
            let text = bench.actor.effort.as_ref().map_or_else(|| model_name(model).to_owned(), |e| format!("{} · {e}", model_name(model)));
            let at = name.right_bottom() + vec2(7.0, -1.5);
            if room.right() - at.x > 24.0 {
                last = fit(&self.painter, at, Align2::LEFT_BOTTOM, &text, (Self::font(10.0), self.pal.weak), room.right() - at.x);
            }
        }
        fit(&self.painter, pos2(room.left(), room.center().y + 2.0), Align2::LEFT_TOP, &line, status, width);
        last
    }

    /// The tickets the bench holds, from its right in `sizes`: the one it works on first, with
    /// its progress under it unless it is small.
    fn bench_crates(&mut self, cell: Rect, bench: &Bench, sizes: &[Size]) {
        let mut x = cell.right() - 8.0;
        let mut drawn = Vec::new();
        for (c, size) in bench.crates.iter().zip(sizes) {
            let size = *size;
            let barred = drawn.is_empty() && size != Size::Small;
            let y = cell.center().y - if barred { 3.0 } else { 0.0 };
            let rect = Rect::from_center_size(pos2(x - size.dims().x / 2.0, y), size.dims());
            let drawn_at = self.chip(c, rect.center(), size);
            if barred {
                self.progress(drawn_at, c.age, look(c.state));
            }
            drawn.push(c.issue);
            x = rect.left() - 8.0;
        }
        self.drawn.hidden += bench.crates.len().saturating_sub(drawn.len());
        self.drawn.benches.push((bench.key.clone(), drawn));
    }

    /// The worker: a disc in its shift's colour with its initial, a lamp for its status, and
    /// gears turning with *t* while it works.
    fn actor(&self, at: Pos2, radius: f32, bench: &Bench) {
        let status = bench_colour(bench.status);
        let disc = match bench.actor.shift {
            Shift::On => status.lerp_to_gamma(self.pal.cell, 0.25),
            Shift::Paused => CLAIMED.lerp_to_gamma(self.pal.cell, 0.4),
            Shift::Off => STOPPED.lerp_to_gamma(self.pal.cell, 0.6),
        };
        if bench.status == BenchStatus::Working {
            let turn = (self.scene.t.0.rem_euclid(3_600) as f32) / 60.0;
            for tooth in 0..8 {
                let angle = turn + tooth as f32 * std::f32::consts::TAU / 8.0;
                let p = at + vec2(angle.cos(), angle.sin()) * (radius + 2.5);
                self.painter.circle_filled(p, 2.2, WORKING);
            }
        }
        self.painter.circle(at, radius, disc, Stroke::new(1.0, status));
        let initial: String = bench.actor.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
        self.painter.text(at, Align2::CENTER_CENTER, initial, FontId::proportional(radius * 1.1), ink(disc));
        let lamp = at + vec2(radius * 0.72, -radius * 0.72);
        self.painter.circle(lamp, 3.5, status, Stroke::new(1.0, self.pal.cell));
    }

    /// The helper's lamp, beside the worker's name: a check running, or just passed or failed.
    fn helper(&self, bench: &Bench, name: Rect, right: f32) {
        let Some(helper) = &bench.helper else { return };
        let colour = outcome_colour(helper.outcome);
        let mark = match helper.outcome {
            Outcome::Running => icons::FLASK,
            Outcome::Passed => icons::CHECK,
            Outcome::Failed => icons::CROSS,
            Outcome::Unknown => "?",
        };
        let text = format!("{mark} {} {}", helper.name, short(helper.age));
        let at = pos2((name.right() + 8.0).min(right - 40.0), name.center().y);
        let rect = fit(&self.painter, at, Align2::LEFT_CENTER, &text, (Self::font(10.0), colour), (right - at.x).max(10.0));
        self.tooltip(rect, ("helper", &bench.key), &helper.tooltip);
    }

    /// How long a ticket has been in its phase at the bench: a bar under its chip in its phase's
    /// colour, full at an hour.
    fn progress(&self, rect: Rect, age: i64, look: Look) {
        let Look::Phase(colour) = look else { return };
        let track = Rect::from_min_size(rect.left_bottom() + vec2(0.0, 5.0), vec2(rect.width(), 3.0));
        self.painter.rect_filled(track, 1.5, self.pal.edge);
        let done = (age.max(0) as f32 / 3_600.0).min(1.0);
        self.painter.rect_filled(track.with_max_x(track.left() + track.width() * done), 1.5, colour);
    }

    fn dock(&mut self, column: Rect) {
        let scene = self.scene;
        let room = self.card(column, "DOCK", &format!("{} waiting", scene.counts.waiting));
        if scene.dock.is_empty() {
            self.painter.text(room.center(), Align2::CENTER_CENTER, "empty", Self::font(11.0), self.pal.weak);
            return;
        }
        let fits = ((room.height() - 12.0) / 27.0).floor().max(0.0) as usize;
        let shown = if scene.dock.len() > fits { fits.saturating_sub(1) } else { scene.dock.len() };
        for (n, c) in scene.dock.iter().take(shown).enumerate() {
            let centre = pos2(room.left() + Size::Medium.dims().x / 2.0 + 4.0, room.top() + 12.0 + 27.0 * n as f32);
            let rect = self.chip(c, centre, Size::Medium);
            let who = c.worker.as_deref().unwrap_or("");
            let width = room.right() - rect.right() - 4.0;
            let text = rect.right_center() + vec2(5.0, 0.0);
            fit(&self.painter, text, Align2::LEFT_BOTTOM, &short(c.age), (Self::font(11.0), WAITING), width);
            fit(&self.painter, text, Align2::LEFT_TOP, who, (Self::font(9.5), self.pal.weak), width);
            self.drawn.dock.push(c.issue);
        }
        self.more(room, scene.counts.waiting.saturating_sub(shown));
    }

    /// The gate: its counts (passed green, failed red, no result grey when any), what it is doing,
    /// the tickets in it, and its runs, newest first (✔/✖/?, the run, its tickets and cause, its
    /// age) in all the room left: a run that never finished (#42) is grey, `never finished`.
    fn gate(&mut self, column: Rect) {
        let gate = &self.scene.gate;
        let room = self.card(column, "GATE", "");
        let counts = [
            (format!("{} {}", icons::CHECK, gate.passed), PASSED),
            ("   ".to_owned(), self.pal.text),
            (format!("{} {}", icons::CROSS, gate.failed), if gate.failed > 0 { SENT_BACK } else { self.pal.weak }),
            (if gate.no_result > 0 { format!("   ? {}", gate.no_result) } else { String::new() }, STOPPED),
        ];
        pieces(&self.painter, pos2(column.right() - 8.0, column.top() + 6.0), Align2::RIGHT_TOP, &counts, Self::font(11.0), room.width() * 0.6);
        let mut y = room.top();
        let status = match (gate.running.first(), gate.recent.first()) {
            (Some(run), _) => (format!("running {} · {}", run.sub, short(run.age)), WORKING),
            (None, Some(last)) => (format!("idle · last {} {} ago", outcome_word(last.outcome), short(last.age)), self.pal.weak),
            (None, None) => ("idle · no runs yet".to_owned(), self.pal.weak),
        };
        fit(&self.painter, pos2(room.left(), y), Align2::LEFT_TOP, &status.0, (Self::font(11.0), status.1), room.width());
        y += 17.0;
        if !gate.crates.is_empty() {
            let chip = Size::Medium.dims();
            let fits = ((room.width() + 8.0) / (chip.x + 8.0)).floor().max(1.0) as usize;
            let shown = gate.crates.len().min(fits);
            for (n, c) in gate.crates.iter().take(shown).enumerate() {
                let x = room.left() + 4.0 + chip.x / 2.0 + n as f32 * (chip.x + 8.0);
                self.chip(c, pos2(x, y + 4.0 + chip.y / 2.0), Size::Medium);
                self.drawn.gate.push(c.issue);
            }
            self.drawn.hidden += gate.crates.len() - shown;
            y += chip.y + 12.0;
        }
        let runs: Vec<&GateRun> = gate.running.iter().chain(&gate.recent).collect();
        let bottom = room.bottom() - if gate.inspectors.is_empty() { 0.0 } else { 15.0 };
        for (n, run) in runs.iter().enumerate() {
            if y + RUN_LINE > bottom {
                break;
            }
            self.run_line(Rect::from_min_size(pos2(room.left(), y), vec2(room.width(), RUN_LINE)), run, n);
            y += RUN_LINE;
        }
        let inspectors: Vec<String> = gate.inspectors.iter().map(|a| format!("{} · {}", a.name, shift_word(a.shift))).collect();
        if !inspectors.is_empty() {
            fit(&self.painter, room.left_bottom(), Align2::LEFT_BOTTOM, &inspectors.join("  "), (Self::font(10.0), self.pal.weak), room.width());
        }
    }

    /// A run in the gate's list: its mark in its outcome's colour, the run, the tickets it gated,
    /// its cause (red when it failed) and its age; a glow while it is fresh.
    fn run_line(&mut self, line: Rect, run: &GateRun, n: usize) {
        let colour = outcome_colour(run.outcome);
        let fresh = if run.outcome == Outcome::Running { 1.0 } else { self.fresh(run.age) };
        if fresh > 0.0 {
            self.painter.rect_filled(line, 3.0, colour.gamma_multiply(0.22 * fresh));
        }
        let mark = match run.outcome {
            Outcome::Running => icons::COG,
            Outcome::Passed => icons::CHECK,
            Outcome::Failed => icons::CROSS,
            Outcome::Unknown => "?",
        };
        let font = Self::font(11.0);
        self.painter.text(line.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER, mark, font.clone(), colour);
        let age = self.painter.text(line.right_center() - vec2(3.0, 0.0), Align2::RIGHT_CENTER, short(run.age), font.clone(), self.pal.weak);
        let mut what = vec![(run.sub.clone(), self.pal.text)];
        if !run.issues.is_empty() {
            let tickets: Vec<String> = run.issues.iter().map(|i| format!("#{i}")).collect();
            what.push((format!("  {}", tickets.join(" ")), self.pal.weak));
        }
        if let Some(cause) = &run.cause {
            let red = matches!(run.outcome, Outcome::Failed);
            what.push((format!("  {cause}"), if red { SENT_BACK } else { self.pal.weak }));
        }
        let width = age.left() - line.left() - 24.0;
        pieces(&self.painter, line.left_center() + vec2(19.0, 0.0), Align2::LEFT_CENTER, &what, font, width);
        self.tooltip(line, ("run", n, &run.sub), &run.tooltip);
        self.drawn.runs.push(run.sub.clone());
    }

    fn shipping(&mut self, column: Rect) {
        let ship = &self.scene.shipping;
        let room = self.card(column, "MAIN", &format!("{} of {} merges", ship.landed, ship.total));
        let done_height = if ship.done_count > 0 { 40.0 } else { 0.0 };
        let mut top = room.top();
        if let Some(last) = ship.blocks.last() {
            let text = format!("last: {} · {} ago", last.label, short(last.age));
            fit(&self.painter, pos2(room.left(), top), Align2::LEFT_TOP, &text, (Self::font(11.0), PASSED), room.width());
            top += 16.0;
        }
        let site = Rect::from_min_max(pos2(room.left(), top), pos2(room.right(), room.bottom() - done_height));
        self.building(site);
        if done_height > 0.0 {
            let strip = Rect::from_min_max(pos2(room.left(), room.bottom() - done_height + 4.0), room.right_bottom());
            let title = format!("done, no merge · {}", ship.done_count);
            self.painter.text(strip.left_top(), Align2::LEFT_TOP, title, Self::font(10.0), self.pal.weak);
            let room = strip.with_min_y(strip.top() + 13.0);
            let done: Vec<&Crate> = ship.done.iter().collect();
            let shown = self.grid(room.with_max_y(room.bottom() + 12.0), &done, Size::Small);
            self.drawn.done = done.iter().take(shown).map(|c| c.issue).collect();
            self.drawn.hidden += ship.done_count.saturating_sub(shown);
        }
    }

    /// The app on main (#32): a brick per merge, its width its ticket's claim → merge time, laid
    /// in rows from the ground up, the oldest first; scrolled with the wheel when the wall
    /// outgrows its room, the newest rows shown first.
    fn building(&mut self, site: Rect) {
        let blocks = &self.scene.shipping.blocks;
        let ground = Rect::from_min_max(pos2(site.left(), site.bottom() - 4.0), site.right_bottom());
        self.painter.rect_filled(ground, 1.0, PASSED.gamma_multiply(0.7));
        let area = site.with_max_y(ground.top() - 2.0).with_max_x(site.right() - SCROLLBAR);
        let labels: Vec<f32> =
            blocks.iter().map(|b| self.painter.layout_no_wrap(b.mark.clone(), FontId::proportional(10.0), Color32::WHITE).size().x).collect();
        let wall = wall_layout(blocks, &labels, area.size());
        let content = wall.rows as f32 * (wall.height + BRICK_GAP) - BRICK_GAP;
        let overflow = (content - area.height()).max(0.0);
        let hovered = self.ui.rect_contains_pointer(area);
        let mut scroll = self.wall.scroll;
        if hovered && overflow > 0.0 {
            scroll -= self.ui.input(|i| i.smooth_scroll_delta.y);
        }
        scroll = scroll.clamp(0.0, overflow);
        self.wall.scroll = scroll;
        let painter = self.painter.with_clip_rect(area.intersect(self.painter.clip_rect()));
        let mut shown = 0;
        for (block, place) in blocks.iter().zip(&wall.places) {
            let top = if overflow > 0.0 {
                area.top() + (wall.rows - 1 - place.row) as f32 * (wall.height + BRICK_GAP) - scroll
            } else {
                area.bottom() - (place.row + 1) as f32 * (wall.height + BRICK_GAP) + BRICK_GAP
            };
            let rect = Rect::from_min_size(pos2(area.left() + place.x, top), vec2(place.width, wall.height));
            if !area.intersects(rect) {
                continue;
            }
            self.brick(&painter, rect, block, area);
            shown += 1;
        }
        self.drawn.blocks = shown;
        self.drawn.wall_hidden = blocks.len() - shown;
        if overflow > 0.0 {
            let track = Rect::from_min_max(pos2(site.right() - SCROLLBAR + 3.0, area.top()), pos2(site.right(), area.bottom()));
            self.painter.rect_filled(track, 2.0, self.pal.edge.gamma_multiply(0.6));
            let thumb = (area.height() / content * track.height()).max(12.0);
            let at = track.top() + (track.height() - thumb) * (scroll / overflow);
            self.painter.rect_filled(Rect::from_min_size(pos2(track.left(), at), vec2(track.width(), thumb)), 2.0, self.pal.weak);
        }
    }

    /// A merge's brick in the wall (#32): its ticket's colour, its number, a strip along its foot
    /// with its life's phases in their colours (the time each took, to scale), a red corner when
    /// it was sent back (the count in it from two). Flown in from where the ticket's chip was; a
    /// merge no ticket names is a plain grey brick. Only the part inside `clip` takes the pointer.
    fn brick(&mut self, wall: &egui::Painter, place: Rect, block: &Block, clip: Rect) {
        self.drawn.bricks.push((block.mark.clone(), place));
        let rect = match block.issue {
            Some(issue) => self.motion.land(issue, place),
            None => place,
        };
        // In flight from the floor, it isn't held to the wall's room.
        let flying = rect != place;
        let floor = self.painter.clone();
        let (painter, clip) = if flying { (&floor, floor.clip_rect()) } else { (wall, clip) };
        let fresh = self.fresh(block.age);
        if fresh > 0.0 {
            painter.rect_filled(rect.expand(2.0 + 3.0 * fresh), 4.0, PASSED.gamma_multiply(0.5 * fresh));
        }
        self.brick_face(painter, rect, block);
        sent_back_corner(painter, rect, block.sent_back);
        if let Some(ring) = block.issue.filter(|i| self.marks.hovered == Some(*i)).map(|_| self.ring()) {
            painter.rect_stroke(rect.expand(2.5), 5.0, Stroke::new(2.0, ring), StrokeKind::Outside);
        }
        self.drawn.marks.push(block.mark.clone());
        let target = rect.intersect(clip);
        if target.is_positive() {
            match block.issue {
                Some(issue) => self.pointed(target, issue, block.pr, &block.tooltip),
                None => self.tooltip(target, ("block", block.t.0, block.pr), &block.tooltip),
            }
        }
    }

    /// A brick's body in its ticket's colour, its number, and its life's strip along its foot.
    fn brick_face(&self, painter: &egui::Painter, rect: Rect, block: &Block) {
        let fill = rgb(block.colour);
        painter.rect(rect, 3.0, fill, Stroke::new(1.0, fill.lerp_to_gamma(Color32::BLACK, 0.35)), StrokeKind::Inside);
        let strip_height = if block.segments.is_empty() { 0.0 } else { (rect.height() * 0.28).max(4.0) };
        let face = rect.with_max_y(rect.bottom() - strip_height);
        if strip_height > 0.0 {
            let strip = Rect::from_min_max(pos2(rect.left() + 2.0, rect.bottom() - strip_height - 1.0), pos2(rect.right() - 2.0, rect.bottom() - 2.0));
            self.phases(painter, strip, block);
        }
        let ink = ink(fill);
        let galley = painter.layout_no_wrap(block.mark.clone(), FontId::proportional(brick_font(rect.height())), ink);
        if galley.size().x <= face.width() - 4.0 {
            painter.galley(Align2::CENTER_CENTER.anchor_size(face.center() + vec2(0.0, 0.5), galley.size()).min, galley, ink);
        }
    }

    /// A brick's life along `strip`: each segment in its phase's colour, as long as its share of
    /// the life; a segment too short to see gets a sliver, the rest giving way.
    fn phases(&self, painter: &egui::Painter, strip: Rect, block: &Block) {
        painter.rect_filled(strip.expand(0.5), 1.5, Color32::BLACK.gamma_multiply(0.45));
        let total: i64 = block.segments.iter().map(|s| s.seconds.max(0)).sum();
        if total <= 0 {
            return;
        }
        let mut x = strip.left();
        for segment in &block.segments {
            let width = strip.width() * segment.seconds.max(0) as f32 / total as f32;
            let colour = match look(segment.state) {
                Look::Phase(colour) => colour,
                Look::Hollow | Look::Plain => self.pal.weak,
            };
            let piece = Rect::from_min_max(pos2(x, strip.top()), pos2((x + width.max(1.0)).min(strip.right()), strip.bottom()));
            painter.rect_filled(piece, 0.0, colour);
            x += width;
        }
    }

    /// The key's pieces placed along the bottom: in one row when they fit in `width`, else in two
    /// of five each.
    fn legend_layout(&self, width: f32) -> Vec<KeyItem> {
        let sample = [0x5c, 0x5f, 0x70];
        let phase = |colour: Color32, label: &'static str| (Some((sample, Look::Phase(colour))), label);
        let items = [
            (None, "border = phase · fill = ticket"),
            (Some((sample, Look::Hollow)), "up next"),
            phase(CLAIMED, "claimed"),
            phase(WORKING, "working"),
            phase(SENT_BACK, "sent back"),
            phase(STOPPED, "stopped / shelved"),
            phase(WAITING, "waiting for merge"),
            phase(GATING, "in the gate"),
            (Some((sample, Look::Plain)), "merged: a brick, as wide as it took"),
            (Some((swarmception_domain::colours::NO_TICKET, Look::Plain)), "grey !13: a merge, no ticket"),
        ];
        let measured: Vec<f32> = items
            .iter()
            .map(|(swatch, label)| {
                let text = self.painter.layout_no_wrap((*label).to_owned(), Self::font(11.0), self.pal.text).size().x;
                text + if swatch.is_some() { 26.0 } else { 0.0 } + 18.0
            })
            .collect();
        let one_row = measured.iter().sum::<f32>() <= width;
        let per_row = items.len().div_ceil(2);
        let mut x = 0.0;
        items
            .iter()
            .zip(measured)
            .enumerate()
            .map(|(n, ((swatch, label), measured))| {
                if one_row {
                    let item = KeyItem { row: 0, x, width: measured, swatch: *swatch, label };
                    x += measured;
                    item
                } else {
                    let share = width / per_row as f32;
                    KeyItem { row: n / per_row, x: (n % per_row) as f32 * share, width: share, swatch: *swatch, label }
                }
            })
            .collect()
    }

    /// What a chip's look means, along the bottom: its border the phase, its fill the ticket.
    fn legend(&self, rect: Rect, key: &[KeyItem]) {
        let ground = self.pal.card;
        for item in key {
            let at = rect.left_top() + vec2(item.x, item.row as f32 * LEGEND_ROW + LEGEND_ROW / 2.0);
            let Some((colour, look)) = item.swatch else {
                fit(&self.painter, at, Align2::LEFT_CENTER, item.label, (Self::font(11.0), self.pal.weak), item.width - 8.0);
                continue;
            };
            let swatch = Rect::from_center_size(at + vec2(11.0, 0.0), vec2(18.0, 10.0));
            paint_chip(&self.painter, swatch, &Chip { colour, look, label: "", ground, highlight: None, glow: 0.0 });
            fit(&self.painter, at + vec2(26.0, 0.0), Align2::LEFT_CENTER, item.label, (Self::font(11.0), self.pal.text), item.width - 34.0);
        }
    }

    /// The traced ticket's journey: a faint path through the stations it passed, a pill at each
    /// with the time spent there (`bench 42m`), ending at the ticket where it is now.
    fn journey(&mut self, columns: &[Rect; 5]) {
        let Some(journey) = self.scene.journey.as_ref().filter(|j| Some(j.issue) == self.marks.traced) else { return };
        let colour = self
            .scene
            .shipping
            .blocks
            .iter()
            .find(|b| b.issue == Some(journey.issue))
            .map_or_else(|| swarmception_domain::colours::ticket_colour(journey.issue), |b| b.colour);
        let line = rgb(colour);
        let mut stacked = [0usize; 5];
        let mut points: Vec<Pos2> = Vec::new();
        for leg in &journey.legs {
            let column = match leg.station {
                Station::Bench => 1,
                Station::Dock => 2,
                Station::Gate => 3,
                Station::Main | Station::Done => 4,
            };
            let Some(rect) = columns.get(column) else { continue };
            let level = stacked.get(column).copied().unwrap_or(0);
            if let Some(n) = stacked.get_mut(column) {
                *n += 1;
            }
            points.push(pos2(rect.center().x, rect.bottom() - 22.0 - 22.0 * level as f32));
        }
        // Where it is now: the stations draw before the ticker, so its first chip.
        if let Some(now) = self.drawn.chips.iter().find(|(issue, _)| *issue == journey.issue).map(|(_, r)| r.center()) {
            points.push(now);
        }
        self.painter.add(egui::Shape::line(points.clone(), Stroke::new(2.5, line.gamma_multiply(0.55))));
        for (leg, at) in journey.legs.iter().zip(&points) {
            let galley = self.painter.layout_no_wrap(leg.label.clone(), Self::font(11.0), self.pal.text);
            let pill = Rect::from_center_size(*at, galley.size() + vec2(14.0, 6.0));
            self.painter.rect(pill, 9.0, self.pal.cell.gamma_multiply(0.92), Stroke::new(1.5, line), StrokeKind::Inside);
            self.painter.galley(pill.center() - galley.size() / 2.0, galley, self.pal.text);
            self.drawn.journey.push(leg.label.clone());
        }
    }
}

const fn outcome_word(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Running => "running",
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Unknown => "no result",
    }
}

const fn shift_word(shift: Shift) -> &'static str {
    match shift {
        Shift::On => "on shift",
        Shift::Paused => "paused",
        Shift::Off => "off shift",
    }
}

/// A bench's status, as its line under the name says it.
fn status_line(bench: &Bench) -> String {
    let status = bench.status.name();
    match bench.crates.first() {
        Some(c) if matches!(bench.status, BenchStatus::Working | BenchStatus::Claimed | BenchStatus::SentBack | BenchStatus::Stopped) => {
            let note = c.note.as_ref().filter(|_| bench.status == BenchStatus::SentBack).map(|n| format!(": {n}")).unwrap_or_default();
            format!("{status} {} · {}{note}", c.label, short(c.age))
        }
        Some(c) => format!("{status} · holds {}", c.label),
        None => status.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benches_take_more_columns_only_when_one_would_squash_them() {
        // Three workers in a tall column: one column, each strip at its most.
        assert_eq!(bench_grid(3, vec2(500.0, 300.0)), (1, BENCH_MOST));
        // Twelve on a laptop: two columns of six, every strip with its two lines.
        let (cols, height) = bench_grid(12, vec2(500.0, 300.0));
        assert_eq!(cols, 2);
        assert!(height >= BENCH_LEAST, "{height}");
        // Twelve on a big screen: one column of short strips.
        let (cols, height) = bench_grid(12, vec2(900.0, 700.0));
        assert_eq!(cols, 1);
        assert!((BENCH_LEAST..=BENCH_MOST).contains(&height), "{height}");
        // Too narrow for a second column: one column, each on one line.
        let (cols, height) = bench_grid(12, vec2(300.0, 300.0));
        assert_eq!(cols, 1);
        assert!(height < BENCH_TWO_LINES);
        // None, or a room with no size: never a division by nothing.
        assert_eq!(bench_grid(0, vec2(0.0, 0.0)).0, 1);
    }

    fn block(took: Option<i64>) -> Block {
        Block {
            t: swarmception_model::Timestamp(0),
            pr: None,
            issue: took.map(|_| 1),
            label: String::new(),
            mark: String::new(),
            colour: [0, 0, 0],
            age: 0,
            took,
            segments: Vec::new(),
            sent_back: 0,
            tooltip: Vec::new(),
        }
    }

    #[test]
    fn a_bricks_width_is_its_time_clamped() {
        let sizes = (30.0, 46.0, 184.0);
        let (least, base, most) = sizes;
        assert_eq!(brick_width(Some(600), Some(600), sizes), base, "the median ticket is the base width");
        assert_eq!(brick_width(Some(1_200), Some(600), sizes), base * 2.0, "twice as long, twice as wide");
        assert_eq!(brick_width(Some(60_000), Some(600), sizes), most, "one long ticket can't take the row");
        assert_eq!(brick_width(Some(1), Some(600), sizes), least, "a quick one still holds its number");
        assert_eq!(brick_width(None, Some(600), sizes), least, "a merge no ticket names stays small");
        assert_eq!(brick_width(Some(5), Some(0), sizes), least, "no median: nothing to divide by");
    }

    #[test]
    fn the_wall_packs_rows_by_width_and_shrinks_before_it_scrolls() {
        // A hundred bricks, every tenth one long, in a laptop's main column.
        let blocks: Vec<Block> = (0..100).map(|n| block(Some(if n % 10 == 0 { 6_000 } else { 900 }))).collect();
        let labels = vec![20.0; blocks.len()];
        let wall = wall_layout(&blocks, &labels, vec2(290.0, 380.0));
        assert_eq!(wall.places.len(), 100);
        for place in &wall.places {
            assert!(place.x + place.width <= 290.0 + 0.01, "{place:?} inside the row");
        }
        for pair in wall.places.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(b.row > a.row || b.x >= a.x + a.width + BRICK_GAP - 0.01, "in order, never overlapping: {a:?} {b:?}");
        }
        assert_eq!(wall.height, 20.0, "too many for the larger sizes: the smallest, to scroll");
        let long = wall.places[0].width;
        assert!(long > wall.places[1].width * 2.0, "the long ones stand out: {long} vs {}", wall.places[1].width);
        // A few bricks fit at the largest size.
        let few = wall_layout(&blocks[..12], &labels, vec2(290.0, 380.0));
        assert_eq!(few.height, 26.0);
        assert!(few.rows as f32 * (few.height + BRICK_GAP) <= 380.0 + BRICK_GAP);
        // Nothing, or no room: no rows, no panic.
        assert_eq!(wall_layout(&[], &[], vec2(290.0, 380.0)).rows, 0);
        assert_eq!(wall_layout(&blocks[..3], &[], vec2(0.0, 0.0)).places.len(), 3);
        // No brick narrower than its number at the size it is drawn.
        for (place, label) in wall.places.iter().zip(&labels) {
            assert!(place.width >= label * brick_font(wall.height) / 10.0 + 8.0 - 0.01, "{place:?}");
        }
    }

    #[test]
    fn a_narrow_or_short_bench_draws_smaller_chips() {
        let wide = Rect::from_min_size(Pos2::ZERO, vec2(480.0, 50.0));
        let narrow = Rect::from_min_size(Pos2::ZERO, vec2(250.0, 50.0));
        assert_eq!(bench_sizes(wide, false)[0], Size::Large);
        assert_eq!(bench_sizes(narrow, false)[0], Size::Medium);
        assert!(bench_sizes(wide, true).iter().all(|s| *s == Size::Small));
    }
}
