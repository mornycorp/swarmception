//! A repository's GitHub issues and pull requests (#17): what the tracking branch doesn't say about
//! a ticket (its title, state, labels, author, times, page and pull request). Parsed from hostile
//! JSON: what `gh api --paginate repos/<owner>/<name>/issues?state=all` prints, the REST API's
//! issues, which lists the pull requests too (flagged by `pull_request`), one page's array after
//! another. An item that doesn't read is left out and counted; text is capped. An issue's pull
//! request is the newest whose body closes it (`Closes #17`, `fixes #17`, …) or whose head branch
//! is the issue's (`17-…`, as swarm-core's `claim` names them; [`branch_issue`]), the branches
//! read from the pulls endpoint ([`parse_pulls`]).
//!
//! [`IssueCache`] is how they are kept on disk per repository: versioned JSON
//! ([`ISSUES_VERSION`]); a cache from an older or newer build still reads, keeping every issue
//! that reads. It records when it was fetched, when it was last fetched in full, and the
//! validator (`ETag`, `Last-Modified`) of its last incremental request, and takes in what changed
//! since ([`IssueCache::merge`], #62).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Timestamp;

/// [`IssueCache`]'s shape. 1: the first. 2: each one's `updated` and a pull request's `branch`;
/// the cache's `full` and `validator` (#62).
pub const ISSUES_VERSION: u32 = 2;
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
    /// When it last changed on GitHub: what an incremental fetch asks from (`since`).
    pub updated: Option<Timestamp>,
    /// Its page.
    pub url: String,
    /// A pull request's: the issues its body closes, in order.
    pub closes: Vec<u64>,
    /// A pull request's: its head branch, from the pulls endpoint.
    pub branch: Option<String>,
    /// An issue's: its pull request, the newest that closes it or is on its branch.
    pub pr: Option<PrRef>,
}

impl Issue {
    /// The issues this pull request is for: those its body closes, and its branch's.
    fn targets(&self) -> impl Iterator<Item = u64> + '_ {
        self.closes.iter().copied().chain(self.branch.as_deref().and_then(branch_issue))
    }
}

/// The issue a branch is for, as swarm-core's `claim` names them: `<n>-<slug>`.
#[must_use]
pub fn branch_issue(branch: &str) -> Option<u64> {
    let (number, rest) = branch.split_once('-')?;
    let digits = !number.is_empty() && number.len() <= 19 && number.bytes().all(|b| b.is_ascii_digit());
    if !digits || rest.is_empty() {
        return None;
    }
    number.parse().ok().filter(|n| *n > 0)
}

/// The issues and pull requests, by number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Issues {
    by_number: BTreeMap<u64, Issue>,
}

