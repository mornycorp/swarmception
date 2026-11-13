//! The repository: open one, read its tracking branch again, fetch it, and say what it held.
//! Each load reads the repository's GitHub issues too, beside the branch (#17).

use std::collections::BTreeSet;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::issues::IssuesStatus;
use crate::live::LiveState;
use crate::session::{Job, Session};
use swarmception_source::SourceError;

/// The most line errors a status lists.
pub const MAX_LISTED_ERRORS: usize = 20;

/// What `repo.open` opens.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenParams {
    /// A directory inside a git repository whose tracking branch holds a swarm's events.
    pub path: String,
}

/// A line of the branch that didn't read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LineErrorWire {
    pub instance: String,
    pub line: usize,
    pub message: String,
}

/// What the repository and its tracking branch held at the last read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusResult {
    /// The repository's top level, once read; the path asked for before.
    pub repo: Option<String>,
    /// The tracking ref read and its commit.
    pub reference: Option<String>,
    pub commit: Option<String>,
    /// The read going on.
    pub job: Option<Job>,
    /// Why the last read failed.
    pub error: Option<String>,
    pub machines: usize,
    /// Distinct worker names (a name's sessions on one machine are one worker).
    pub workers: usize,
    /// Agent instances: one file each.
    pub instances: usize,
    pub tickets: usize,
    /// Tickets merged, and merge commits on main (none when the repository has no main).
    pub merged: usize,
    pub main_merges: usize,
    /// The main ref read.
    pub main: Option<String>,
    pub events: usize,
    pub first: Option<Timestamp>,
    pub last: Option<Timestamp>,
    /// Lines that didn't read, how many, and the first of them.
    pub line_errors: usize,
    pub errors: Vec<LineErrorWire>,
    /// The repository's GitHub issues: where from, how many, the fetch going on, why it failed.
    pub issues: IssuesStatus,
    /// The folder `repo.open` is checking in the background (#161: a front end's session); the
    /// repository open stays open meanwhile.
    pub opening: Option<String>,
    /// The last `repo.open` refused in the background, until the next is asked for (#161).
    pub refused: Option<OpenRefused>,
}

/// A `repo.open` refused once its check landed in the background (#161): the path asked for and
/// why. The repository open stays open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenRefused {
    pub path: String,
    pub refusal: Refusal,
}

fn status(session: &Session) -> StatusResult {
    let mut status = StatusResult {
        repo: session.repo.as_ref().map(|p| p.display().to_string()),
        job: session.job.as_ref().map(|(job, _)| *job),
        error: session.error.clone(),
        issues: session.issues.status(),
        opening: session.opening().map(str::to_owned),
        refused: session.refused.clone(),
        ..StatusResult::default()
    };
    let Some(loaded) = &session.loaded else { return status };
    let activity = &loaded.activity;
    status.repo = Some(loaded.tracking.repo.display().to_string());
    status.reference = Some(loaded.tracking.reference.clone());
    status.commit = Some(loaded.tracking.commit.clone());
    status.machines = activity.agents.iter().map(|a| &a.machine).collect::<BTreeSet<_>>().len();
    status.workers = activity.agents.iter().map(|a| (&a.machine, &a.name)).collect::<BTreeSet<_>>().len();
    status.instances = activity.agents.len();
    let issues = activity.agents.iter().flat_map(|a| a.work.iter().map(|w| w.issue).chain(a.marks.iter().filter_map(|m| m.issue)));
    status.tickets = issues.collect::<BTreeSet<_>>().len();
    status.merged = swarmception_domain::tickets(activity).iter().filter(|t| t.merged.is_some()).count();
    status.main_merges = loaded.tracking.merges.len();
    status.main.clone_from(&loaded.tracking.main);
    status.events = activity.events();
    status.first = activity.extent.map(|(first, _)| first);
    status.last = activity.extent.map(|(_, last)| last);
    status.line_errors = activity.errors.len();
    status.errors = activity
        .errors
        .iter()
        .take(MAX_LISTED_ERRORS)
        .map(|e| LineErrorWire { instance: e.instance.clone(), line: e.line, message: e.message.clone() })
        .collect();
    status
}

/// Whether `path` is a swarm's clone to open (#69): a folder there, in a git repository with a
/// tracking branch; its top level. A few `rev-parse`s with a deadline, nothing read.
///
/// # Errors
///
/// [`Refusal::NoSuchFolder`], [`Refusal::NotADirectory`], [`Refusal::NotARepository`],
/// [`Refusal::NoTrackingBranch`], or [`Refusal::Source`] when git itself fails.
pub fn check(path: &str) -> Result<PathBuf, Refusal> {
    let folder = PathBuf::from(path);
    if path.trim().is_empty() || !folder.exists() {
        return Err(Refusal::NoSuchFolder { path: path.to_owned() });
    }
    if !folder.is_dir() {
        return Err(Refusal::NotADirectory { path: path.to_owned() });
    }
    match swarmception_source::probe(&folder) {
        Ok(probe) => Ok(probe.top),
        Err(SourceError::NotARepo { message, .. }) => Err(Refusal::NotARepository { path: path.to_owned(), message }),
        Err(SourceError::NoTrackingBranch { path: top }) => Err(Refusal::NoTrackingBranch { path: top.display().to_string() }),
        Err(error @ SourceError::Git(_)) => Err(Refusal::Source { message: error.to_string() }),
    }
}

