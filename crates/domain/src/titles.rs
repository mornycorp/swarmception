//! What GitHub says of a ticket (#17), as the timeline's and the factory's text: a bar's label
//! with its title (`#58 Architecture review`, the widget eliding what doesn't fit), and the lines
//! a ticket's tooltips start with: its title, then its state and labels, then its pull request.
//! A ticket GitHub's issues don't have (none fetched, or not there) keeps the tooltips it had.

use swarmception_model::{IssueKind, IssueState, Issues, Timestamp};

/// The most characters of a title a bar's label carries.
pub const LABEL_TITLE: usize = 60;
/// The most characters of a title a tooltip line carries.
pub const TOOLTIP_TITLE: usize = 120;

/// `text` to at most `max` characters, `…` ending it when cut.
#[must_use]
pub fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Ticket `issue`'s title, when GitHub has one.
#[must_use]
pub fn title(issues: &Issues, issue: u64) -> Option<&str> {
    issues.get(issue).map(|i| i.title.as_str()).filter(|t| !t.is_empty())
}

/// A bar's label with the title: `#58 Architecture review`; none without one.
#[must_use]
pub fn label(issues: &Issues, issue: u64) -> Option<String> {
    title(issues, issue).map(|t| format!("#{issue} {}", short(t, LABEL_TITLE)))
}

/// The lines a ticket's tooltip starts with, when GitHub has it: `#17 <title>`; its state and
/// labels (`open · type::task, area::ui`); its pull request, the one its events name (`named`)
/// or the newest that closes it (`PR #26 <title> (merged)`). At `at`, a moment of the swarm's,
/// nothing after it is told: the state is closed only if it closed by then, and a pull request's
/// state is left out.
#[must_use]
pub fn header(issues: &Issues, issue: u64, named: Option<u64>, at: Option<Timestamp>) -> Option<Vec<String>> {
    let found = issues.get(issue)?;
    let mut lines = vec![format!("#{issue} {}", short(&found.title, TOOLTIP_TITLE)).trim_end().to_owned()];
    let state = match at {
        Some(t) if found.closed.is_some_and(|c| c <= t) => IssueState::Closed.name(),
        Some(_) => IssueState::Open.name(),
        None => found.state.name(),
    };
    let what = if found.kind == IssueKind::PullRequest { "pull request, " } else { "" };
    let mut line = format!("{what}{state}");
    if !found.labels.is_empty() {
        line.push_str(&format!(" · {}", found.labels.join(", ")));
    }
    if let Some(author) = &found.author {
        line.push_str(&format!(" · by {author}"));
    }
    lines.push(line);
    if let Some(pr) = issues.pr_of(issue, named).filter(|pr| pr.number != issue) {
        let mut line = format!("PR #{}", pr.number);
        if !pr.title.is_empty() {
            line.push_str(&format!(" {}", short(&pr.title, TOOLTIP_TITLE)));
        }
        if at.is_none() && pr.state != IssueState::Unknown {
            line.push_str(&format!(" ({})", pr.state.name()));
        }
        lines.push(line);
    }
    Some(lines)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use swarmception_model::parse_issues;

    /// GitHub's issues for the sample's tickets: #170 (its PR #173 named by its events) and #168.
    pub(crate) fn issues() -> Issues {
        let pages = r#"[
          {"number":170,"title":"The spec, written down","state":"closed","labels":[{"name":"type::task"},{"name":"area::docs"}],
           "user":{"login":"cmaughan"},"closed_at":"2026-10-31T04:28:58Z","html_url":"https://github.com/o/n/issues/170"},
          {"number":173,"title":"Spec: the first draft","state":"closed","html_url":"https://github.com/o/n/pull/173","body":"Closes #170",
           "pull_request":{"merged_at":"2026-10-31T04:28:58Z"}},
          {"number":168,"title":"A very long title that goes on and on and on, well past what any bar could hold at all","state":"open","html_url":"https://github.com/o/n/issues/168"}
        ]"#;
        Issues::new(parse_issues(pages).unwrap().issues)
    }

    #[test]
    fn short_cuts_with_an_ellipsis() {
        assert_eq!(short("abc", 3), "abc");
        assert_eq!(short("abcd", 3), "ab…");
        assert_eq!(short("ab  cd", 4), "ab…");
        assert_eq!(short("ééééé", 2), "é…");
    }

    #[test]
    fn a_label_carries_the_title() {
        let issues = issues();
        assert_eq!(label(&issues, 170).as_deref(), Some("#170 The spec, written down"));
        let long = label(&issues, 168).unwrap();
        assert!(long.ends_with('…') && long.chars().count() <= LABEL_TITLE + 5, "{long}");
        assert_eq!(label(&issues, 5), None);
    }

    #[test]
    fn the_header_is_the_title_then_state_and_labels_then_the_pr() {
        let issues = issues();
        assert_eq!(
            header(&issues, 170, Some(173), None).unwrap(),
            ["#170 The spec, written down", "closed · type::task, area::docs · by cmaughan", "PR #173 Spec: the first draft (merged)"]
        );
        assert_eq!(header(&issues, 170, None, None).unwrap()[2], "PR #173 Spec: the first draft (merged)", "the PR that closes it");
        let before = swarmception_model::Timestamp::parse("2026-10-31T04:00:00Z");
        assert_eq!(
            header(&issues, 170, Some(173), before).unwrap(),
            ["#170 The spec, written down", "open · type::task, area::docs · by cmaughan", "PR #173 Spec: the first draft"],
            "nothing after t: open then, and the PR's state left out"
        );
        assert_eq!(header(&issues, 168, Some(200), None).unwrap()[2], "PR #200", "a PR GitHub didn't list: its number");
        assert_eq!(header(&issues, 173, None, None).unwrap()[1], "pull request, merged");
        assert_eq!(header(&issues, 5, None, None), None);
    }
}
