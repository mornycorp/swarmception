//! Where the events come from (docs/architecture.md › Layers, L2): a repository's `tracking`
//! branch, read through `git` (Swarm Core's docs/tracking-format.md › The branch), and the merge
//! commits on its `main`. The reader takes `refs/remotes/origin/tracking`, else a local
//! `refs/heads/tracking` (and `main` the same way, or `master`), and never writes to either;
//! `fetch` brings the remote's up to date. Every call has a deadline and capped output, and
//! nothing reaches git's command line unchecked: the paths it reads are the branch's own,
//! checked as instance ids. Beside the branch, the repository's GitHub issues ([`github`], #17):
//! its `origin` as `owner/name`, `gh api` with a deadline, and a versioned cache per repository.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

mod git;
pub mod github;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use swarmception_model::{AgentFile, MainMerge, is_instance_id, parse_file, parse_merge};

pub use git::{Git, GitError};
pub use github::{Github, GithubError, Slug};

/// The refs a reader takes, in order.
pub const REFS: [&str; 2] = ["refs/remotes/origin/tracking", "refs/heads/tracking"];
/// The refs whose merge commits are the merges onto main, in order: the first there is read.
pub const MAIN_REFS: [&str; 4] = ["refs/remotes/origin/main", "refs/heads/main", "refs/remotes/origin/master", "refs/heads/master"];
/// The most merges onto main read, newest first.
pub const MAX_MERGES: usize = 100_000;
/// How long a read may take.
pub const READ_DEADLINE: Duration = Duration::from_secs(20);
/// How long a fetch may take.
pub const FETCH_DEADLINE: Duration = Duration::from_secs(60);
/// The most a read takes in from git: the branch's files together.
pub const MAX_BYTES: usize = 256 * 1024 * 1024;
/// The most agent files read.
pub const MAX_FILES: usize = 20_000;

/// Why a read failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    /// The path isn't a git repository.
    NotARepo { path: PathBuf, message: String },
    /// Neither tracking ref is there.
    NoTrackingBranch { path: PathBuf },
    /// Git failed, or ran past its deadline, or said more than the cap.
    Git(GitError),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotARepo { path, message } => write!(f, "{} is not a git repository: {message}", path.display()),
            Self::NoTrackingBranch { path } => write!(f, "{} has no tracking branch ({} or {})", path.display(), REFS[0], REFS[1]),
            Self::Git(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<GitError> for SourceError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

/// What a read found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tracking {
    /// The repository's top level.
    pub repo: PathBuf,
    /// The ref read, of [`REFS`].
    pub reference: String,
    /// Its commit.
    pub commit: String,
    /// Every agent's file, parsed.
    pub files: Vec<AgentFile>,
    /// Paths under `tracking/` that weren't an agent's file (skipped).
    pub skipped: Vec<String>,
    /// The main ref read, of [`MAIN_REFS`]: none when the repository has none of them.
    pub main: Option<String>,
    /// Its commit.
    pub main_commit: Option<String>,
    /// Its first-parent merge commits, oldest first.
    pub merges: Vec<MainMerge>,
}

/// The instance a branch path names, `tracking/<instance>/work.jsonl`.
#[must_use]
pub fn instance_of(path: &str) -> Option<&str> {
    let instance = path.strip_prefix("tracking/")?.strip_suffix("/work.jsonl")?;
    is_instance_id(instance).then_some(instance)
}

/// Reads `repo`'s tracking branch.
///
/// # Errors
///
/// [`SourceError`]: not a repository, no tracking branch, or git failing.
pub fn read(repo: &Path) -> Result<Tracking, SourceError> {
    let git = Git::new(repo, READ_DEADLINE, MAX_BYTES);
    let top = git.run(&["rev-parse", "--show-toplevel"], None).map_err(|e| SourceError::NotARepo { path: repo.to_path_buf(), message: e.to_string() })?;
    let top = PathBuf::from(String::from_utf8_lossy(&top).trim());
    let git = Git::new(&top, READ_DEADLINE, MAX_BYTES);
    let (reference, commit) = first_ref(&git, &REFS).ok_or_else(|| SourceError::NoTrackingBranch { path: top.clone() })?;
    let listing = git.run(&["ls-tree", "-r", "-z", "--name-only", &commit, "--", "tracking/"], None)?;
    let mut paths = Vec::new();
    let mut skipped = Vec::new();
    for path in listing.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = String::from_utf8_lossy(path).into_owned();
        match instance_of(&path) {
            Some(_) if paths.len() < MAX_FILES => paths.push(path),
            _ => skipped.push(path),
        }
    }
    let texts = git.cat_files(&commit, &paths)?;
    let files = paths.iter().zip(texts).filter_map(|(path, text)| Some(parse_file(instance_of(path)?, &text))).collect();
    let (main, main_commit, merges) = match first_ref(&git, &MAIN_REFS) {
        Some((main, head)) => {
            let merges = merges(&git, &head)?;
            (Some(main), Some(head), merges)
        }
        None => (None, None, Vec::new()),
    };
    Ok(Tracking { repo: top, reference, commit, files, skipped, main, main_commit, merges })
}

