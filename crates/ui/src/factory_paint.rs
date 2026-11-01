//! Drawing the factory's floor from a [`FactoryScene`], with egui's painter: five stations left to
//! right (up next and the shelf, the benches, the dock, the gate, main), what just happened and
//! a legend under them. Every ticket is the same chip at every station ([`crate::factory_chip`]):
//! its fill its own colour, its border its phase, only its size changing. Every chip, bench, run
//! and brick has its tooltip, the scene's own lines. [`Drawn`] records what each station drew, for
//! a test to check against `factory.at`.
//!
//! Hovering a ticket rings it at every station it is drawn at (and the shell selects its bars on
//! the timeline); clicking it traces its journey through the stations, with the time at each.
//! Chips glide and resize between stations; nothing else moves by the wall clock: the gate's
//! press, the gears and the glow of what just happened follow *t*, so a still *t* draws a still
//! factory.

use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};
use swarmception_domain::factory::{Bench, BenchStatus, Block, Crate, FactoryScene, GateRun, Happening, HappeningKind, Outcome, Shift, Station, short};

use crate::factory_chip::{CLAIMED, Chip, GATING, Look, Motion, PASSED, SENT_BACK, STOPPED, Size, WAITING, WORKING, ink, look, look_after, paint_chip, rgb};
use crate::icons;

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
    pub happenings: usize,
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
    /// The journey traced, its legs' labels in order.
    pub journey: Vec<String>,
    /// What each brick in main's wall says, oldest first.
    pub marks: Vec<String>,
}

/// What the floor is asked to show besides the scene: the ticket hovered last frame, the
/// ticket whose journey is traced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub hovered: Option<u64>,
    pub traced: Option<u64>,
}

const IDLE: Color32 = Color32::from_rgb(0x7f, 0xb0, 0x8f);
const MAIN: Color32 = Color32::from_rgb(0x8e, 0x6c, 0xd4);

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

