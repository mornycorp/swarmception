//! The factory's tooltips in colour (#31, the owner's review): the scene's lines as they are,
//! each phrase that names a phase in that phase's colour (the theme's tokens, #152, in the look
//! drawn in, #154: claimed `warning`, working `strong`, waiting for merge `accent`, in the gate
//! `gate`, sent back `error`, stopped `muted`, merged `success`), what passed `success`, what
//! failed `error` and what has no result `muted` (a gate run that never finished, #42), durations
//! emphasised and clock times quiet. The first line, what the
//! tooltip is of, is strong. The words are the scene's: only how they are drawn is decided here.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use swarmception_domain::colours::Paint;

use crate::factory_chip::{CLAIMED, GATING, PASSED, SENT_BACK, STOPPED, WAITING, WORKING, drawn};
use crate::icons;

/// How a piece of a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// A phase, a pass or a failure, in its colour.
    Colour(Paint),
    /// A duration: `19m`, `3 min 20 s`, `1h05`.
    Duration,
    /// A clock time: `2026-10-31T03:01:00Z`.
    Time,
}

/// The phrases drawn in a colour, longest first where one starts another.
const PHRASES: &[(&str, Paint)] = &[
    ("waiting for merge", WAITING),
    ("done without a merge", STOPPED),
    ("no stop logged", SENT_BACK),
    ("never finished", STOPPED),
    ("no result", STOPPED),
    ("in the gate", GATING),
    ("on the shelf", STOPPED),
    ("off shift", STOPPED),
    ("sent back", SENT_BACK),
    ("works on", WORKING),
    ("abandoned", SENT_BACK),
    ("submitted", WAITING),
    ("finished", WAITING),
    ("released", STOPPED),
    ("claimed", CLAIMED),
    ("working", WORKING),
    ("running", WORKING),
    ("waiting", WAITING),
    ("stopped", STOPPED),
    ("shelved", STOPPED),
    ("passed", PASSED),
    ("merged", PASSED),
    ("landed", PASSED),
    ("failed", SENT_BACK),
    ("paused", CLAIMED),
    ("began", WORKING),
    ("rc=0", PASSED),
];

/// The marks a line's icons give (a run's ✔ and ✖), coloured as their words are.
const MARKS: [(icons::Icon, Paint); 2] = [(icons::CHECK, PASSED), (icons::XMARK, SENT_BACK)];

/// Whether `c` continues a word.
fn wordy(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// The length of the clock time `YYYY-MM-DDTHH:MM:SSZ` at the start of `s`.
fn time_len(s: &str) -> Option<usize> {
    let b = s.as_bytes().get(..20)?;
    let shape = b"0000-00-00T00:00:00Z";
    let fits = b.iter().zip(shape).all(|(c, want)| if *want == b'0' { c.is_ascii_digit() } else { c == want });
    fits.then_some(20)
}

/// The length of the duration at the start of `s`: `42m`, `30s`, `1h05`, `2d01h`, `12 s`,
/// `3 min 20 s`, `1 h 05 min`; none for a number that isn't one (`8 crates`, `5-5`, `0.5`).
fn duration_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let mut i = 0;
    let mut end = None;
    loop {
        let from = i;
        while digit(i) {
            i += 1;
        }
        if i == from {
            break;
        }
        let spaced = b.get(i) == Some(&b' ');
        let j = if spaced { i + 1 } else { i };
        let rest = s.get(j..).unwrap_or("");
        // A unit ends a word, or runs on into the next figure (`1h05`).
        let unit = ["min", "s", "h", "d", "m"]
            .into_iter()
            .find(|u| rest.starts_with(u) && !rest.get(u.len()..).and_then(|r| r.chars().next()).is_some_and(|c| c.is_alphabetic() || c == '_'));
        let Some(unit) = unit else {
            // `1h05`: the minutes after the hours, glued on.
            if end == Some(from) && !wordy(s.get(i..).and_then(|r| r.chars().next())) {
                end = Some(i);
            }
            break;
        };
        i = j + unit.len();
        end = Some(i);
        if b.get(i) == Some(&b' ') && digit(i + 1) {
            i += 1;
        } else if !digit(i) {
            break;
        }
    }
    end
}

