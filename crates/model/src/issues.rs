//! A repository's GitHub issues and pull requests (#17): what the tracking branch doesn't say about
//! a ticket (its title, state, labels, author, times, page and pull request). Parsed from hostile
//! JSON: what `gh api --paginate repos/<owner>/<name>/issues?state=all` prints, the REST API's
//! issues, which lists the pull requests too (flagged by `pull_request`), one page's array after
//! another. An item that doesn't read is left out and counted; text is capped. An issue's pull
//! request is the newest whose body closes it (`Closes #17`, `fixes #17`, …).
//!
//! [`IssueCache`] is how they are kept on disk per repository: versioned JSON
//! ([`ISSUES_VERSION`]); a cache from an older or newer build still reads, keeping every issue
//! that reads.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// [`IssueCache`]'s shape. 1: the first.
pub const ISSUES_VERSION: u32 = 1;
/// The most issues and pull requests kept.
pub const MAX_ISSUES: usize = 100_000;
/// The most labels kept on one.
pub const MAX_LABELS: usize = 32;
/// The longest title, label, author or URL kept, in bytes; longer is cut.
pub const MAX_FIELD: usize = 512;
/// How much of a pull request's body is read for the issues it closes.
pub const MAX_BODY: usize = 64 * 1024;
/// The most issues one pull request is read as closing.
const MAX_CLOSES: usize = 64;

/// Whether it is an issue or a pull request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    #[default]
    Issue,
    PullRequest,
}

/// Where it stands on GitHub.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IssueState {
    #[default]
    Open,
    Closed,
    /// A pull request merged.
    Merged,
    /// A state a newer build wrote.
    #[serde(other)]
    Unknown,
}

impl IssueState {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::Unknown => "unknown",
        }
    }
}

/// An issue's pull request: the newest that closes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrRef {
    pub number: u64,
    pub title: String,
    pub state: IssueState,
    pub url: String,
}

/// One issue or pull request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Issue {
    pub number: u64,
    pub kind: IssueKind,
    pub title: String,
    pub state: IssueState,
    pub labels: Vec<String>,
    /// Who opened it, by login.
    pub author: Option<String>,
    pub created: Option<Timestamp>,
    pub closed: Option<Timestamp>,
    /// Its page.
    pub url: String,
    /// A pull request's: the issues its body closes, in order.
    pub closes: Vec<u64>,
    /// An issue's: its pull request, the newest that closes it.
    pub pr: Option<PrRef>,
}

/// The issues and pull requests, by number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Issues {
    by_number: BTreeMap<u64, Issue>,
}

impl Issues {
    /// `issues` by number (the last of a number kept), each issue's pull request linked: the
    /// newest pull request whose body closes it.
    #[must_use]
    pub fn new(issues: Vec<Issue>) -> Self {
        let mut by_number: BTreeMap<u64, Issue> = issues.into_iter().take(MAX_ISSUES).map(|i| (i.number, i)).collect();
        let mut links: BTreeMap<u64, PrRef> = BTreeMap::new();
        for pr in by_number.values().filter(|i| i.kind == IssueKind::PullRequest) {
            for closed in &pr.closes {
                links.insert(*closed, PrRef { number: pr.number, title: pr.title.clone(), state: pr.state, url: pr.url.clone() });
            }
        }
        for (number, pr) in links {
            if let Some(issue) = by_number.get_mut(&number).filter(|i| i.kind == IssueKind::Issue) {
                issue.pr = Some(pr);
            }
        }
        Self { by_number }
    }

    #[must_use]
    pub fn get(&self, number: u64) -> Option<&Issue> {
        self.by_number.get(&number)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_number.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_number.is_empty()
    }

    /// How many are pull requests.
    #[must_use]
    pub fn pulls(&self) -> usize {
        self.by_number.values().filter(|i| i.kind == IssueKind::PullRequest).count()
    }

    /// Every one, by number.
    pub fn iter(&self) -> impl Iterator<Item = &Issue> {
        self.by_number.values()
    }

    /// Ticket `issue`'s pull request: `named` (the one its events name) when GitHub has it, else
    /// the newest that closes it; as a bare number when GitHub doesn't have the one named.
    #[must_use]
    pub fn pr_of(&self, issue: u64, named: Option<u64>) -> Option<PrRef> {
        let known = |n: u64| {
            self.get(n).filter(|i| i.kind == IssueKind::PullRequest).map(|i| PrRef {
                number: i.number,
                title: i.title.clone(),
                state: i.state,
                url: i.url.clone(),
            })
        };
        match named {
            Some(n) => Some(known(n).unwrap_or_else(|| PrRef { number: n, title: String::new(), state: IssueState::Unknown, url: String::new() })),
            None => self.get(issue).and_then(|i| i.pr.clone()),
        }
    }
}