/// How long [`probe`] may take: a few `rev-parse`s, before a repository is opened (#69).
pub const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// What [`probe`] found: a git repository with a tracking branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// The repository's top level.
    pub top: PathBuf,
    /// The tracking ref there, of [`REFS`].
    pub reference: String,
}

/// Whether `repo` is a swarm's clone, before it is opened (#69): a git repository (its top
/// level) with a tracking branch. A few `rev-parse`s, each within [`PROBE_DEADLINE`]; nothing
/// is read or fetched.
///
/// # Errors
///
/// [`SourceError::NotARepo`] or [`SourceError::NoTrackingBranch`].
pub fn probe(repo: &Path) -> Result<Probe, SourceError> {
    let git = Git::new(repo, PROBE_DEADLINE, 64 * 1024);
    let top = git.run(&["rev-parse", "--show-toplevel"], None).map_err(|e| SourceError::NotARepo { path: repo.to_path_buf(), message: e.to_string() })?;
    let top = PathBuf::from(String::from_utf8_lossy(&top).trim());
    if top.as_os_str().is_empty() {
        // A bare repository has no top level: no clone to open.
        return Err(SourceError::NotARepo { path: repo.to_path_buf(), message: "a bare repository has no working tree".into() });
    }
    let git = Git::new(&top, PROBE_DEADLINE, 64 * 1024);
    let (reference, _) = first_ref(&git, &REFS).ok_or_else(|| SourceError::NoTrackingBranch { path: top.clone() })?;
    Ok(Probe { top, reference })
}

/// The commits a read would take now: the tracking ref's and main's, the first of [`REFS`] and
/// of [`MAIN_REFS`] there. A few `rev-parse`s, so a live watch (#54) reads the branch again only
/// when one of them moved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heads {
    pub tracking: Option<String>,
    pub main: Option<String>,
}

impl Heads {
    /// The commits `tracking` was read at.
    #[must_use]
    pub fn of(tracking: &Tracking) -> Self {
        Self { tracking: Some(tracking.commit.clone()), main: tracking.main_commit.clone() }
    }
}

/// `repo`'s [`Heads`] now.
///
/// # Errors
///
/// [`SourceError::NotARepo`] when git can't read the repository.
pub fn heads(repo: &Path) -> Result<Heads, SourceError> {
    let git = Git::new(repo, READ_DEADLINE, 64 * 1024);
    git.run(&["rev-parse", "--git-dir"], None).map_err(|e| SourceError::NotARepo { path: repo.to_path_buf(), message: e.to_string() })?;
    Ok(Heads { tracking: first_ref(&git, &REFS).map(|(_, commit)| commit), main: first_ref(&git, &MAIN_REFS).map(|(_, commit)| commit) })
}

/// The first of `refs` there, and its commit.
fn first_ref(git: &Git, refs: &[&str]) -> Option<(String, String)> {
    refs.iter().find_map(|r| {
        let spec = format!("{r}^{{commit}}");
        let out = git.run(&["rev-parse", "--verify", "--quiet", &spec], None).ok()?;
        Some(((*r).to_owned(), String::from_utf8_lossy(&out).trim().to_owned()))
    })
}

/// The first-parent merge commits of `head`, oldest first.
fn merges(git: &Git, head: &str) -> Result<Vec<MainMerge>, SourceError> {
    let count = format!("--max-count={MAX_MERGES}");
    let log = git.run(&["log", "--first-parent", "--merges", &count, "--format=%H%x1f%ct%x1f%s", head, "--"], None)?;
    let mut merges: Vec<MainMerge> = String::from_utf8_lossy(&log).lines().filter_map(parse_merge).collect();
    merges.reverse();
    Ok(merges)
}

