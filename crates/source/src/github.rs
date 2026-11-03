//! A repository's GitHub issues (#17): its `origin` remote read as `owner/name` on GitHub, the
//! issues and pull requests fetched through `gh api` (paginated, with a deadline and a cap, as
//! every git call here), and kept in a cache per repository under the user's cache directory,
//! versioned JSON (`swarmception_model::IssueCache`). No `gh`, no network, no GitHub remote: each
//! is a [`GithubError`] to show, never a failure to read the tracking branch.
//!
//! [`refresh`] brings a cache up to date (#62): with no cache, one of another version, one whose
//! last full fetch is a week old ([`FULL_EVERY`]) or one with no change times, every page of the
//! issues and, beside them, of the pulls (their head branches); else only what changed since the
//! newest change it holds (`since=`, a little before it: [`OVERLAP`]), one page, sent with the
//! validator GitHub gave the same request last time, so a `304 Not Modified` says nothing changed
//! for free. More than a page changed: every page after all. The pull requests that changed get
//! their branches from one page of the pulls, newest first.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use swarmception_model::issues::{MAX_FIELD, Parsed};
use swarmception_model::{IssueCache, IssueKind, Timestamp, Validator, parse_issues, parse_pulls};

use crate::git::{Git, GitError};

/// How long `gh api` may take, every page together.
pub const GH_DEADLINE: Duration = Duration::from_secs(90);
/// The most `gh api` may print.
pub const GH_CAP: usize = 512 * 1024 * 1024;
/// The largest cache file read.
pub const MAX_CACHE: u64 = 256 * 1024 * 1024;
/// A cache last fetched in full this long ago (in seconds) is fetched in full again: an issue
/// deleted or moved to another repository leaves the cache only then.
pub const FULL_EVERY: i64 = 7 * 24 * 60 * 60;
/// How long before the newest change it holds an incremental fetch asks from (in seconds): a
/// list GitHub serves may lag a write by a moment.
pub const OVERLAP: i64 = 60;
/// The most an incremental fetch asks for, one page; more changed means a full fetch.
pub const PAGE: usize = 100;

/// A repository on GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Slug {
    pub owner: String,
    pub name: String,
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

impl Slug {
    /// `owner/name`, each part letters, digits, `-`, `_` and `.` (not only dots), as GitHub
    /// allows them.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (owner, name) = text.split_once('/')?;
        let ok = |part: &str| {
            !part.is_empty()
                && part.len() <= 100
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
                && part.bytes().any(|b| b != b'.')
        };
        (ok(owner) && ok(name)).then(|| Self { owner: owner.to_owned(), name: name.to_owned() })
    }

    /// The repository a remote URL names on github.com: `https://github.com/o/n(.git)`,
    /// `git@github.com:o/n.git`, `ssh://git@github.com/o/n.git`; none for another host.
    #[must_use]
    pub fn of_remote(url: &str) -> Option<Self> {
        let url = url.trim();
        let rest = ["https://", "http://", "ssh://", "git://"]
            .iter()
            .find_map(|scheme| url.strip_prefix(scheme))
            .map_or_else(|| url.split_once('@').map(|(_, r)| r.replacen(':', "/", 1)), |r| Some(r.rsplit_once('@').map_or(r, |(_, host)| host).to_owned()))?;
        let path = rest.strip_prefix("github.com/").or_else(|| rest.strip_prefix("www.github.com/"))?;
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        Self::parse(path)
    }

    /// An issue's page, `https://github.com/<owner>/<name>/issues/<n>`.
    #[must_use]
    pub fn issue_url(&self, number: u64) -> String {
        format!("https://github.com/{self}/issues/{number}")
    }

    /// A pull request's page.
    #[must_use]
    pub fn pull_url(&self, number: u64) -> String {
        format!("https://github.com/{self}/pull/{number}")
    }
}

/// Why the issues couldn't be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubError {
    /// The repository has no `origin` remote.
    NoRemote,
    /// Its `origin` isn't on GitHub.
    NotGithub { url: String },
    /// `gh` couldn't start: not installed, not on the path.
    NoGh { message: String },
    /// `gh` failed: not signed in, no network, no such repository; its own words.
    Gh { message: String },
    /// What `gh` printed didn't read as issues.
    Garbled { message: String },
    /// The cache couldn't be read or written.
    Cache { path: PathBuf, message: String },
    /// No cache directory: neither `HOME` nor the platform's own is set.
    NoCacheDir,
}

