//! A tooltip's lines with their styling (#35, the owner's: "make the tooltip a bit more colorful;
//! use red/green where necessary"): each line is runs of text ([`TipSpan`]), each in a colour
//! from [`crate::colours`] or the theme's text colour, strong or not. The rows' bars carry them
//! ([`crate::rows::Bar::tooltip_lines`]) beside their plain text, and the timeline's host data
//! hands them to the widget ([`crate::timeline`]). Nothing here is egui's: a front end maps the
//! colours and the strong runs onto its own text.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::colours::{self, Rgb};

/// What passed or merged is drawn in: green, the merges onto main's colour.
pub const PASSED: Rgb = colours::MAIN_MERGE;
/// What failed or was sent back is drawn in: red, the sent back colour.
pub const FAILED: Rgb = colours::SENT_BACK;

/// A run of a tooltip line's text: in `colour`, or the theme's text colour with none, and strong
/// (bold) or not.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TipSpan {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<Rgb>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub strong: bool,
}

/// One line of a tooltip: its runs, left to right. A blank line has none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TipLine {
    pub spans: Vec<TipSpan>,
}

impl TipLine {
    /// `text` in the theme's text colour.
    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self::default().then(text)
    }

    /// `text`, strong.
    #[must_use]
    pub fn strong(text: impl Into<String>) -> Self {
        Self::default().then_strong(text)
    }

    /// `text` in `colour`.
    #[must_use]
    pub fn coloured(text: impl Into<String>, colour: Rgb) -> Self {
        Self::default().then_coloured(text, colour)
    }

    /// The line with `text` added at its end, plain.
    #[must_use]
    pub fn then(self, text: impl Into<String>) -> Self {
        self.with(text.into(), None, false)
    }

    /// The line with `text` added at its end, strong.
    #[must_use]
    pub fn then_strong(self, text: impl Into<String>) -> Self {
        self.with(text.into(), None, true)
    }

    /// The line with `text` added at its end, in `colour`.
    #[must_use]
    pub fn then_coloured(self, text: impl Into<String>, colour: Rgb) -> Self {
        self.with(text.into(), Some(colour), false)
    }

    /// An empty `text` adds no run.
    fn with(mut self, text: String, colour: Option<Rgb>, strong: bool) -> Self {
        if !text.is_empty() {
            self.spans.push(TipSpan { text, colour, strong });
        }
        self
    }

    /// The line's text, its runs run together: what a consumer that wants plain text reads.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }

    /// Whether no run has a colour or is strong.
    #[must_use]
    pub fn is_plain(&self) -> bool {
        self.spans.iter().all(|span| span.colour.is_none() && !span.strong)
    }

    /// The colour of the run whose text is `text`, if one is and has one: for tests and checks.
    #[must_use]
    pub fn colour_of(&self, text: &str) -> Option<Rgb> {
        self.spans.iter().find(|span| span.text == text).and_then(|span| span.colour)
    }

    /// Whether the run whose text is `text` is strong.
    #[must_use]
    pub fn is_strong(&self, text: &str) -> bool {
        self.spans.iter().any(|span| span.text == text && span.strong)
    }
}

impl From<String> for TipLine {
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl From<&str> for TipLine {
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

/// The lines' text, a line each.
#[must_use]
pub fn texts(lines: &[TipLine]) -> Vec<String> {
    lines.iter().map(TipLine::text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_its_runs_and_reads_as_their_text() {
        let line = TipLine::coloured("working", colours::CLAIMED).then(": 03:00 → 03:10 (").then_strong("10 min 00 s").then(")");
        assert_eq!(line.text(), "working: 03:00 → 03:10 (10 min 00 s)");
        assert_eq!(line.colour_of("working"), Some(colours::CLAIMED));
        assert!(line.is_strong("10 min 00 s") && !line.is_strong("working"));
        assert!(!line.is_plain());
        assert!(TipLine::plain("by m-1").is_plain());
        assert!(TipLine::plain("").spans.is_empty(), "a blank line has no runs");
        assert_eq!(texts(&[TipLine::from("a"), TipLine::strong("b")]), ["a", "b"]);
    }

    #[test]
    fn a_plain_run_serializes_as_its_text_alone() {
        let line = TipLine::plain("by m-1").then_coloured("merged", PASSED).then_strong("5 s");
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(json, r#"{"spans":[{"text":"by m-1"},{"text":"merged","colour":[119,212,150]},{"text":"5 s","strong":true}]}"#);
        assert_eq!(serde_json::from_str::<TipLine>(&json).unwrap(), line);
    }
}