/// Fetches the remote's tracking branch into `refs/remotes/origin/tracking` and its `main` into
/// `refs/remotes/origin/main`, in one fetch (one round trip: a live watch's, #54, makes one
/// every interval); a remote with no `main` has its tracking branch fetched alone.
///
/// # Errors
///
/// [`SourceError::Git`] when git fails or runs past its deadline fetching the tracking branch.
pub fn fetch(repo: &Path) -> Result<(), SourceError> {
    fetch_within(repo, FETCH_DEADLINE)
}

/// [`fetch`] with `deadline` for each fetch it makes.
///
/// # Errors
///
/// As [`fetch`].
pub fn fetch_within(repo: &Path, deadline: Duration) -> Result<(), SourceError> {
    let git = Git::new(repo, deadline, 1024 * 1024);
    let tracking = "+refs/heads/tracking:refs/remotes/origin/tracking";
    match git.run(&["fetch", "--quiet", "origin", tracking, "+refs/heads/main:refs/remotes/origin/main"], None) {
        Ok(_) => return Ok(()),
        // Past the deadline, a second try would only wait as long again.
        Err(error @ GitError::Deadline { .. }) => return Err(error.into()),
        Err(_) => {}
    }
    // A remote whose main is called something else still has its tracking branch fetched.
    git.run(&["fetch", "--quiet", "origin", tracking], None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_paths() {
        assert_eq!(instance_of("tracking/mini-8d44f3/work.jsonl"), Some("mini-8d44f3"));
        assert_eq!(instance_of("tracking/README.md"), None);
        assert_eq!(instance_of("tracking/a/b/work.jsonl"), None);
        assert_eq!(instance_of("README.md"), None);
    }

    /// A repository in a temporary directory with a tracking branch of two agents' files.
    fn repo() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "swarmception-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(dir.join("tracking/mini-1")).unwrap();
        std::fs::create_dir_all(dir.join("tracking/mini-2")).unwrap();
        std::fs::write(
            dir.join("tracking/mini-1/work.jsonl"),
            "{\"event\":\"session.start\",\"machine\":\"mini\",\"name\":\"w1\",\"role\":\"worker\",\"t\":\"2026-10-31T03:00:00Z\"}\n",
        )
        .unwrap();
        std::fs::write(dir.join("tracking/mini-2/work.jsonl"), "{\"event\":\"work\",\"issue\":3,\"t\":\"2026-10-31T03:01:00Z\"}\nbad\n").unwrap();
        std::fs::write(dir.join("tracking/README.md"), "the branch").unwrap();
        let run = |args: &[&str]| {
            let ok = std::process::Command::new("git").current_dir(&dir).args(args).output().unwrap();
            assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
        };
        let commit = |args: &[&str]| {
            let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
            all.extend_from_slice(args);
            run(&all);
        };
        run(&["init", "-q", "-b", "main"]);
        commit(&["commit", "-q", "--allow-empty", "-m", "start"]);
        run(&["switch", "-q", "-c", "topic"]);
        commit(&["commit", "-q", "--allow-empty", "-m", "a change"]);
        run(&["switch", "-q", "main"]);
        commit(&["merge", "-q", "--no-ff", "-m", "Merge pull request #7 from them/topic", "topic"]);
        run(&["switch", "-q", "--orphan", "tracking"]);
        run(&["add", "."]);
        commit(&["commit", "-q", "-m", "events"]);
        dir
    }

    #[test]
    fn reads_a_local_tracking_branch() {
        let dir = repo();
        let tracking = read(&dir).unwrap();
        assert_eq!(tracking.reference, "refs/heads/tracking");
        assert_eq!(tracking.files.len(), 2);
        assert_eq!(tracking.files[0].instance, "mini-1");
        assert_eq!(tracking.files[1].errors.len(), 1);
        assert_eq!(tracking.skipped, ["tracking/README.md"]);
        assert_eq!(tracking.main.as_deref(), Some("refs/heads/main"));
        assert_eq!(tracking.merges.iter().map(|m| m.pr).collect::<Vec<_>>(), [Some(7)], "the one merge commit, not its parents");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_directory_that_isnt_a_repo() {
        let dir = std::env::temp_dir().join(format!("swarmception-not-a-repo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(matches!(read(&dir), Err(SourceError::NotARepo { .. })));
        assert!(matches!(heads(&dir), Err(SourceError::NotARepo { .. })));
        assert!(matches!(probe(&dir), Err(SourceError::NotARepo { .. })));
        std::fs::remove_dir_all(dir).ok();
    }

    /// #69: a probe before opening says a clone's top level and tracking ref, or why it isn't one.
    #[test]
    fn a_probe_finds_the_top_level_and_the_tracking_branch() {
        let dir = repo();
        let found = probe(&dir.join("tracking/mini-1")).unwrap();
        assert_eq!((found.top, found.reference.as_str()), (std::fs::canonicalize(&dir).unwrap(), "refs/heads/tracking"));
        git(&dir, &["switch", "-q", "main"]);
        git(&dir, &["branch", "-q", "-D", "tracking"]);
        assert!(matches!(probe(&dir), Err(SourceError::NoTrackingBranch { .. })));
        std::fs::remove_dir_all(dir).ok();
    }

    fn git(dir: &Path, args: &[&str]) {
        let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
        all.extend_from_slice(args);
        let ok = std::process::Command::new("git").current_dir(dir).args(&all).output().unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    /// A clone of [`repo`] whose remote's tracking branch gains an event after the clone (#54).
    #[test]
    fn heads_move_only_when_a_fetch_brings_something() {
        let remote = repo();
        let clone = remote.with_extension("clone");
        git(&remote, &["clone", "-q", "--no-checkout", remote.to_str().unwrap(), clone.to_str().unwrap()]);
        git(&clone, &["fetch", "-q", "origin", "+refs/heads/tracking:refs/remotes/origin/tracking"]);
        let first = read(&clone).unwrap();
        assert_eq!(first.reference, "refs/remotes/origin/tracking");
        assert_eq!(heads(&clone).unwrap(), Heads::of(&first));
        assert!(first.main_commit.is_some(), "main's commit is read with its merges");

        fetch(&clone).unwrap();
        assert_eq!(heads(&clone).unwrap(), Heads::of(&first), "nothing new: nothing moved");

        // The remote's swarm logs an event.
        let file = remote.join("tracking/mini-1/work.jsonl");
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str("{\"event\":\"claim\",\"issue\":9,\"t\":\"2026-10-31T03:05:00Z\"}\n");
        std::fs::write(&file, text).unwrap();
        git(&remote, &["commit", "-q", "-am", "an event"]);
        assert_eq!(heads(&clone).unwrap(), Heads::of(&first), "not until a fetch");
        fetch(&clone).unwrap();
        let moved = heads(&clone).unwrap();
        assert_ne!(moved.tracking, Heads::of(&first).tracking);
        assert_eq!(moved.main, first.main_commit, "main stayed");
        let second = read(&clone).unwrap();
        assert_eq!(Heads::of(&second), moved);
        assert_eq!(second.files[0].events.len(), first.files[0].events.len() + 1);
        std::fs::remove_dir_all(remote).ok();
        std::fs::remove_dir_all(clone).ok();
    }

    #[test]
    fn a_remote_with_no_main_still_has_its_tracking_branch_fetched() {
        let remote = repo();
        git(&remote, &["branch", "-q", "-m", "main", "trunk"]);
        let clone = remote.with_extension("trunk");
        git(&remote, &["init", "-q", clone.to_str().unwrap()]);
        git(&clone, &["remote", "add", "origin", remote.to_str().unwrap()]);
        fetch(&clone).unwrap();
        let tracking = read(&clone).unwrap();
        assert_eq!((tracking.reference.as_str(), tracking.main.as_deref()), ("refs/remotes/origin/tracking", None));
        std::fs::remove_dir_all(remote).ok();
        std::fs::remove_dir_all(clone).ok();
    }

    #[test]
    fn a_fetch_with_no_remote_or_past_its_deadline_is_an_error() {
        let dir = repo();
        assert!(matches!(fetch(&dir), Err(SourceError::Git(GitError::Failed { .. }))), "no origin");
        // A remote whose upload-pack naps: the fetch is stopped at its deadline.
        let clone = dir.with_extension("slow");
        git(&dir, &["clone", "-q", "--no-checkout", dir.to_str().unwrap(), clone.to_str().unwrap()]);
        git(&clone, &["config", "remote.origin.uploadpack", "sleep 5; git-upload-pack"]);
        let started = std::time::Instant::now();
        let result = fetch_within(&clone, Duration::from_millis(300));
        assert!(matches!(result, Err(SourceError::Git(GitError::Deadline { .. }))), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(4));
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(clone).ok();
    }
}