impl Issues {
    /// `issues` by number (the last of a number kept), each issue's pull request linked: the
    /// newest pull request whose body closes it or whose head branch is the issue's.
    #[must_use]
    pub fn new(issues: Vec<Issue>) -> Self {
        let mut by_number: BTreeMap<u64, Issue> = issues.into_iter().take(MAX_ISSUES).map(|i| (i.number, Issue { pr: None, ..i })).collect();
        let mut links: BTreeMap<u64, PrRef> = BTreeMap::new();
        for pr in by_number.values().filter(|i| i.kind == IssueKind::PullRequest) {
            for target in pr.targets() {
                links.insert(target, PrRef { number: pr.number, title: pr.title.clone(), state: pr.state, url: pr.url.clone() });
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
    updated_at: Option<String>,
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
            updated: self.updated_at.as_deref().and_then(Timestamp::parse),
            url: cut(self.html_url.as_deref().unwrap_or_default(), MAX_FIELD),
            closes,
            branch: None,
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

/// A pull request of the pulls endpoint as it comes: its number and head branch.
#[derive(Deserialize)]
struct RawPullHead {
    number: Option<u64>,
    head: Option<RawHead>,
}

#[derive(Deserialize)]
struct RawHead {
    #[serde(rename = "ref")]
    branch: Option<String>,
}

/// Reads what `gh api [--paginate] repos/<owner>/<name>/pulls?state=all` printed: each pull
/// request's head branch, by number. Items that don't read are left out.
///
/// # Errors
///
/// [`IssuesError`] when the text isn't JSON arrays at all.
pub fn parse_pulls(text: &str) -> Result<BTreeMap<u64, String>, IssuesError> {
    let mut branches = BTreeMap::new();
    for page in serde_json::Deserializer::from_str(text).into_iter::<Vec<serde_json::Value>>() {
        let page = page.map_err(|e| IssuesError(e.to_string()))?;
        for item in page {
            if branches.len() >= MAX_ISSUES {
                break;
            }
            let Ok(RawPullHead { number: Some(number), head: Some(RawHead { branch: Some(branch) }) }) = serde_json::from_value(item) else {
                continue;
            };
            if number > 0 && !branch.is_empty() {
                branches.insert(number, cut(&branch, MAX_FIELD));
            }
        }
    }
    Ok(branches)
}

/// The issues `body` closes, as GitHub reads its keywords: `close`, `closes`, `closed`, `fix`,
/// `fixes`, `fixed`, `resolve`, `resolves`, `resolved`, then `#<n>` (a colon after the keyword
/// allowed), including comma/and-separated local references on the same line, in order, each once.
#[must_use]
pub fn closing_refs(body: &str) -> Vec<u64> {
    const KEYWORDS: [&str; 9] = ["close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved"];
    let mut out = Vec::new();
    let mut previous = None;
    for (at, _) in body.match_indices('#') {
        let continuation = previous
            .take()
            .and_then(|end| body.get(end..at))
            .is_some_and(|between| !between.contains(['\n', '\r']) && matches!(between.trim(), "," | "and" | ", and"));
        let digits: String = body.get(at + 1..).unwrap_or_default().chars().take_while(char::is_ascii_digit).take(21).collect();
        let Ok(number) = digits.parse::<u64>() else { continue };
        let raw = body.get(..at).unwrap_or_default();
        if !continuation && !raw.ends_with(|c: char| c.is_whitespace() || c == ':') {
            continue;
        }
        let before = raw.trim_end();
        let before = before.strip_suffix(':').unwrap_or(before).trim_end();
        let word: String = before.chars().rev().take_while(char::is_ascii_alphabetic).collect::<Vec<_>>().into_iter().rev().collect();
        let preceded = before.len() == word.len() || before.get(..before.len() - word.len()).is_some_and(|b| !b.ends_with(|c: char| c.is_alphanumeric()));
        if (continuation || (preceded && KEYWORDS.contains(&word.to_ascii_lowercase().as_str()))) && number > 0 {
            previous = Some(at + 1 + digits.len());
            if !out.contains(&number) && out.len() < MAX_CLOSES {
                out.push(number);
            }
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
    /// When they were last fetched in full (every page), not only what changed.
    #[serde(default)]
    pub full: Option<Timestamp>,
    /// The last incremental request's validator, to ask GitHub whether anything changed since.
    #[serde(default)]
    pub validator: Option<Validator>,
    pub issues: Vec<Issue>,
}

/// What GitHub said of an incremental request (`since`), to send back with the same request
/// (`If-None-Match`, `If-Modified-Since`): a `304 Not Modified` then means nothing changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Validator {
    /// The request's `since`: the validator is good for that request only.
    pub since: Option<Timestamp>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
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
    full: Option<Timestamp>,
    #[serde(default)]
    validator: Option<serde_json::Value>,
    #[serde(default)]
    issues: Vec<serde_json::Value>,
}

impl IssueCache {
    /// A cache of `issues`, `repo`'s, fetched in full at `fetched`.
    #[must_use]
    pub fn new(repo: &str, fetched: Option<Timestamp>, issues: Vec<Issue>) -> Self {
        let issues = issues.into_iter().take(MAX_ISSUES).map(|i| Issue { pr: None, ..i }).collect();
        Self { version: ISSUES_VERSION, repo: repo.to_owned(), fetched, full: fetched, validator: None, issues }
    }

    /// When the newest of them last changed on GitHub.
    #[must_use]
    pub fn newest(&self) -> Option<Timestamp> {
        self.issues.iter().filter_map(|i| i.updated).max()
    }

    /// Takes in what changed, fetched at `fetched`: each one replaces the one of its number (a
    /// pull request keeping the branch it had when the change has none), a new one is added;
    /// the rest stay. How many were new or different.
    pub fn merge(&mut self, fetched: Timestamp, changed: Vec<Issue>) -> usize {
        let mut by_number: BTreeMap<u64, Issue> = std::mem::take(&mut self.issues).into_iter().map(|i| (i.number, i)).collect();
        let mut moved = 0;
        for issue in changed {
            let old = by_number.get(&issue.number);
            let branch = issue.branch.clone().or_else(|| old.filter(|o| o.kind == issue.kind).and_then(|o| o.branch.clone()));
            let issue = Issue { branch, pr: None, ..issue };
            if old != Some(&issue) && (old.is_some() || by_number.len() < MAX_ISSUES) {
                moved += 1;
                by_number.insert(issue.number, issue);
            }
        }
        self.issues = by_number.into_values().collect();
        self.fetched = Some(fetched);
        moved
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
        let validator = raw.validator.and_then(|v| serde_json::from_value::<Validator>(v).ok());
        Ok(Self { version: raw.version, repo: raw.repo, fetched: raw.fetched, full: raw.full, validator, issues })
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

    #[test]
    fn a_first_version_cache_reads_with_nothing_of_the_second() {
        let v1 = r#"{"version":1,"repo":"o/n","fetched":5,"issues":[{"number":3,"kind":"pullRequest","title":"t","closes":[2]}]}"#;
        let cache = IssueCache::read(v1).unwrap();
        assert_eq!((cache.version, cache.full, cache.validator.as_ref()), (1, None, None));
        assert_eq!((cache.issues[0].updated, cache.issues[0].branch.as_deref()), (None, None));
        assert_eq!(cache.newest(), None);
        let odd = IssueCache::read(r#"{"version":2,"validator":"not one","issues":[]}"#).unwrap();
        assert_eq!(odd.validator, None, "a validator that doesn't read is none");
    }

    #[test]
    fn branches_name_their_issue() {
        assert_eq!(branch_issue("62-issues-fetched-incrementally"), Some(62));
        assert_eq!(branch_issue("7-x"), Some(7));
        assert_eq!(branch_issue("progress-5"), None);
        assert_eq!(branch_issue("62"), None, "a claim's branch has its title after the number");
        assert_eq!(branch_issue("62-"), None);
        assert_eq!(branch_issue("0-zero"), None);
        assert_eq!(branch_issue("-3-x"), None);
        assert_eq!(branch_issue("99999999999999999999-big"), None);
        assert_eq!(branch_issue("feature/62-x"), None);
    }

    #[test]
    fn the_pulls_endpoint_gives_each_head_branch() {
        let pages = r#"[{"number":26,"head":{"ref":"17-issue-titles","sha":"abc"},"body":"x"},{"number":27,"head":{}},{"head":{"ref":"x"}},"odd"]
            [{"number":30,"head":{"ref":"18-later"}},{"number":31,"head":{"ref":""}}]"#;
        let branches = parse_pulls(pages).unwrap();
        assert_eq!(branches.into_iter().collect::<Vec<_>>(), [(26, "17-issue-titles".to_owned()), (30, "18-later".to_owned())]);
        assert!(parse_pulls(r#"{"message":"Not Found"}"#).is_err());
        assert!(parse_pulls("").unwrap().is_empty());
    }

    fn pull(number: u64, branch: Option<&str>, closes: &[u64]) -> Issue {
        Issue {
            number,
            kind: IssueKind::PullRequest,
            title: format!("PR {number}"),
            branch: branch.map(str::to_owned),
            closes: closes.to_vec(),
            ..Issue::default()
        }
    }

    fn issue(number: u64) -> Issue {
        Issue { number, title: format!("issue {number}"), ..Issue::default() }
    }

    #[test]
    fn an_issues_pr_is_on_its_branch_too() {
        let issues = Issues::new(vec![
            issue(5),
            issue(6),
            issue(7),
            pull(10, Some("5-the-fix"), &[]),
            pull(11, Some("6-first"), &[]),
            pull(12, None, &[6]),
            pull(13, Some("7-x"), &[99]),
        ]);
        assert_eq!(issues.pr_of(5, None).map(|p| p.number), Some(10), "by its branch alone");
        assert_eq!(issues.pr_of(6, None).map(|p| p.number), Some(12), "the newest of its branch's and the one closing it");
        assert_eq!(issues.pr_of(7, None).map(|p| p.number), Some(13), "a branch's, though its body closes another");
        let stale = Issue { pr: Some(PrRef { number: 1, title: String::new(), state: IssueState::Open, url: String::new() }), ..issue(8) };
        assert_eq!(Issues::new(vec![stale]).pr_of(8, None), None, "a link that was is gone");
    }

    #[test]
    fn a_merge_takes_in_what_changed_and_keeps_the_rest() {
        let at = |s: i64| Some(Timestamp(s));
        let mut cache = IssueCache::new(
            "o/n",
            at(100),
            vec![Issue { updated: at(50), ..issue(1) }, Issue { updated: at(60), ..pull(2, Some("1-fix"), &[]) }, Issue { updated: at(40), ..issue(3) }],
        );
        assert_eq!((cache.full, cache.newest()), (at(100), at(60)));
        let changed = vec![
            Issue { updated: at(60), ..pull(2, None, &[]) },
            Issue { title: "renamed".into(), state: IssueState::Closed, updated: at(70), ..issue(3) },
            Issue { updated: at(80), ..pull(4, Some("3-other"), &[]) },
        ];
        assert_eq!(cache.merge(Timestamp(200), changed), 2, "#3 changed and #4 is new; #2 is as it was");
        assert_eq!((cache.fetched, cache.full, cache.newest()), (at(200), at(100), at(80)), "fetched now; in full still at the first");
        assert_eq!(cache.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(cache.issues[1].branch.as_deref(), Some("1-fix"), "a pull request keeps its branch");
        assert_eq!((cache.issues[2].title.as_str(), cache.issues[2].state), ("renamed", IssueState::Closed));
        let issues = Issues::new(cache.issues.clone());
        assert_eq!((issues.pr_of(1, None).map(|p| p.number), issues.pr_of(3, None).map(|p| p.number)), (Some(2), Some(4)));
        assert_eq!(cache.merge(Timestamp(300), Vec::new()), 0);
        assert_eq!(cache.issues.len(), 4);
    }
}