/// `repo.open`: open a repository and read its tracking branch. A folder that isn't a swarm's
/// clone ([`check`]) is refused before anything changes: the repository open stays open, its
/// watch going on (#69). Opened, the live watch starts afresh at the new repository's default
/// (#69: the old one's stops), and the repository goes first in `repo.recent`.
///
/// A session with a waker (a front end's) checks the folder in the background (#161): the answer
/// comes at once with `opening` set, and the repository opens, or `refused` says why not, when
/// the check lands ([`Session::poll`]). Another `repo.open` meanwhile takes its place.
pub struct Open;

impl Command for Open {
    const ID: &'static str = "repo.open";
    const LABEL: &'static str = "Open Repository";
    type Params = OpenParams;
    type Result = StatusResult;

    fn run(session: &mut Session, params: OpenParams) -> Result<StatusResult, EngineError> {
        session.refused = None;
        let Some(waker) = session.waker() else {
            session.probing = None;
            let top = check(&params.path)?;
            return open_checked(session, top);
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let path = params.path.clone();
        std::thread::spawn(move || {
            let _ = sender.send(check(&path));
            waker();
        });
        session.probing = Some((params.path, receiver));
        Ok(status(session))
    }
}

/// Opens `top`, a folder [`check`] passed: what `repo.open` does once the check is in.
pub(crate) fn open_checked(session: &mut Session, top: PathBuf) -> Result<StatusResult, EngineError> {
    session.remember(&top);
    session.repo = Some(top);
    session.loaded = None;
    session.error = None;
    // The old repository's read and watch stop: what they bring is dropped.
    session.job = None;
    session.live = LiveState::default();
    session.opened();
    session.rebuild();
    let read = session.start(Job::Read);
    session.start_issues(true);
    read?;
    Ok(status(session))
}

/// The most repositories `repo.recent` lists.
pub const MAX_RECENT: usize = 10;

/// A repository opened before, as `repo.recent` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecentRepo {
    /// Its top level, as opened.
    pub path: String,
    /// Its folder's name, as File ▸ Open Recent shows it.
    pub name: String,
    /// Nothing is there now (moved or deleted): opening it is refused.
    pub missing: bool,
}

/// `repo.recent`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecentResult {
    /// The newest first, at most [`MAX_RECENT`].
    pub repos: Vec<RecentRepo>,
}

fn recent(session: &Session) -> RecentResult {
    let repos = session
        .recent()
        .iter()
        .map(|path| RecentRepo {
            path: path.display().to_string(),
            name: path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned()),
            missing: !path.is_dir(),
        })
        .collect();
    RecentResult { repos }
}

/// `repo.recent`: the repositories opened last, newest first (#69): what File ▸ Open Recent
/// lists. The app keeps the list across runs (app-wide, not per repository) and hands it back
/// at the start (`Session::remember_all`).
pub struct Recent;

impl Command for Recent {
    const ID: &'static str = "repo.recent";
    const LABEL: &'static str = "Recent Repositories";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = RecentResult;

    fn run(session: &mut Session, _params: NoParams) -> Result<RecentResult, EngineError> {
        Ok(recent(session))
    }
}

/// `repo.clearRecent`: empties the list of repositories opened (File ▸ Open Recent ▸ Clear
/// Menu); the repository open stays open.
pub struct ClearRecent;

impl Command for ClearRecent {
    const ID: &'static str = "repo.clearRecent";
    const LABEL: &'static str = "Clear Recent Repositories";
    type Params = NoParams;
    type Result = RecentResult;

    fn run(session: &mut Session, _params: NoParams) -> Result<RecentResult, EngineError> {
        session.clear_recent();
        Ok(recent(session))
    }
}

/// A repository is open and no read is going on: a live check (#54) gives way to a Reload or a
/// Fetch, so it never holds them up.
pub(crate) fn has_repo(session: &Session) -> Result<(), Unavailable> {
    if session.repo.is_none() {
        return Err(Unavailable::NoRepo);
    }
    if session.job.as_ref().is_some_and(|(job, _)| *job != Job::Live) {
        return Err(Unavailable::Loading);
    }
    Ok(())
}

/// `repo.reload`: read the tracking branch again.
pub struct Reload;

impl Command for Reload {
    const ID: &'static str = "repo.reload";
    const LABEL: &'static str = "Reload";
    type Params = NoParams;
    type Result = StatusResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        has_repo(session)
    }

    fn run(session: &mut Session, _params: NoParams) -> Result<StatusResult, EngineError> {
        let read = session.start(Job::Read);
        session.start_issues(false);
        read?;
        Ok(status(session))
    }
}

/// `repo.fetch`: fetch the remote's tracking branch, then read it.
pub struct Fetch;

impl Command for Fetch {
    const ID: &'static str = "repo.fetch";
    const LABEL: &'static str = "Fetch and Reload";
    type Params = NoParams;
    type Result = StatusResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        has_repo(session)
    }

    fn run(session: &mut Session, _params: NoParams) -> Result<StatusResult, EngineError> {
        let read = session.start(Job::Fetch);
        session.start_issues(false);
        read?;
        Ok(status(session))
    }
}

/// `repo.status`: the repository, the read going on, and what the branch held.
pub struct Status;

impl Command for Status {
    const ID: &'static str = "repo.status";
    const LABEL: &'static str = "Status";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = StatusResult;

    fn run(session: &mut Session, _params: NoParams) -> Result<StatusResult, EngineError> {
        Ok(status(session))
    }
}