impl fmt::Display for GithubError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRemote => write!(f, "no issues: the repository has no origin remote"),
            Self::NotGithub { url } => write!(f, "no issues: origin ({url}) isn't on GitHub"),
            Self::NoGh { message } => write!(f, "no issues: can't run gh ({message})"),
            Self::Gh { message } => write!(f, "no issues: gh failed: {message}"),
            Self::Garbled { message } => write!(f, "no issues: {message}"),
            Self::Cache { path, message } => write!(f, "the issues' cache {}: {message}", path.display()),
            Self::NoCacheDir => write!(f, "the issues aren't cached: no cache directory (HOME isn't set)"),
        }
    }
}

impl std::error::Error for GithubError {}

/// Where the issues come from: the `gh` to run, and the directory the caches are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Github {
    pub gh: PathBuf,
    /// None: nothing is cached.
    pub cache_dir: Option<PathBuf>,
}

impl Default for Github {
    /// `gh` on the path, the user's cache directory.
    fn default() -> Self {
        Self { gh: PathBuf::from("gh"), cache_dir: default_cache_dir() }
    }
}

/// The user's cache directory for Swarmception: `~/Library/Caches/swarmception` on macOS,
/// `%LOCALAPPDATA%\swarmception` on Windows, `$XDG_CACHE_HOME/swarmception` (else
/// `~/.cache/swarmception`) elsewhere; none when the variables it needs aren't set.
#[must_use]
pub fn default_cache_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Caches"))
    } else if cfg!(windows) {
        var("LOCALAPPDATA")
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("swarmception"))
}

/// The cache file of `slug`'s issues under `dir`.
#[must_use]
pub fn cache_path(dir: &Path, slug: &Slug) -> PathBuf {
    dir.join("issues").join(format!("{}--{}.json", slug.owner, slug.name))
}

/// The GitHub repository `repo`'s `origin` remote names.
///
/// # Errors
///
/// [`GithubError::NoRemote`], or [`GithubError::NotGithub`] for a remote elsewhere.
pub fn origin(repo: &Path) -> Result<Slug, GithubError> {
    let git = Git::new(repo, crate::READ_DEADLINE, 64 * 1024);
    let url = git.run(&["remote", "get-url", "origin"], None).map_err(|_| GithubError::NoRemote)?;
    let url = String::from_utf8_lossy(&url).trim().to_owned();
    Slug::of_remote(&url).ok_or(GithubError::NotGithub { url })
}

impl From<GitError> for GithubError {
    fn from(e: GitError) -> Self {
        match e {
            GitError::Spawn(message) => Self::NoGh { message },
            GitError::Failed { stderr, status, .. } => Self::Gh { message: if stderr.is_empty() { status } else { stderr } },
            GitError::Deadline { seconds, .. } => Self::Gh { message: format!("took over {seconds} s and was stopped") },
            GitError::TooLarge { cap, .. } => Self::Gh { message: format!("said over {cap} bytes") },
            GitError::Garbled(message) => Self::Garbled { message },
        }
    }
}

/// `gh api <args>`, run in `repo`: what it printed.
fn gh_api(gh: &Path, repo: &Path, args: &[&str]) -> Result<Vec<u8>, GitError> {
    let args: Vec<&str> = std::iter::once("api").chain(args.iter().copied()).collect();
    Git::program(gh, repo, GH_DEADLINE, GH_CAP).run(&args, None)
}

/// Fetches `slug`'s issues and pull requests with `gh api --paginate`, run in `repo`.
///
/// # Errors
///
/// [`GithubError::NoGh`], [`GithubError::Gh`] (its stderr, or its deadline), or
/// [`GithubError::Garbled`].
pub fn fetch_issues(gh: &Path, repo: &Path, slug: &Slug) -> Result<Parsed, GithubError> {
    let out = gh_api(gh, repo, &["--paginate", &format!("repos/{slug}/issues?state=all&per_page=100")])?;
    parse_issues(&String::from_utf8_lossy(&out)).map_err(|e| GithubError::Garbled { message: e.to_string() })
}

/// The head branches of `slug`'s pull requests, by number: every page, or (`newest`) only that
/// many of the most recently changed, one page.
///
/// # Errors
///
/// As [`fetch_issues`].
pub fn fetch_branches(gh: &Path, repo: &Path, slug: &Slug, newest: Option<usize>) -> Result<std::collections::BTreeMap<u64, String>, GithubError> {
    let out = match newest {
        Some(n) => gh_api(gh, repo, &[&format!("repos/{slug}/pulls?state=all&sort=updated&direction=desc&per_page={}", n.clamp(1, PAGE))])?,
        None => gh_api(gh, repo, &["--paginate", &format!("repos/{slug}/pulls?state=all&per_page=100")])?,
    };
    parse_pulls(&String::from_utf8_lossy(&out)).map_err(|e| GithubError::Garbled { message: e.to_string() })
}