/// What a read of `gh api`'s output found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    pub issues: Vec<Issue>,
    /// Items that didn't read as an issue.
    pub skipped: usize,
}

/// Why `gh api`'s output didn't read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuesError(pub String);

impl std::fmt::Display for IssuesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GitHub's issues didn't read: {}", self.0)
    }
}

impl std::error::Error for IssuesError {}

/// An item of the REST API's issues as it comes: unknown fields skipped, every field optional.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawIssue {
    number: Option<u64>,
    title: Option<String>,
    state: Option<String>,
    labels: Vec<RawLabel>,
    user: Option<RawUser>,
    created_at: Option<String>,
    closed_at: Option<String>,
    html_url: Option<String>,
    body: Option<String>,
    pull_request: Option<RawPull>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawLabel {
    Named { name: String },
    Bare(String),
    Other(serde::de::IgnoredAny),
}

#[derive(Deserialize)]
struct RawUser {
    login: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawPull {
    merged_at: Option<String>,
}

/// `text` cut to `cap` bytes at a character boundary.
fn cut(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_owned();
    }
    let mut end = cap;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default().to_owned()
}

impl RawIssue {
    fn issue(self) -> Option<Issue> {
        let number = self.number.filter(|n| *n > 0)?;
        let pull = self.pull_request;
        let kind = if pull.is_some() { IssueKind::PullRequest } else { IssueKind::Issue };
        let merged = pull.as_ref().and_then(|p| p.merged_at.as_deref()).and_then(Timestamp::parse).is_some();
        let state = match self.state.as_deref() {
            Some("open") => IssueState::Open,
            Some("closed") if merged => IssueState::Merged,
            Some("closed") => IssueState::Closed,
            _ => IssueState::Unknown,
        };
        let labels = self
            .labels
            .into_iter()
            .filter_map(|l| match l {
                RawLabel::Named { name } | RawLabel::Bare(name) => Some(cut(&name, MAX_FIELD)),
                RawLabel::Other(_) => None,
            })
            .take(MAX_LABELS)
            .collect();
        let closes = if kind == IssueKind::PullRequest { closing_refs(&cut(self.body.as_deref().unwrap_or_default(), MAX_BODY)) } else { Vec::new() };
        Some(Issue {
            number,
            kind,
            title: cut(self.title.as_deref().unwrap_or_default().trim(), MAX_FIELD),
            state,
            labels,
            author: self.user.and_then(|u| u.login).map(|l| cut(&l, MAX_FIELD)),
            created: self.created_at.as_deref().and_then(Timestamp::parse),
            closed: self.closed_at.as_deref().and_then(Timestamp::parse),
            url: cut(self.html_url.as_deref().unwrap_or_default(), MAX_FIELD),
            closes,
            pr: None,
        })
    }
}

/// Reads what `gh api --paginate repos/<owner>/<name>/issues?state=all` printed: one page's JSON
/// array after another (or one array). Items that don't read as an issue are skipped and counted.
///
/// # Errors
///
/// [`IssuesError`] when the text isn't JSON arrays at all (an error object, say).
pub fn parse_issues(text: &str) -> Result<Parsed, IssuesError> {
    let mut parsed = Parsed::default();
    for page in serde_json::Deserializer::from_str(text).into_iter::<Vec<serde_json::Value>>() {
        let page = page.map_err(|e| IssuesError(e.to_string()))?;
        for item in page {
            if parsed.issues.len() >= MAX_ISSUES {
                parsed.skipped += 1;
                continue;
            }
            match serde_json::from_value::<RawIssue>(item).ok().and_then(RawIssue::issue) {
                Some(issue) => parsed.issues.push(issue),
                None => parsed.skipped += 1,
            }
        }
    }
    Ok(parsed)
}

