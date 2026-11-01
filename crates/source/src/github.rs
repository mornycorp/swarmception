//! A repository's GitHub issues (#17): its `origin` remote read as `owner/name` on GitHub, the
//! issues and pull requests fetched through `gh api` (paginated, with a deadline and a cap, as
//! every git call here), and kept in a cache per repository under the user's cache directory,
//! versioned JSON (`swarmception_model::IssueCache`). No `gh`, no network, no GitHub remote: each
//! is a [`GithubError`] to show, never a failure to read the tracking branch.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use swarmception_model::issues::Parsed;
use swarmception_model::{IssueCache, Timestamp, parse_issues};

use crate::git::{Git, GitError};

/// How long `gh api` may take, every page together.
pub const GH_DEADLINE: Duration = Duration::from_secs(90);
/// The most `gh api` may print.
pub const GH_CAP: usize = 512 * 1024 * 1024;
/// The largest cache file read.
pub const MAX_CACHE: u64 = 256 * 1024 * 1024;

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

/// Fetches `slug`'s issues and pull requests with `gh api --paginate`, run in `repo`.
///
/// # Errors
///
/// [`GithubError::NoGh`], [`GithubError::Gh`] (its stderr, or its deadline), or
/// [`GithubError::Garbled`].
pub fn fetch_issues(gh: &Path, repo: &Path, slug: &Slug) -> Result<Parsed, GithubError> {
    let endpoint = format!("repos/{slug}/issues?state=all&per_page=100");
    let runner = Git::program(gh, repo, GH_DEADLINE, GH_CAP);
    let out = runner.run(&["api", "--paginate", &endpoint], None).map_err(|e| match e {
        GitError::Spawn(message) => GithubError::NoGh { message },
        GitError::Failed { stderr, status, .. } => GithubError::Gh { message: if stderr.is_empty() { status } else { stderr } },
        GitError::Deadline { seconds, .. } => GithubError::Gh { message: format!("took over {seconds} s and was stopped") },
        GitError::TooLarge { cap, .. } => GithubError::Gh { message: format!("said over {cap} bytes") },
        GitError::Garbled(message) => GithubError::Garbled { message },
    })?;
    parse_issues(&String::from_utf8_lossy(&out)).map_err(|e| GithubError::Garbled { message: e.to_string() })
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

    /// A `gh` that prints `out` (or fails with `err`), and writes its arguments down.
    #[cfg(unix)]
    fn fake_gh(dir: &Path, out: &str, fail: Option<&str>) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(dir.join("out.json"), out).unwrap();
        let body = match fail {
            Some(err) => format!("#!/bin/sh\necho \"$@\" > '{0}/args'\necho '{err}' >&2\nexit 1\n", dir.display()),
            None => format!("#!/bin/sh\necho \"$@\" > '{0}/args'\ncat '{0}/out.json'\n", dir.display()),
        };
        let gh = dir.join("gh");
        std::fs::write(&gh, body).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        gh
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
}