/// Every issue and pull request of `slug`, each pull request with its head branch: the issues'
/// pages and the pulls' fetched at once.
///
/// # Errors
///
/// As [`fetch_issues`], for either.
pub fn fetch_full(gh: &Path, repo: &Path, slug: &Slug) -> Result<Parsed, GithubError> {
    let (gh_pulls, repo_pulls, slug_pulls) = (gh.to_path_buf(), repo.to_path_buf(), slug.clone());
    let pulls = std::thread::spawn(move || fetch_branches(&gh_pulls, &repo_pulls, &slug_pulls, None));
    let issues = fetch_issues(gh, repo, slug);
    let branches = pulls.join().unwrap_or_else(|_| Err(GithubError::Gh { message: "the pulls' fetch stopped".into() }));
    let mut parsed = issues?;
    let branches = branches?;
    for issue in parsed.issues.iter_mut().filter(|i| i.kind == IssueKind::PullRequest) {
        issue.branch = branches.get(&issue.number).cloned();
    }
    Ok(parsed)
}

/// Why a refresh fetched every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FullWhy {
    /// There was no cache (or it didn't read).
    NoCache,
    /// The cache is another version's.
    Version(u32),
    /// Its last full fetch is [`FULL_EVERY`] old, or unknown.
    Stale,
    /// It holds no change times to ask from.
    NoTimes,
    /// More changed than one page holds.
    TooMany,
}

impl fmt::Display for FullWhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCache => write!(f, "no cache"),
            Self::Version(v) => write!(f, "the cache is version {v}"),
            Self::Stale => write!(f, "a week since the last full fetch"),
            Self::NoTimes => write!(f, "the cache has no change times"),
            Self::TooMany => write!(f, "over {PAGE} changed"),
        }
    }
}

/// How a refresh went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// Every page, and why.
    Full(FullWhy),
    /// What changed since the newest change held: how many came, and how many of them were new
    /// or different.
    Incremental { came: usize, changed: usize },
    /// GitHub said nothing changed (`304 Not Modified`).
    Unchanged,
}

/// Why `cache`, at `now`, is fetched in full: none when only what changed is asked for.
#[must_use]
pub fn full_why(cache: Option<&IssueCache>, now: Timestamp) -> Option<FullWhy> {
    let Some(cache) = cache else { return Some(FullWhy::NoCache) };
    if cache.version != swarmception_model::issues::ISSUES_VERSION {
        return Some(FullWhy::Version(cache.version));
    }
    if cache.full.is_none_or(|full| now.0.saturating_sub(full.0) >= FULL_EVERY) {
        return Some(FullWhy::Stale);
    }
    cache.newest().is_none().then_some(FullWhy::NoTimes)
}

/// What an incremental request found.
enum Since {
    Unchanged,
    Changed { parsed: Parsed, validator: Validator },
    TooMany,
}

/// A header value fit to send back: printable ASCII, not too long.
fn sendable(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= MAX_FIELD && value.bytes().all(|b| (0x20..0x7f).contains(&b))).then(|| value.to_owned())
}

/// An answer's headers: names lowercased, in order.
type Headers = Vec<(String, String)>;

/// What `gh api --include` printed: the status code, the headers and the body.
fn split_response(out: &str) -> Option<(u16, Headers, &str)> {
    let (status, mut rest) = out.split_once('\n')?;
    let code = status.split_whitespace().nth(1)?.parse().ok()?;
    let mut headers = Vec::new();
    loop {
        let (line, after) = rest.split_once('\n').unwrap_or((rest, ""));
        rest = after;
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
        if rest.is_empty() {
            break;
        }
    }
    Some((code, headers, rest))
}