/// The issues `body` closes, as GitHub reads its keywords: `close`, `closes`, `closed`, `fix`,
/// `fixes`, `fixed`, `resolve`, `resolves`, `resolved`, then `#<n>` (a colon after the keyword
/// allowed), in order, each once.
#[must_use]
pub fn closing_refs(body: &str) -> Vec<u64> {
    const KEYWORDS: [&str; 9] = ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"];
    let mut out = Vec::new();
    for (at, _) in body.match_indices('#') {
        let digits: String = body.get(at + 1..).unwrap_or_default().chars().take_while(char::is_ascii_digit).take(21).collect();
        let Ok(number) = digits.parse::<u64>() else { continue };
        let raw = body.get(..at).unwrap_or_default();
        if !raw.ends_with(|c: char| c.is_whitespace() || c == ':') {
            continue;
        }
        let before = raw.trim_end();
        let before = before.strip_suffix(':').unwrap_or(before).trim_end();
        let word: String = before.chars().rev().take_while(char::is_ascii_alphabetic).collect::<Vec<_>>().into_iter().rev().collect();
        let preceded = before.len() == word.len() || before.get(..before.len() - word.len()).is_some_and(|b| !b.ends_with(|c: char| c.is_alphanumeric()));
        if preceded && KEYWORDS.contains(&word.to_ascii_lowercase().as_str()) && number > 0 && !out.contains(&number) && out.len() < MAX_CLOSES {
            out.push(number);
        }
    }
    out
}

/// The issues of one repository as they are kept on disk, versioned.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssueCache {
    pub version: u32,
    /// `owner/name` on GitHub.
    pub repo: String,
    /// When they were fetched.
    pub fetched: Option<Timestamp>,
    pub issues: Vec<Issue>,
}

/// A cache as it may come from any build: every field optional, each issue read alone.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCache {
    version: u32,
    #[serde(default)]
    repo: String,
    #[serde(default)]
    fetched: Option<Timestamp>,
    #[serde(default)]
    issues: Vec<serde_json::Value>,
}

impl IssueCache {
    /// A cache of `issues`, `repo`'s, fetched at `fetched`.
    #[must_use]
    pub fn new(repo: &str, fetched: Option<Timestamp>, issues: Vec<Issue>) -> Self {
        Self { version: ISSUES_VERSION, repo: repo.to_owned(), fetched, issues }
    }

    /// Reads a cache: one from an older or newer build too, keeping every issue that reads (an
    /// unknown field is a newer build's, skipped).
    ///
    /// # Errors
    ///
    /// [`IssuesError`] for text that isn't a cache at all (no version, not JSON).
    pub fn read(text: &str) -> Result<Self, IssuesError> {
        let raw: RawCache = serde_json::from_str(text).map_err(|e| IssuesError(format!("the cache: {e}")))?;
        let issues = raw.issues.into_iter().take(MAX_ISSUES).filter_map(|v| serde_json::from_value::<Issue>(v).ok()).filter(|i| i.number > 0).collect();
        Ok(Self { version: raw.version, repo: raw.repo, fetched: raw.fetched, issues })
    }

    /// The cache as JSON text.
    #[must_use]
    pub fn write(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two pages as `gh api --paginate` prints them: an issue, a merged PR closing it, an open PR,
    /// and items that don't read.
    pub(crate) const PAGES: &str = r###"[
      {"number":17,"title":"Fetch and cache the repo's issues","state":"open","labels":[{"name":"type::task"},{"name":"area::ui","color":"fff"}],
       "user":{"login":"cmaughan"},"created_at":"2026-10-31T20:00:00Z","closed_at":null,"html_url":"https://github.com/o/n/issues/17","body":"Closes #3","extra":{"x":1}},
      {"number":26,"title":"Issue titles","state":"closed","labels":[],"user":{"login":"bot"},"created_at":"2026-11-01T09:00:00Z","closed_at":"2026-11-01T11:00:00Z",
       "html_url":"https://github.com/o/n/pull/26","body":"## What\n\nCloses #17; also fixes: #18.","pull_request":{"merged_at":"2026-11-01T11:00:00Z"}},
      {"title":"no number"},
      "not an object"
    ]
    [
      {"number":30,"title":"A later try","state":"open","html_url":"https://github.com/o/n/pull/30","body":"Resolves #18","pull_request":{"merged_at":null}},
      {"number":18,"title":"   Padded   ","state":"closed","closed_at":"bad","html_url":"https://github.com/o/n/issues/18"}
    ]"###;

    #[test]
    fn pages_of_issues_and_pull_requests_read() {
        let parsed = parse_issues(PAGES).unwrap();
        assert_eq!(parsed.skipped, 2);
        let numbers: Vec<(u64, IssueKind, IssueState)> = parsed.issues.iter().map(|i| (i.number, i.kind, i.state)).collect();
        assert_eq!(
            numbers,
            [
                (17, IssueKind::Issue, IssueState::Open),
                (26, IssueKind::PullRequest, IssueState::Merged),
                (30, IssueKind::PullRequest, IssueState::Open),
                (18, IssueKind::Issue, IssueState::Closed)
            ]
        );
        let issue = &parsed.issues[0];
        assert_eq!(issue.title, "Fetch and cache the repo's issues");
        assert_eq!(issue.labels, ["type::task", "area::ui"]);
        assert_eq!(issue.author.as_deref(), Some("cmaughan"));
        assert_eq!(issue.created.map(|t| t.to_string()).as_deref(), Some("2026-10-31T20:00:00Z"));
        assert!(issue.closes.is_empty(), "an issue's body closes nothing");
        assert_eq!(parsed.issues[1].closes, [17, 18]);
        assert_eq!(parsed.issues[3].title, "Padded");
        assert_eq!(parsed.issues[3].closed, None, "a time that doesn't read is none");
    }