const fn happening_colour(kind: HappeningKind) -> Color32 {
    match kind {
        HappeningKind::Claimed => CLAIMED,
        HappeningKind::Began => WORKING,
        HappeningKind::Submitted => WAITING,
        HappeningKind::SentBack | HappeningKind::GateFailed => SENT_BACK,
        HappeningKind::Stopped | HappeningKind::Released => STOPPED,
        HappeningKind::Merged | HappeningKind::Done | HappeningKind::GatePassed => PASSED,
        HappeningKind::Landed => MAIN,
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
pub fn paint(ui: &egui::Ui, rect: Rect, scene: &FactoryScene, motion: &mut Motion, marks: Marks) -> Drawn {
    motion.begin(ui.input(|i| i.time), rect);
    let floor_click = ui.interact(rect, ui.scope_id().with("factory-floor"), egui::Sense::click()).clicked();
    let mut floor = Floor { ui, painter: ui.painter_at(rect), pal: Palette::of(ui.visuals()), motion, drawn: Drawn::default(), scene, marks };
    let strip = (rect.height() * 0.26).clamp(52.0, 118.0);
    let (stations, under) = rect.split_top_bottom_at_y(rect.max.y - strip);
    let columns = columns(stations.shrink2(vec2(2.0, 2.0)));
    floor.intake(columns[0]);
    floor.benches(columns[1]);
    floor.dock(columns[2]);
    floor.gate(columns[3]);
    floor.shipping(columns[4]);
    for pair in columns.windows(2) {
        floor.arrow(pair[0].right(), pair[1].left(), pair[0].center().y);
    }
    let (ticker, legend) = under.shrink2(vec2(2.0, 4.0)).split_left_right_at_fraction(0.7);
    floor.ticker(ticker);
    floor.legend(legend.shrink2(vec2(6.0, 0.0)));
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
    const SHARES: [f32; 5] = [0.11, 0.36, 0.13, 0.17, 0.23];
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
        response.on_hover_text(lines.join("\n"));
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
        self.pointed(rect, c.issue, &c.tooltip);
        rect
    }

    /// The tooltip of ticket `issue`'s chip at `rect`, and whether the pointer is on it or clicks it.
    fn pointed(&mut self, rect: Rect, issue: u64, tooltip: &[String]) {
        let id = self.ui.scope_id().with(("factory-chip", issue, self.drawn.chips.len()));
        let response = self.ui.interact(rect, id, egui::Sense::click());
        if response.hovered() {
            self.drawn.hovered = Some(issue);
        }
        if response.clicked() {
            self.drawn.clicked = Some(Some(issue));
        }
        response.on_hover_text(tooltip.join("\n"));
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
        let cols = if n > 3 && room.width() > 480.0 { 2 } else { 1 };
        let rows = n.div_ceil(cols);
        let height = (room.height() / rows as f32).min(78.0);
        let width = room.width() / cols as f32;
        for (i, bench) in scene.benches.iter().enumerate() {
            let (col, row) = ((i / rows) as f32, (i % rows) as f32);
            let cell = Rect::from_min_size(room.left_top() + vec2(col * width, row * height), vec2(width, height)).shrink(2.5);
            self.bench(cell, bench);
        }
    }

    fn bench(&mut self, cell: Rect, bench: &Bench) {
        let status = bench_colour(bench.status);
        let lit = bench.status == BenchStatus::Working;
        let edge = if lit { Stroke::new(1.5, status) } else { Stroke::new(1.0, self.pal.edge) };
        self.painter.rect(cell, 5.0, self.pal.cell, edge, StrokeKind::Inside);
        let compact = cell.height() < 44.0;
        let radius = if compact { 8.0 } else { (cell.height() / 2.0 - 8.0).clamp(8.0, 14.0) };
        let avatar = pos2(cell.left() + 8.0 + radius, cell.center().y);
        self.actor(avatar, radius, bench);
        let held = bench.crates.len().saturating_sub(1).min(2) as f32;
        let crates_width = if bench.crates.is_empty() { 0.0 } else { Size::Large.dims().x + 12.0 } + held * (Size::Medium.dims().x + 8.0);
        let text = Rect::from_min_max(pos2(avatar.x + radius + 8.0, cell.top()), pos2(cell.right() - crates_width - 4.0, cell.bottom()));
        let name = self.bench_text(text, bench, compact);
        self.helper(bench, name, text.right());
        self.tooltip(cell, ("bench", &bench.key), &bench.tooltip);
        self.bench_crates(cell, bench, compact);
    }

    /// The worker's name, its model and effort, and its status line, in `room`: the name's rect.
    fn bench_text(&self, room: Rect, bench: &Bench, compact: bool) -> Rect {
        let width = room.width().max(10.0);
        let (at, anchor) = if compact { (room.left_center(), Align2::LEFT_CENTER) } else { (room.left_top() + vec2(0.0, 6.0), Align2::LEFT_TOP) };
        let name = fit(&self.painter, at, anchor, &bench.actor.name, (FontId::proportional(13.0), self.pal.text), width);
        if compact {
            return name;
        }
        if let Some(model) = bench.actor.model.as_deref().filter(|_| room.height() >= 58.0) {
            let text = bench.actor.effort.as_ref().map_or_else(|| model_name(model).to_owned(), |e| format!("{} · {e}", model_name(model)));
            fit(&self.painter, name.left_bottom() + vec2(0.0, 1.0), Align2::LEFT_TOP, &text, (Self::font(10.0), self.pal.weak), width);
        }
        let line = status_line(bench);
        fit(&self.painter, room.left_bottom() - vec2(0.0, 6.0), Align2::LEFT_BOTTOM, &line, (Self::font(11.0), bench_colour(bench.status)), width);
        name
    }

    /// The ticket the bench works on, large at its right with its progress, and up to two more
    /// it holds, medium.
    fn bench_crates(&mut self, cell: Rect, bench: &Bench, compact: bool) {
        let mut x = cell.right() - 8.0;
        let mut drawn = Vec::new();
        for (n, c) in bench.crates.iter().take(3).enumerate() {
            let size = match (n, compact) {
                (0, false) => Size::Large,
                (_, false) => Size::Medium,
                (_, true) => Size::Small,
            };
            let y = cell.center().y - if size == Size::Large { 3.0 } else { 0.0 };
            let rect = Rect::from_center_size(pos2(x - size.dims().x / 2.0, y), size.dims());
            let drawn_at = self.chip(c, rect.center(), size);
            if size == Size::Large {
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

    fn gate(&mut self, column: Rect) {
        let gate = &self.scene.gate;
        let room = self.card(column, "GATE", &format!("{} {}  {} {}", icons::CHECK, gate.passed, icons::CROSS, gate.failed));
        let press = Rect::from_min_size(room.left_top(), vec2(room.width(), (room.height() * 0.42).clamp(54.0, 104.0)));
        self.press(press);
        let mut y = press.bottom() + 4.0;
        let status = match (gate.running.first(), gate.recent.first()) {
            (Some(run), _) => (format!("running {} · {}", run.sub, short(run.age)), WORKING),
            (None, Some(last)) => (format!("idle · last {} {} ago", outcome_word(last.outcome), short(last.age)), self.pal.weak),
            (None, None) => ("idle · no runs yet".to_owned(), self.pal.weak),
        };
        fit(&self.painter, pos2(room.left(), y), Align2::LEFT_TOP, &status.0, (Self::font(11.0), status.1), room.width());
        y += 17.0;
        let runs: Vec<&GateRun> = gate.running.iter().chain(&gate.recent).collect();
        let bottom = room.bottom() - if gate.inspectors.is_empty() { 0.0 } else { 15.0 };
        for (n, run) in runs.iter().enumerate() {
            if y + 15.0 > bottom {
                break;
            }
            self.run_line(Rect::from_min_size(pos2(room.left(), y), vec2(room.width(), 15.0)), run, n);
            y += 15.0;
        }
        let inspectors: Vec<String> = gate.inspectors.iter().map(|a| format!("{} · {}", a.name, shift_word(a.shift))).collect();
        if !inspectors.is_empty() {
            fit(&self.painter, room.left_bottom(), Align2::LEFT_BOTTOM, &inspectors.join("  "), (Self::font(10.0), self.pal.weak), room.width());
        }
    }

    /// The inspection press: its beam and lamp, the piston (down while a run goes, with *t*), and
    /// the crates being gated on its bed.
    fn press(&mut self, press: Rect) {
        let gate = &self.scene.gate;
        let frame = self.pal.weak;
        let beam = Rect::from_min_size(press.left_top(), vec2(press.width(), 10.0));
        let bed = Rect::from_min_max(pos2(press.left(), press.bottom() - 6.0), press.right_bottom());
        self.painter.rect_filled(beam, 2.0, frame);
        self.painter.rect_filled(bed, 2.0, frame);
        for x in [press.left() + 3.0, press.right() - 3.0] {
            self.painter.line_segment([pos2(x, beam.bottom()), pos2(x, bed.top())], Stroke::new(4.0, frame));
        }
        let last = gate.recent.first();
        let lamp = match (gate.running.is_empty(), last) {
            (false, _) => (WORKING, 1.0),
            (true, Some(run)) => (outcome_colour(run.outcome), self.fresh(run.age)),
            (true, None) => (STOPPED, 0.0),
        };
        let lamp_at = beam.center();
        if lamp.1 > 0.0 {
            self.painter.circle_filled(lamp_at, 7.0 + 5.0 * lamp.1, lamp.0.gamma_multiply(0.35 * lamp.1));
        }
        self.painter.circle(lamp_at, 6.0, if lamp.1 > 0.0 { lamp.0 } else { lamp.0.gamma_multiply(0.5) }, Stroke::new(1.0, Color32::BLACK.gamma_multiply(0.6)));
        let chip = Size::Medium.dims();
        let travel = (bed.top() - beam.bottom() - chip.y - 10.0).max(0.0);
        let stroke = if gate.running.is_empty() {
            0.0
        } else {
            let phase = (self.scene.t.0.rem_euclid(120) as f32) / 120.0 * std::f32::consts::TAU;
            0.35 + 0.3 * phase.sin()
        };
        let head = beam.bottom() + 2.0 + travel * stroke;
        let piston = Rect::from_center_size(pos2(press.center().x, (beam.bottom() + head) / 2.0), vec2(6.0, head - beam.bottom()));
        self.painter.rect_filled(piston, 0.0, frame);
        self.painter.rect_filled(Rect::from_center_size(pos2(press.center().x, head + 2.0), vec2(press.width() * 0.6, 4.0)), 1.0, frame);
        let inner = press.width() - 14.0;
        let fits = ((inner + 8.0) / (chip.x + 8.0)).floor().max(1.0) as usize;
        let gating = &gate.crates;
        let shown = gating.len().min(fits);
        let row = shown as f32 * (chip.x + 8.0) - 8.0;
        for (n, c) in gating.iter().take(shown).enumerate() {
            let x = press.center().x - row / 2.0 + chip.x / 2.0 + n as f32 * (chip.x + 8.0);
            self.chip(c, pos2(x, bed.top() - chip.y / 2.0 - 4.0), Size::Medium);
            self.drawn.gate.push(c.issue);
        }
        self.drawn.hidden += gating.len() - shown;
        for run in &gate.running {
            self.drawn.runs.push(run.sub.clone());
        }
    }

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
        let font = Self::font(10.5);
        self.painter.text(line.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER, mark, font.clone(), colour);
        let age = self.painter.text(line.right_center() - vec2(3.0, 0.0), Align2::RIGHT_CENTER, short(run.age), font.clone(), self.pal.weak);
        let what = run.cause.as_ref().map_or_else(|| run.sub.clone(), |cause| format!("{} · {cause}", run.sub));
        let width = age.left() - line.left() - 22.0;
        fit(&self.painter, line.left_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, &what, (font, self.pal.text), width);
        self.tooltip(line, ("run", n, &run.sub), &run.tooltip);
        if run.outcome != Outcome::Running {
            self.drawn.runs.push(run.sub.clone());
        }
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

    /// The app on main: a block per merge, stacked from the ground up, the newest glowing.
    fn building(&mut self, site: Rect) {
        let blocks = &self.scene.shipping.blocks;
        let ground = Rect::from_min_max(pos2(site.left(), site.bottom() - 4.0), site.right_bottom());
        self.painter.rect_filled(ground, 1.0, PASSED.gamma_multiply(0.7));
        let area = site.with_max_y(ground.top() - 2.0);
        // The wall's bricks: the small chip when they fit, smaller as the wall grows.
        let sizes = [Size::Small.dims(), vec2(26.0, 13.0), vec2(18.0, 10.0), vec2(12.0, 7.0), vec2(8.0, 5.0), vec2(5.0, 4.0)];
        let gap = 3.0;
        let fits = |size: Vec2| {
            let cols = ((area.width() + gap) / (size.x + gap)).floor().max(1.0) as usize;
            let rows = ((area.height() + gap) / (size.y + gap)).floor().max(0.0) as usize;
            (cols, cols * rows)
        };
        let size = sizes.into_iter().find(|s| fits(*s).1 >= blocks.len()).unwrap_or(sizes[5]);
        let (cols, room) = fits(size);
        let skip = blocks.len().saturating_sub(room);
        for (n, block) in blocks.iter().skip(skip).enumerate() {
            let (col, row) = ((n % cols) as f32, (n / cols) as f32);
            let min = pos2(area.left() + col * (size.x + gap), area.bottom() - (row + 1.0) * (size.y + gap) + gap);
            self.block(Rect::from_min_size(min, size), block);
        }
        self.drawn.blocks = blocks.len() - skip;
    }

    /// A merge's brick in the wall: its ticket's chip, small and plain (no border, its number
    /// only), flown in from where the chip was; a merge no ticket names is main's own colour.
    fn block(&mut self, rect: Rect, block: &Block) {
        let rect = match block.issue {
            Some(issue) => self.motion.land(issue, rect),
            None => rect,
        };
        let fresh = self.fresh(block.age);
        if fresh > 0.0 {
            self.painter.rect_filled(rect.expand(2.0 + 3.0 * fresh), 4.0, PASSED.gamma_multiply(0.5 * fresh));
        }
        let label = &block.mark;
        let highlight = block.issue.filter(|i| self.marks.hovered == Some(*i)).map(|_| self.ring());
        paint_chip(&self.painter, rect, &Chip { colour: block.colour, look: Look::Plain, label, ground: self.pal.card, highlight, glow: 0.0 });
        self.drawn.marks.push(block.mark.clone());
        match block.issue {
            Some(issue) => self.pointed(rect, issue, &block.tooltip),
            None => self.tooltip(rect, ("block", block.t.0, block.pr), &block.tooltip),
        }
    }

    /// What just happened, newest first, in columns.
    fn ticker(&mut self, rect: Rect) {
        let scene = self.scene;
        self.painter.rect(rect, 6.0, self.pal.card, Stroke::new(1.0, self.pal.edge), StrokeKind::Inside);
        let title = format!("JUST HAPPENED · the {} before t", short(scene.recent));
        self.painter.text(rect.left_top() + vec2(8.0, 5.0), Align2::LEFT_TOP, title, Self::font(11.0), self.pal.weak);
        let room = Rect::from_min_max(rect.left_top() + vec2(8.0, 21.0), rect.right_bottom() - vec2(8.0, 4.0));
        if scene.happenings.is_empty() {
            self.painter.text(room.left_top(), Align2::LEFT_TOP, "nothing", Self::font(11.0), self.pal.weak);
            return;
        }
        let per_column = ((room.height() / 15.0).floor().max(1.0)) as usize;
        let columns = if room.width() > 640.0 { 2 } else { 1 };
        let width = room.width() / columns as f32;
        for (n, happening) in scene.happenings.iter().take(per_column * columns).enumerate() {
            let at = room.left_top() + vec2((n / per_column) as f32 * width, (n % per_column) as f32 * 15.0);
            self.happening(Rect::from_min_size(at, vec2(width - 8.0, 15.0)), happening);
            self.drawn.happenings += 1;
        }
    }

    /// A line of the ticker: the ticket's chip, tiny, in the look the happening left it in (a
    /// lamp for the gate and main), its age and what happened; ringed while its ticket is hovered.
    fn happening(&mut self, line: Rect, happening: &Happening) {
        let fresh = self.fresh(happening.age);
        let dot = Rect::from_center_size(line.left_center() + vec2(11.0, 0.0), vec2(18.0, 10.0));
        match (happening.issue, happening.colour) {
            (Some(issue), Some(colour)) => {
                if self.marks.hovered == Some(issue) {
                    self.painter.rect_filled(line.expand2(vec2(3.0, 0.0)), 3.0, self.ring().gamma_multiply(0.12));
                }
                let chip = Chip { colour, look: look_after(happening.kind), label: "", ground: self.pal.card, highlight: None, glow: 0.0 };
                paint_chip(&self.painter, dot, &chip);
                self.pointed(line, issue, std::slice::from_ref(&happening.text));
            }
            _ => {
                self.painter.circle_filled(dot.center(), 4.5, happening_colour(happening.kind));
            }
        }
        let age = self.painter.text(line.left_center() + vec2(26.0, 0.0), Align2::LEFT_CENTER, short(happening.age), Self::font(10.5), self.pal.weak);
        let text = self.pal.weak.lerp_to_gamma(self.pal.text, 0.4 + 0.6 * fresh);
        let left = (line.left() + 62.0).max(age.right() + 8.0);
        fit(&self.painter, pos2(left, line.center().y), Align2::LEFT_CENTER, &happening.text, (Self::font(11.0), text), line.right() - left);
    }

    /// What a chip's look means: its border the phase, its fill the ticket.
    fn legend(&self, rect: Rect) {
        self.painter.text(rect.left_top(), Align2::LEFT_TOP, "border = phase · fill = ticket", Self::font(11.0), self.pal.weak);
        let ground = self.pal.card;
        let sample = [0x5c, 0x5f, 0x70];
        let phase = |colour: Color32, label: &'static str| (Look::Phase(colour), label);
        let items = [
            (Look::Hollow, "up next (hollow)"),
            phase(CLAIMED, "claimed"),
            phase(WORKING, "working"),
            phase(SENT_BACK, "sent back"),
            phase(STOPPED, "stopped / shelved"),
            phase(WAITING, "waiting for merge"),
            phase(GATING, "in the gate"),
            (Look::Plain, "merged: a brick on main"),
        ];
        let grey = swarmception_domain::factory::NO_TICKET_COLOUR;
        let room = rect.with_min_y(rect.top() + 17.0);
        let rows = ((room.height() / 15.0).floor().max(1.0)) as usize;
        let items = items.iter().map(|(look, label)| (sample, *look, *label)).chain([(grey, Look::Plain, "grey !13: a merge, no ticket")]);
        let count = 9; // the eight looks, and the grey brick
        let rows = rows.min(count);
        let columns = count.div_ceil(rows);
        let width = room.width() / columns.max(1) as f32;
        for (n, (colour, look, label)) in items.enumerate() {
            let at = room.left_top() + vec2((n / rows) as f32 * width, (n % rows) as f32 * 15.0 + 7.0);
            let swatch = Rect::from_center_size(at + vec2(11.0, 0.0), vec2(18.0, 10.0));
            paint_chip(&self.painter, swatch, &Chip { colour, look, label: "", ground, highlight: None, glow: 0.0 });
            fit(&self.painter, at + vec2(26.0, 0.0), Align2::LEFT_CENTER, label, (Self::font(10.5), self.pal.text), width - 30.0);
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
            .map_or_else(|| swarmception_domain::factory::ticket_colour(journey.issue), |b| b.colour);
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
        Outcome::Unknown => "ended",
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