/// `line`, cut into pieces and how each is drawn.
#[must_use]
pub fn tones(line: &str) -> Vec<(&str, Tone)> {
    let mut out: Vec<(&str, Tone)> = Vec::new();
    let mut plain = 0;
    let mut i = 0;
    while i < line.len() {
        let rest = line.get(i..).unwrap_or("");
        let before = line.get(..i).and_then(|b| b.chars().next_back());
        let found = if wordy(before) {
            None
        } else {
            let phrase = PHRASES
                .iter()
                .find(|(p, _)| rest.starts_with(p) && !wordy(rest.get(p.len()..).and_then(|r| r.chars().next())))
                .map(|(p, colour)| (p.len(), Tone::Colour(*colour)))
                .or_else(|| MARKS.iter().find(|(mark, _)| rest.starts_with(*mark)).map(|(mark, colour)| (mark.len_utf8(), Tone::Colour(*colour))));
            let number = !matches!(before, Some('#' | '!' | '.' | '=' | '-' | ':' | '/'));
            phrase.or_else(|| time_len(rest).map(|n| (n, Tone::Time))).or_else(|| number.then(|| duration_len(rest)).flatten().map(|n| (n, Tone::Duration)))
        };
        match found {
            Some((len, tone)) => {
                if plain < i {
                    out.push((line.get(plain..i).unwrap_or(""), Tone::Plain));
                }
                out.push((line.get(i..i + len).unwrap_or(""), tone));
                i += len;
                plain = i;
            }
            None => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    if plain < line.len() {
        out.push((line.get(plain..).unwrap_or(""), Tone::Plain));
    }
    out
}

/// The tooltip's `lines` as one coloured text, in `font`, an icon in it in `glyphs`
/// ([`icons::family`], #176).
#[must_use]
pub fn job(lines: &[String], visuals: &egui::Visuals, font: &FontId, glyphs: &egui::FontFamily) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = |color: Color32| TextFormat { font_id: font.clone(), color, ..TextFormat::default() };
    for (n, line) in lines.iter().enumerate() {
        if n > 0 {
            job.append("\n", 0.0, format(visuals.text_color()));
        }
        if n == 0 {
            append(&mut job, line, &format(visuals.strong_text_color()), glyphs);
            continue;
        }
        for (piece, tone) in tones(line) {
            let color = match tone {
                Tone::Plain => visuals.text_color(),
                Tone::Duration => visuals.strong_text_color(),
                Tone::Colour(colour) => drawn(colour, visuals),
                Tone::Time => visuals.weak_text_color(),
            };
            let mut format = format(color);
            if tone == Tone::Duration {
                format.underline = egui::Stroke::new(1.0, color.gamma_multiply(0.4));
            }
            append(&mut job, piece, &format, glyphs);
        }
    }
    job
}

/// `text` in `format`, an icon in it a run in `glyphs` (#176).
fn append(job: &mut LayoutJob, text: &str, format: &TextFormat, glyphs: &egui::FontFamily) {
    let glyph = TextFormat { font_id: FontId::new(format.font_id.size, glyphs.clone()), ..format.clone() };
    for (run, is_icon) in icons::runs(text) {
        job.append(run, 0.0, if is_icon { glyph.clone() } else { format.clone() });
    }
}

/// `response` with `lines` as its tooltip, in colour.
pub fn show(response: egui::Response, lines: &[String]) -> egui::Response {
    if lines.is_empty() {
        return response;
    }
    response.on_hover_ui(|ui| {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = job(lines, ui.visuals(), &font, &icons::family(ui.ctx()));
        // Wrapped at a tooltip's width, as egui's own text tooltips are.
        job.wrap.max_width = ui.spacing().tooltip_width;
        ui.label(job);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coloured(line: &str) -> Vec<(&str, Tone)> {
        tones(line).into_iter().filter(|(_, tone)| *tone != Tone::Plain).collect()
    }

    #[test]
    fn phases_take_their_colours_passes_green_failures_red() {
        assert_eq!(
            coloured("working for 19 min 02 s (since 2026-10-31T03:02:00Z)"),
            [("working", Tone::Colour(WORKING)), ("19 min 02 s", Tone::Duration), ("2026-10-31T03:02:00Z", Tone::Time)]
        );
        assert_eq!(coloured("waiting for merge for 2m"), [("waiting for merge", Tone::Colour(WAITING)), ("2m", Tone::Duration)]);
        assert_eq!(coloured("failed 3m ago: cause=clippy"), [("failed", Tone::Colour(SENT_BACK)), ("3m", Tone::Duration)]);
        assert_eq!(coloured("passed: rc=0"), [("passed", Tone::Colour(PASSED)), ("rc=0", Tone::Colour(PASSED))]);
        assert_eq!(coloured("#58 sent back: flaky"), [("sent back", Tone::Colour(SENT_BACK))]);
        assert_eq!(
            coloured("never finished: gate-166b started 2026-10-31T03:04:34Z"),
            [("never finished", Tone::Colour(STOPPED)), ("2026-10-31T03:04:34Z", Tone::Time)],
            "#42: no result is grey, neither passed nor failed"
        );
        assert_eq!(coloured("no result: 1h05")[0], ("no result", Tone::Colour(STOPPED)));
        // Whole words only: `worker:` is no phase, `unmerged` no merge.
        assert!(coloured("worker: worker-1 unmerged").is_empty());
    }

    #[test]
    fn durations_and_numbers_that_are_not() {
        for d in ["42m", "30s", "1h05", "2d01h", "12 s", "3 min 20 s", "1 h 05 min"] {
            assert_eq!(coloured(&format!("for {d} now")), [(d, Tone::Duration)], "{d}");
        }
        for not in ["8 crates", "opus-5-5", "0.5 h", "#13", "PR #175", "!13", "4 sessions"] {
            assert!(coloured(not).is_empty(), "{not}: {:?}", coloured(not));
        }
    }

    #[test]
    fn the_pieces_put_back_are_the_line() {
        for line in ["claimed   2026-10-31T03:01:00Z", "gate run gate-pr8 · ✔ ok", "", "idle", "naïve 5m — ünïcode"] {
            let joined: String = tones(line).into_iter().map(|(piece, _)| piece).collect();
            assert_eq!(joined, line);
        }
    }

    #[test]
    fn the_first_line_is_strong_and_the_job_holds_every_line() {
        let visuals = egui::Visuals::dark();
        let lines = ["#7 · PR #8".to_owned(), "in the gate for 2m".to_owned()];
        let job = job(&lines, &visuals, &FontId::proportional(13.0), &egui::FontFamily::Proportional);
        assert_eq!(job.text, "#7 · PR #8\nin the gate for 2m");
        assert_eq!(job.sections[0].format.color, visuals.strong_text_color());
        let gating = |job: &LayoutJob, colour| {
            job.sections.iter().any(|s| s.format.color == colour && job.text.get(s.byte_range.start.0..s.byte_range.end.0) == Some("in the gate"))
        };
        assert!(gating(&job, crate::factory_chip::rgb(GATING.dark)));
        let light = egui::Visuals::light();
        let job = super::job(&lines, &light, &FontId::proportional(13.0), &egui::FontFamily::Proportional);
        assert!(gating(&job, crate::factory_chip::rgb(GATING.light)), "the theme's light gate on a light look (#154)");
    }
}