/// Asks for `slug`'s issues and pull requests changed since `since`, one page, sending
/// `validator` when it is the same request's.
fn fetch_since(gh: &Path, repo: &Path, slug: &Slug, since: Timestamp, validator: Option<&Validator>) -> Result<Since, GithubError> {
    let endpoint = format!("repos/{slug}/issues?state=all&sort=updated&direction=asc&per_page={PAGE}&since={since}");
    let validator = validator.filter(|v| v.since == Some(since));
    let header = validator.and_then(|v| {
        v.etag
            .as_deref()
            .and_then(sendable)
            .map(|e| format!("If-None-Match: {e}"))
            .or_else(|| v.last_modified.as_deref().and_then(sendable).map(|m| format!("If-Modified-Since: {m}")))
    });
    let mut args = vec!["--include"];
    if let Some(header) = &header {
        args.extend(["--header", header]);
    }
    args.push(&endpoint);
    let out = match gh_api(gh, repo, &args) {
        Err(GitError::Failed { stderr, .. }) if header.is_some() && stderr.contains("HTTP 304") => return Ok(Since::Unchanged),
        other => other?,
    };
    let out = String::from_utf8_lossy(&out);
    let (code, headers, body) = split_response(&out).ok_or_else(|| GithubError::Garbled { message: "gh api's answer had no status line".into() })?;
    if !(200..300).contains(&code) {
        return Err(GithubError::Gh { message: format!("HTTP {code}") });
    }
    let header = |name: &str| headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
    if header("link").is_some_and(|link| link.contains("rel=\"next\"")) {
        return Ok(Since::TooMany);
    }
    let parsed = parse_issues(body).map_err(|e| GithubError::Garbled { message: e.to_string() })?;
    let validator = Validator { since: Some(since), etag: header("etag").and_then(sendable), last_modified: header("last-modified").and_then(sendable) };
    Ok(Since::Changed { parsed, validator })
}

/// Brings `cached` (none: no cache) up to date at `now`: every page when [`full_why`] says so,
/// else what changed since, merged in (module docs). The cache to keep, and how it went.
///
/// # Errors
///
/// As [`fetch_issues`]; `cached` is then as it was, for the caller to keep.
pub fn refresh(gh: &Path, repo: &Path, slug: &Slug, cached: Option<IssueCache>, now: Timestamp) -> Result<(IssueCache, Refresh), GithubError> {
    let full = |why| fetch_full(gh, repo, slug).map(|parsed| (IssueCache::new(&slug.to_string(), Some(now), parsed.issues), Refresh::Full(why)));
    if let Some(why) = full_why(cached.as_ref(), now) {
        return full(why);
    }
    let Some(mut cache) = cached else { return full(FullWhy::NoCache) };
    let Some(newest) = cache.newest() else { return full(FullWhy::NoTimes) };
    let since = Timestamp(newest.0.saturating_sub(OVERLAP));
    match fetch_since(gh, repo, slug, since, cache.validator.as_ref())? {
        Since::Unchanged => {
            cache.fetched = Some(now);
            Ok((cache, Refresh::Unchanged))
        }
        Since::TooMany => full(FullWhy::TooMany),
        Since::Changed { mut parsed, validator } => {
            // A branch never changes: only a pull request new to the cache needs its own, among
            // the most recently changed of the pulls (these that changed, and a few more).
            let pulls = parsed.issues.iter().filter(|i| i.kind == IssueKind::PullRequest).count();
            let known = |number: u64| cache.issues.iter().any(|c| c.number == number && c.kind == IssueKind::PullRequest && c.branch.is_some());
            if parsed.issues.iter().any(|i| i.kind == IssueKind::PullRequest && !known(i.number)) {
                let branches = fetch_branches(gh, repo, slug, Some((pulls + 10).min(PAGE)))?;
                for issue in parsed.issues.iter_mut().filter(|i| i.kind == IssueKind::PullRequest) {
                    issue.branch = branches.get(&issue.number).cloned();
                }
            }
            let came = parsed.issues.len();
            let changed = cache.merge(now, parsed.issues);
            cache.validator = Some(validator);
            Ok((cache, Refresh::Incremental { came, changed }))
        }
    }
}

/// The cached issues of `slug` under `dir`: none when there is no cache yet, or it is another
/// repository's.
///
/// # Errors
///
/// [`GithubError::Cache`] for a cache that can't be read or doesn't read as one.
pub fn load_cache(dir: &Path, slug: &Slug) -> Result<Option<IssueCache>, GithubError> {
    let path = cache_path(dir, slug);
    let failed = |message: String| GithubError::Cache { path: path.clone(), message };
    match std::fs::metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed(e.to_string())),
        Ok(meta) if meta.len() > MAX_CACHE => return Err(failed(format!("over {MAX_CACHE} bytes"))),
        Ok(_) => {}
    }
    let text = std::fs::read_to_string(&path).map_err(|e| failed(e.to_string()))?;
    let cache = IssueCache::read(&text).map_err(|e| failed(e.to_string()))?;
    Ok((cache.repo == slug.to_string()).then_some(cache))
}

