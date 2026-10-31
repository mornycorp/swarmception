//! Where the events come from (docs/architecture.md › Layers, L2): a repository's `tracking`
//! branch, read through `git` (Swarm Core's docs/tracking-format.md › The branch), and the merge
//! commits on its `main`. The reader takes `refs/remotes/origin/tracking`, else a local
//! `refs/heads/tracking` (and `main` the same way, or `master`), and never writes to either;
//! `fetch` brings the remote's up to date. Every call has a deadline and capped output, and
//! nothing reaches git's command line unchecked: the paths it reads are the branch's own,
//! checked as instance ids.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![warn(clippy::cognitive_complexity, clippy::too_many_lines)]
#![cfg_attr(test, allow(clippy::cognitive_complexity, clippy::too_many_lines))]

mod git;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use swarmception_model::{AgentFile, MainMerge, is_instance_id, parse_file, parse_merge};

pub use git::{Git, GitError};

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
    let (main, merges) = match first_ref(&git, &MAIN_REFS) {
        Some((main, head)) => (Some(main), merges(&git, &head)?),
        None => (None, Vec::new()),
    };
    Ok(Tracking { repo: top, reference, commit, files, skipped, main, merges })
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

/// Fetches the remote's tracking branch into `refs/remotes/origin/tracking`, then its `main`
/// into `refs/remotes/origin/main` when it has one.
///
/// # Errors
///
/// [`SourceError::Git`] when git fails or runs past its deadline fetching the tracking branch.
pub fn fetch(repo: &Path) -> Result<(), SourceError> {
    let git = Git::new(repo, FETCH_DEADLINE, 1024 * 1024);
    git.run(&["fetch", "--quiet", "origin", "+refs/heads/tracking:refs/remotes/origin/tracking"], None)?;
    // A remote whose main is called something else still has its tracking branch fetched.
    let _ = git.run(&["fetch", "--quiet", "origin", "+refs/heads/main:refs/remotes/origin/main"], None);
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
        std::fs::remove_dir_all(dir).ok();
    }
}
