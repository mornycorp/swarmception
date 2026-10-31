//! The merges onto a swarm's `main`: its first-parent merge commits, as `git log` prints them
//! (`%H %ct %s`, separated by [`FIELD`]), parsed from hostile text. A line that doesn't read is
//! left out; a subject is capped like any other text.

use crate::{Timestamp, capped};

/// What separates a log line's fields: the unit separator, which a subject doesn't hold.
pub const FIELD: char = '\u{1f}';

/// One merge commit on `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MainMerge {
    /// When it was committed.
    pub t: Timestamp,
    /// Its commit.
    pub commit: String,
    /// The pull request its subject names: `Merge pull request #<n> from …`.
    pub pr: Option<u64>,
    pub subject: String,
}

/// One line of `git log --format=%H%x1f%ct%x1f%s`; none when it isn't one.
#[must_use]
pub fn parse_merge(line: &str) -> Option<MainMerge> {
    let mut fields = line.splitn(3, FIELD);
    let commit = fields.next()?.trim();
    let seconds: i64 = fields.next()?.trim().parse().ok()?;
    let subject = capped(fields.next().unwrap_or_default().trim().to_owned());
    if commit.is_empty() || commit.len() > 64 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let pr = subject.strip_prefix("Merge pull request #").and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok());
    Some(MainMerge { t: Timestamp(seconds), commit: commit.to_owned(), pr, subject })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_requests_merge() {
        let merge = parse_merge("be043bc0\u{1f}1793420113\u{1f}Merge pull request #173 from mornycorp/170-spec").unwrap();
        assert_eq!(merge.pr, Some(173));
        assert_eq!(merge.commit, "be043bc0");
        assert_eq!(merge.t.to_string(), "2026-10-31T04:15:13Z");
    }

    #[test]
    fn other_merges_and_bad_lines() {
        assert_eq!(parse_merge("abc\u{1f}10\u{1f}Merge branch 'x'").unwrap().pr, None);
        assert_eq!(parse_merge("abc\u{1f}10").unwrap().subject, "");
        for bad in ["", "abc", "abc\u{1f}soon\u{1f}x", "not-hex\u{1f}10\u{1f}x", "\u{1f}10\u{1f}x"] {
            assert_eq!(parse_merge(bad), None, "{bad:?}");
        }
    }
}
