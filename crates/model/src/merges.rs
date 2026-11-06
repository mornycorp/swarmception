//! The merges onto a swarm's `main`: its first-parent merge commits, as `git log` prints them
//! (`%H %ct %B`, separated by [`FIELD`], records separated by NUL), parsed from hostile text.
//! A record that doesn't read is left out; a subject is capped like any other text.

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
    /// The request named by a GitHub merge subject or GitLab's `See merge request` trailer.
    pub pr: Option<u64>,
    pub subject: String,
    /// Explicit local issue-closing references in the full commit message, including batches.
    pub closes: Vec<u64>,
}

/// One record of `git log --format=%H%x1f%ct%x1f%B%x00`, without its NUL; none when malformed.
#[must_use]
pub fn parse_merge(line: &str) -> Option<MainMerge> {
    let mut fields = line.splitn(3, FIELD);
    let commit = fields.next()?.trim();
    let seconds: i64 = fields.next()?.trim().parse().ok()?;
    let message = fields.next().unwrap_or_default().trim();
    let subject = capped(message.lines().next().unwrap_or_default().to_owned());
    if commit.is_empty() || commit.len() > 64 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let message = message.get(..message.floor_char_boundary(crate::issues::MAX_BODY.min(message.len()))).unwrap_or_default();
    let pr = subject.strip_prefix("Merge pull request #").and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).or_else(|| {
        message.lines().find_map(|line| {
            let reference = line.strip_prefix("See merge request ")?.trim();
            let (project, number) = reference.rsplit_once('!')?;
            (!project.is_empty() && number.bytes().all(|b| b.is_ascii_digit())).then(|| number.parse().ok()).flatten()
        })
    });
    let closes = crate::issues::closing_refs(message);
    Some(MainMerge { t: Timestamp(seconds), commit: commit.to_owned(), pr, subject, closes })
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

    #[test]
    fn gitlab_batch_reads_closures_from_the_body_not_incidental_mentions() {
        let merge = parse_merge(
            "abc\u{1f}1791922139\u{1f}Merge branch 'batch-1' into 'main'\n\nRelated to #88\nCloses #12, #3, and #9\n\nSee merge request cmaughan/ns!8",
        )
        .unwrap();
        assert_eq!(merge.pr, Some(8));
        assert_eq!(merge.closes, [12, 3, 9]);
        assert_eq!(merge.subject, "Merge branch 'batch-1' into 'main'");
    }

    #[test]
    fn closure_lists_stop_at_prose_newlines_and_foreign_references() {
        assert_eq!(crate::issues::closing_refs("Closes #12,#3 and #9. Related #88\n#77\nFixes other/repo#6\nResolves: #4"), [12, 3, 9, 4]);
        assert_eq!(crate::issues::closing_refs("Closes #1 and discusses #2, #3\nCloses #1, #4\nCloses #0, #5"), [1, 4]);
        let overlong = format!("{}\nCloses #99", "x".repeat(crate::issues::MAX_BODY));
        let merge = parse_merge(&format!("abc\u{1f}10\u{1f}{overlong}")).unwrap();
        assert!(merge.closes.is_empty());
    }
}
