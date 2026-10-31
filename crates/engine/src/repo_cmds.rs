//! The repository: open one, read its tracking branch again, fetch it, and say what it held.

use std::collections::BTreeSet;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::Timestamp;

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::session::{Job, Session};

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
}

fn status(session: &Session) -> StatusResult {
    let mut status = StatusResult {
        repo: session.repo.as_ref().map(|p| p.display().to_string()),
        job: session.job.as_ref().map(|(job, _)| *job),
        error: session.error.clone(),
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
    status.merged = activity.issue_events.iter().filter(|e| e.kind.name() == "merge").map(|e| e.issue).collect::<BTreeSet<_>>().len();
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

/// `repo.open`: open a repository and read its tracking branch.
pub struct Open;

impl Command for Open {
    const ID: &'static str = "repo.open";
    const LABEL: &'static str = "Open Repository";
    type Params = OpenParams;
    type Result = StatusResult;

    fn run(session: &mut Session, params: OpenParams) -> Result<StatusResult, EngineError> {
        let path = PathBuf::from(&params.path);
        if !path.is_dir() {
            return Err(Refusal::NotADirectory { path: params.path }.into());
        }
        session.repo = Some(path);
        session.loaded = None;
        session.rebuild();
        session.start(Job::Read)?;
        Ok(status(session))
    }
}

fn has_repo(session: &Session) -> Result<(), Unavailable> {
    if session.repo.is_none() {
        return Err(Unavailable::NoRepo);
    }
    if session.job.is_some() {
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
        session.start(Job::Read)?;
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
        session.start(Job::Fetch)?;
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