/// Writes `cache` under `dir`, whole or not at all (a temporary file renamed over the old).
///
/// # Errors
///
/// [`GithubError::Cache`] when the directory or the file can't be written.
pub fn save_cache(dir: &Path, slug: &Slug, cache: &IssueCache) -> Result<PathBuf, GithubError> {
    let path = cache_path(dir, slug);
    let failed = |e: std::io::Error| GithubError::Cache { path: path.clone(), message: e.to_string() };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&temporary, cache.write()).map_err(failed)?;
    std::fs::rename(&temporary, &path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        failed(e)
    })?;
    Ok(path)
}

/// Now, by the system clock.
#[must_use]
pub fn now() -> Timestamp {
    let seconds = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Timestamp(i64::try_from(seconds).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(what: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("swarmception-github-{what}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn remotes_on_github_read_as_their_slug() {
        let slug = |url: &str| Slug::of_remote(url).map(|s| s.to_string());
        assert_eq!(slug("https://github.com/mornycorp/mc-engine.git").as_deref(), Some("mornycorp/mc-engine"));
        assert_eq!(slug("https://github.com/mornycorp/mc-engine").as_deref(), Some("mornycorp/mc-engine"));
        assert_eq!(slug("https://user:token@github.com/o/n.git/").as_deref(), Some("o/n"));
        assert_eq!(slug("git@github.com:o/n.git").as_deref(), Some("o/n"));
        assert_eq!(slug("ssh://git@github.com/o/my.repo.git").as_deref(), Some("o/my.repo"));
        assert_eq!(slug("https://gitlab.com/o/n.git"), None);
        assert_eq!(slug("git@gitlab.com:o/n.git"), None);
        assert_eq!(slug("/some/local/path"), None);
        assert_eq!(slug("https://github.com/o"), None);
        assert_eq!(slug("https://github.com/o/n/extra"), None);
        assert_eq!(slug("https://github.com/o/$(rm -rf)"), None, "nothing odd reaches gh's command line");
        assert_eq!(slug("https://github.com/../.."), None);
    }

    #[test]
    fn urls() {
        let slug = Slug::parse("o/n").unwrap();
        assert_eq!(slug.issue_url(17), "https://github.com/o/n/issues/17");
        assert_eq!(slug.pull_url(26), "https://github.com/o/n/pull/26");
    }

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").current_dir(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn origin_reads_the_remote_or_says_why_not() {
        let dir = temp("origin");
        git(&dir, &["init", "-q"]);
        assert_eq!(origin(&dir), Err(GithubError::NoRemote));
        git(&dir, &["remote", "add", "origin", "https://gitlab.com/o/n.git"]);
        assert!(matches!(origin(&dir), Err(GithubError::NotGithub { .. })));
        git(&dir, &["remote", "set-url", "origin", "git@github.com:o/n.git"]);
        assert_eq!(origin(&dir).map(|s| s.to_string()), Ok("o/n".to_owned()));
        std::fs::remove_dir_all(dir).ok();
    }

    /// A `gh` run in `dir` that runs `script` (shell, no `#!` line) for each `gh api …`, gh's
    /// arguments in `$@`: the system's `sh`, which runs `<dir>/api`, the script (`api` its first
    /// argument, put back in `$@`). So no file a test writes is ever exec'd itself (#106, #113):
    /// macOS checks a new executable on its first exec, which took 10–34 s under load. `cargo
    /// xtask fakes` fails on a test that writes one to exec; the engine's tests have the same
    /// helper.
    #[cfg(unix)]
    fn sh_gh(dir: &Path, script: &str) -> PathBuf {
        std::fs::write(dir.join("api"), format!("set -- api \"$@\"\n{script}")).unwrap();
        PathBuf::from("/bin/sh")
    }

    /// A `gh` that prints `out` (or fails with `err`), and writes its arguments down.
    #[cfg(unix)]
    fn fake_gh(dir: &Path, out: &str, fail: Option<&str>) -> PathBuf {
        std::fs::write(dir.join("out.json"), out).unwrap();
        let script = match fail {
            Some(err) => format!("echo \"$@\" >> '{0}/args'\necho '{err}' >&2\nexit 1\n", dir.display()),
            None => format!("echo \"$@\" >> '{0}/args'\ncat '{0}/out.json'\n", dir.display()),
        };
        sh_gh(dir, &script)
    }

    #[cfg(unix)]
    #[test]
    fn gh_api_paginated_reads_issues_and_pull_requests() {
        let dir = temp("gh");
        let pages = r#"[{"number":3,"title":"An issue","state":"open","html_url":"https://github.com/o/n/issues/3"}]
[{"number":4,"title":"Its fix","state":"closed","body":"Fixes #3","pull_request":{"merged_at":"2026-11-01T00:00:00Z"}}]"#;
        let gh = fake_gh(&dir, pages, None);
        let slug = Slug::parse("o/n").unwrap();
        let parsed = fetch_issues(&gh, &dir, &slug).unwrap();
        assert_eq!(parsed.issues.iter().map(|i| i.number).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(parsed.issues[1].closes, [3]);
        assert_eq!(std::fs::read_to_string(dir.join("args")).unwrap().trim(), "api --paginate repos/o/n/issues?state=all&per_page=100");
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn gh_failing_or_missing_or_garbled_is_an_error() {
        let dir = temp("gh-fails");
        let slug = Slug::parse("o/n").unwrap();
        let gh = fake_gh(&dir, "", Some("HTTP 401: Bad credentials"));
        assert_eq!(fetch_issues(&gh, &dir, &slug), Err(GithubError::Gh { message: "HTTP 401: Bad credentials".into() }));
        assert!(matches!(fetch_issues(&dir.join("no-such-gh"), &dir, &slug), Err(GithubError::NoGh { .. })));
        let gh = fake_gh(&dir, r#"{"message":"Not Found"}"#, None);
        assert!(matches!(fetch_issues(&gh, &dir, &slug), Err(GithubError::Garbled { .. })));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_cache_on_disk_round_trips_per_repository() {
        let dir = temp("cache");
        let slug = Slug::parse("o/n").unwrap();
        assert_eq!(load_cache(&dir, &slug), Ok(None), "no cache yet");
        let issues = parse_issues(r#"[{"number":3,"title":"An issue","state":"open","labels":[{"name":"bug"}]}]"#).unwrap().issues;
        let cache = IssueCache::new("o/n", Some(Timestamp(1_793_500_000)), issues);
        let path = save_cache(&dir, &slug, &cache).unwrap();
        assert_eq!(path, dir.join("issues/o--n.json"));
        assert_eq!(load_cache(&dir, &slug), Ok(Some(cache.clone())));
        assert_eq!(load_cache(&dir, &Slug::parse("o/other").unwrap()), Ok(None), "another repository's is its own file");
        // A file there that names another repository isn't this one's.
        std::fs::write(cache_path(&dir, &Slug::parse("x/y").unwrap()), cache.write()).unwrap();
        assert_eq!(load_cache(&dir, &Slug::parse("x/y").unwrap()), Ok(None));
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(load_cache(&dir, &slug), Err(GithubError::Cache { .. })), "a corrupt cache says so");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_cache_dir_is_the_users() {
        if let Some(dir) = default_cache_dir() {
            assert!(dir.ends_with("swarmception"), "{}", dir.display());
        }
    }

    /// A `gh` answering as GitHub would: `issues.json` for the issues' pages, `pulls.json` for the
    /// pulls', `since.txt` (headers and a page, as `--include` prints them) for an incremental
    /// request, and a 304 for a request sent with a validator; each call's arguments written
    /// down, a line each.
    #[cfg(unix)]
    fn github_gh(dir: &Path, issues: &str, pulls: &str, since: &str) -> PathBuf {
        std::fs::write(dir.join("issues.json"), issues).unwrap();
        std::fs::write(dir.join("pulls.json"), pulls).unwrap();
        std::fs::write(dir.join("since.txt"), since).unwrap();
        let d = dir.display();
        sh_gh(
            dir,
            &format!(
                "echo \"$@\" >> '{d}/args'\ncase \"$*\" in\n  *If-None-Match*) echo 'gh: HTTP 304' >&2; exit 1 ;;\n  *since=*) cat '{d}/since.txt' ;;\n  *pulls*) cat '{d}/pulls.json' ;;\n  *) cat '{d}/issues.json' ;;\nesac\n"
            ),
        )
    }

    fn calls(dir: &Path) -> Vec<String> {
        let calls = std::fs::read_to_string(dir.join("args")).unwrap_or_default().lines().map(str::to_owned).collect();
        let _ = std::fs::remove_file(dir.join("args"));
        calls
    }

    const ISSUES: &str = r#"[{"number":3,"title":"An issue","state":"open","updated_at":"2026-11-01T10:00:00Z"},
        {"number":4,"title":"Its fix","state":"open","body":"no keywords","updated_at":"2026-11-01T11:00:00Z","pull_request":{"merged_at":null}}]"#;
    const PULLS: &str = r#"[{"number":4,"head":{"ref":"3-an-issue"}},{"number":6,"head":{"ref":"5-new"}}]"#;
    /// What changed since: #3 renamed, a new issue #5 and its PR #6.
    const SINCE: &str = "HTTP/2.0 200 OK\nCache-Control: private\r\nEtag: W/\"abc\"\r\nLink: <x?page=1>; rel=\"prev\"\r\n\r\n[\
        {\"number\":3,\"title\":\"An issue, renamed\",\"state\":\"open\",\"updated_at\":\"2026-11-02T09:00:00Z\"},\
        {\"number\":5,\"title\":\"New\",\"state\":\"open\",\"updated_at\":\"2026-11-02T09:30:00Z\"},\
        {\"number\":6,\"title\":\"New's fix\",\"state\":\"open\",\"updated_at\":\"2026-11-02T10:00:00Z\",\"pull_request\":{}}]";

    fn when(text: &str) -> Timestamp {
        Timestamp::parse(text).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_refresh_with_no_cache_fetches_every_page_and_the_branches() {
        let dir = temp("refresh-full");
        let gh = github_gh(&dir, ISSUES, PULLS, SINCE);
        let slug = Slug::parse("o/n").unwrap();
        let now = when("2026-11-02T12:00:00Z");
        let (cache, how) = refresh(&gh, &dir, &slug, None, now).unwrap();
        assert_eq!(how, Refresh::Full(FullWhy::NoCache));
        assert_eq!((cache.fetched, cache.full, cache.validator.as_ref()), (Some(now), Some(now), None));
        assert_eq!(cache.issues[1].branch.as_deref(), Some("3-an-issue"));
        let issues = swarmception_model::Issues::new(cache.issues.clone());
        assert_eq!(issues.pr_of(3, None).map(|p| p.number), Some(4), "linked by its branch: its body closes nothing");
        let mut made = calls(&dir);
        made.sort();
        assert_eq!(made, ["api --paginate repos/o/n/issues?state=all&per_page=100", "api --paginate repos/o/n/pulls?state=all&per_page=100"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn an_incremental_refresh_asks_since_the_newest_change_then_with_its_etag() {
        let dir = temp("refresh-since");
        let gh = github_gh(&dir, ISSUES, PULLS, SINCE);
        let slug = Slug::parse("o/n").unwrap();
        let (cache, _) = refresh(&gh, &dir, &slug, None, when("2026-11-02T08:00:00Z")).unwrap();
        let _ = calls(&dir);

        let (cache, how) = refresh(&gh, &dir, &slug, Some(cache), when("2026-11-02T12:00:00Z")).unwrap();
        assert_eq!(how, Refresh::Incremental { came: 3, changed: 3 });
        assert_eq!(
            calls(&dir),
            [
                "api --include repos/o/n/issues?state=all&sort=updated&direction=asc&per_page=100&since=2026-11-01T10:59:00Z",
                "api repos/o/n/pulls?state=all&sort=updated&direction=desc&per_page=11"
            ],
            "a minute before the newest change; the newest of the pulls for new #6's branch, a few more than changed"
        );
        assert_eq!(
            cache.issues.iter().map(|i| (i.number, i.title.as_str())).collect::<Vec<_>>(),
            [(3, "An issue, renamed"), (4, "Its fix"), (5, "New"), (6, "New's fix")]
        );
        assert_eq!((cache.issues[1].branch.as_deref(), cache.issues[3].branch.as_deref()), (Some("3-an-issue"), Some("5-new")));
        assert_eq!((cache.fetched, cache.full), (Some(when("2026-11-02T12:00:00Z")), Some(when("2026-11-02T08:00:00Z"))));
        let validator = cache.validator.clone().unwrap();
        assert_eq!((validator.since, validator.etag.as_deref()), (Some(when("2026-11-01T10:59:00Z")), Some("W/\"abc\"")));

        // The newest change moved, so the validator isn't this request's: asked again, nothing new.
        let (cache, how) = refresh(&gh, &dir, &slug, Some(cache), when("2026-11-02T12:01:00Z")).unwrap();
        assert_eq!(how, Refresh::Incremental { came: 3, changed: 0 });
        assert_eq!(
            calls(&dir),
            ["api --include repos/o/n/issues?state=all&sort=updated&direction=asc&per_page=100&since=2026-11-02T09:59:00Z"],
            "#6's branch known: no pulls asked for"
        );
        // The same request now: sent with its ETag, GitHub's 304.
        let (cache, how) = refresh(&gh, &dir, &slug, Some(cache), when("2026-11-02T12:02:00Z")).unwrap();
        assert_eq!(how, Refresh::Unchanged);
        assert_eq!(
            calls(&dir),
            ["api --include --header If-None-Match: W/\"abc\" repos/o/n/issues?state=all&sort=updated&direction=asc&per_page=100&since=2026-11-02T09:59:00Z"]
        );
        assert_eq!((cache.fetched, cache.issues.len()), (Some(when("2026-11-02T12:02:00Z")), 4));
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn more_than_a_page_changed_is_a_full_fetch() {
        let dir = temp("refresh-many");
        let gh = github_gh(&dir, ISSUES, PULLS, "HTTP/2.0 200 OK\nLink: <x?page=2>; rel=\"next\"\r\n\r\n[]");
        let slug = Slug::parse("o/n").unwrap();
        let (cache, _) = refresh(&gh, &dir, &slug, None, when("2026-11-02T08:00:00Z")).unwrap();
        let (_, how) = refresh(&gh, &dir, &slug, Some(cache), when("2026-11-02T09:00:00Z")).unwrap();
        assert_eq!(how, Refresh::Full(FullWhy::TooMany));
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn an_incremental_refresh_failing_is_an_error() {
        let dir = temp("refresh-fails");
        let slug = Slug::parse("o/n").unwrap();
        let gh = github_gh(&dir, ISSUES, PULLS, SINCE);
        let (cache, _) = refresh(&gh, &dir, &slug, None, when("2026-11-02T08:00:00Z")).unwrap();
        let gh = fake_gh(&dir, "", Some("HTTP 502: Bad Gateway"));
        assert_eq!(
            refresh(&gh, &dir, &slug, Some(cache.clone()), when("2026-11-02T09:00:00Z")),
            Err(GithubError::Gh { message: "HTTP 502: Bad Gateway".into() })
        );
        let gh = github_gh(&dir, ISSUES, PULLS, "HTTP/2.0 500 Internal\n\n{}");
        assert!(matches!(refresh(&gh, &dir, &slug, Some(cache.clone()), when("2026-11-02T09:00:00Z")), Err(GithubError::Gh { .. })));
        let gh = github_gh(&dir, ISSUES, PULLS, "garbage");
        assert!(matches!(refresh(&gh, &dir, &slug, Some(cache), when("2026-11-02T09:00:00Z")), Err(GithubError::Garbled { .. })));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn what_is_fetched_in_full() {
        let now = when("2026-11-02T12:00:00Z");
        let issue = swarmception_model::Issue { number: 1, updated: Some(when("2026-11-01T00:00:00Z")), ..Default::default() };
        let fresh = IssueCache::new("o/n", Some(when("2026-11-01T12:00:00Z")), vec![issue]);
        assert_eq!(full_why(Some(&fresh), now), None);
        assert_eq!(full_why(None, now), Some(FullWhy::NoCache));
        assert_eq!(full_why(Some(&IssueCache { version: 1, ..fresh.clone() }), now), Some(FullWhy::Version(1)));
        assert_eq!(full_why(Some(&IssueCache { full: Some(when("2026-10-26T12:00:00Z")), ..fresh.clone() }), now), Some(FullWhy::Stale), "a week old");
        assert_eq!(full_why(Some(&IssueCache { full: Some(when("2026-10-26T12:00:01Z")), ..fresh.clone() }), now), None, "a second short of a week");
        assert_eq!(full_why(Some(&IssueCache { full: None, ..fresh.clone() }), now), Some(FullWhy::Stale));
        assert_eq!(full_why(Some(&IssueCache { issues: Vec::new(), ..fresh }), now), Some(FullWhy::NoTimes));
    }

    #[test]
    fn an_answer_with_headers_splits() {
        let (code, headers, body) =
            split_response("HTTP/2.0 200 OK\nEtag: W/\"x\"\r\nX-Odd\r\nLast-Modified: Mon, 02 Nov 2026 10:00:00 GMT\r\n\r\n[1]\n").unwrap();
        assert_eq!(code, 200);
        assert_eq!(headers, [("etag".to_owned(), "W/\"x\"".to_owned()), ("last-modified".to_owned(), "Mon, 02 Nov 2026 10:00:00 GMT".to_owned())]);
        assert_eq!(body, "[1]\n");
        assert_eq!(split_response("HTTP/2.0 304 Not Modified\nEtag: x\n").map(|(c, h, b)| (c, h.len(), b)), Some((304, 1, "")));
        assert!(split_response("[]").is_none());
        assert!(split_response("").is_none());
        assert_eq!(sendable("W/\"a\"").as_deref(), Some("W/\"a\""));
        assert_eq!(sendable("a\u{7f}b"), None);
        assert_eq!(sendable("  "), None);
    }
}