    #[test]
    fn an_issues_pr_is_the_newest_that_closes_it() {
        let issues = Issues::new(parse_issues(PAGES).unwrap().issues);
        assert_eq!((issues.len(), issues.pulls()), (4, 2));
        let pr = issues.get(17).and_then(|i| i.pr.as_ref()).unwrap();
        assert_eq!((pr.number, pr.title.as_str(), pr.state), (26, "Issue titles", IssueState::Merged));
        assert_eq!(issues.get(18).and_then(|i| i.pr.as_ref()).map(|p| p.number), Some(30), "#30 is newer than #26");
        assert_eq!(issues.get(26).and_then(|i| i.pr.as_ref()), None, "a pull request has none");
        assert_eq!(issues.pr_of(17, None).map(|p| p.number), Some(26));
        assert_eq!(issues.pr_of(17, Some(30)).map(|p| (p.number, p.title)), Some((30, "A later try".to_owned())), "the one its events name");
        let unknown = issues.pr_of(17, Some(99)).unwrap();
        assert_eq!((unknown.number, unknown.title.as_str(), unknown.state), (99, "", IssueState::Unknown));
        assert_eq!(issues.pr_of(5, None), None);
    }

    #[test]
    fn what_isnt_json_arrays_is_an_error() {
        assert!(parse_issues(r#"{"message":"Not Found"}"#).is_err());
        assert!(parse_issues("[1,").is_err());
        assert_eq!(parse_issues("").unwrap(), Parsed::default());
        assert_eq!(parse_issues("[]").unwrap(), Parsed::default());
    }

    #[test]
    fn closing_keywords() {
        assert_eq!(closing_refs("Closes #3, fixes #4 and Resolved: #5"), [3, 4, 5]);
        assert_eq!(closing_refs("closes #3 closes #3"), [3]);
        assert_eq!(closing_refs("see #3; part of #4; enclosed #5; #6"), Vec::<u64>::new());
        assert_eq!(closing_refs("Fix#7"), Vec::<u64>::new(), "GitHub wants a space or colon: this one is closer to a hashtag");
        assert_eq!(closing_refs("fix #"), Vec::<u64>::new());
        assert_eq!(closing_refs("é closes #9 ü"), [9]);
        assert_eq!(closing_refs("closes #99999999999999999999"), Vec::<u64>::new(), "too big to be one");
    }

    #[test]
    fn long_text_is_cut() {
        let title = "é".repeat(MAX_FIELD);
        let text = format!(r#"[{{"number":1,"title":"{title}","labels":[{}]}}]"#, vec![r#"{"name":"l"}"#; 100].join(","));
        let issue = &parse_issues(&text).unwrap().issues[0];
        assert!(issue.title.len() <= MAX_FIELD);
        assert_eq!(issue.labels.len(), MAX_LABELS);
    }

    #[test]
    fn the_cache_round_trips_with_its_version() {
        let issues = parse_issues(PAGES).unwrap().issues;
        let cache = IssueCache::new("o/n", Some(Timestamp(1_793_500_000)), issues);
        assert_eq!(cache.version, ISSUES_VERSION);
        let back = IssueCache::read(&cache.write()).unwrap();
        assert_eq!(back, cache);
    }

    #[test]
    fn a_newer_or_older_cache_keeps_what_reads() {
        let newer = r#"{"version":9,"repo":"o/n","fetched":5,"later":true,"issues":[
            {"number":1,"title":"one","state":"draft","kind":"issue","newField":[1]},
            {"number":"two"},
            {"number":3,"title":"three"}
        ]}"#;
        let cache = IssueCache::read(newer).unwrap();
        assert_eq!(cache.version, 9);
        assert_eq!(cache.issues.iter().map(|i| (i.number, i.state)).collect::<Vec<_>>(), [(1, IssueState::Unknown), (3, IssueState::Open)]);
        let older = IssueCache::read(r#"{"version":0}"#).unwrap();
        assert!(older.issues.is_empty());
        assert!(IssueCache::read(r#"{"issues":[]}"#).is_err(), "no version: not a cache");
        assert!(IssueCache::read("garbage").is_err());
    }
}
