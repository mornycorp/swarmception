//! The details of what is selected (#47): a ticket, a gate run, a session, a merge onto main, or
//! a bar on the timeline (its ticket, or the merge onto main it draws), told in time order
//! (`swarmception_domain::details`), with the pages it opens on GitHub.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::details::{Details, PageKind, Subject, bar_subject, details};

use crate::command::Command;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::issues_cmds::page_url;
use crate::session::Session;
use crate::stats_cmds::At as When;

/// What to tell: exactly one of a ticket, a gate run, a session, a merge onto main, or a bar on
/// the timeline (`track` and `range`, as `timeline.ticketBars` names one).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DetailsParams {
    /// A ticket, by its issue number.
    pub ticket: Option<u64>,
    /// A gate run, by its sub-agent's id (`gate-pr50`).
    pub gate: Option<String>,
    /// An agent instance's session, by the instance's id (`mini-79ba63`).
    pub session: Option<String>,
    /// A merge onto main, by its commit (seven characters or more of it).
    pub merge: Option<String>,
    /// A bar on the timeline: its row's key (`tickets/2`, `main`) …
    pub track: Option<String>,
    /// … and its place among the row's bars, from 0.
    pub range: Option<usize>,
    /// For a gate run or a session: a time in it (seconds since the epoch, or
    /// `YYYY-MM-DDTHH:MM:SSZ`), to pick one of a gate's reruns or an instance's sessions.
    pub at: Option<When>,
}

/// `details.get`: the story of what is selected, in time order: for a ticket, its claims, its
/// phases with their times, its send backs, its gate runs and its merge; for a gate run, its
/// notes, the PRs it carried and its merges; for a session, its events; for a merge onto main,
/// its gate run and its ticket's merge; each with the issue's title, labels and pages from the
/// issues fetched.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "details.get";
    const LABEL: &'static str = "Details";
    const READ_ONLY: bool = true;
    type Params = DetailsParams;
    type Result = Details;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        session.has_data()
    }

    fn run(session: &mut Session, params: DetailsParams) -> Result<Details, EngineError> {
        let subject = subject_of(session, &params)?;
        let Some(loaded) = &session.loaded else { return Err(EngineError::Disabled { command: Self::ID.to_owned(), reason: Unavailable::NoData }) };
        let found = details(&loaded.activity, &loaded.tracking.merges, &session.issues.issues, &subject);
        let mut found = found.ok_or_else(|| Refusal::NotFound { what: described(&subject) })?;
        for page in &mut found.pages {
            page.url = page_url(session, page.kind == PageKind::Pull, page.number);
        }
        Ok(found)
    }
}

/// What the params name, a bar read as its ticket or its merge onto main.
fn subject_of(session: &Session, params: &DetailsParams) -> Result<Subject, EngineError> {
    let bar = params.track.is_some() || params.range.is_some();
    let named = [params.ticket.is_some(), params.gate.is_some(), params.session.is_some(), params.merge.is_some(), bar];
    if named.iter().filter(|n| **n).count() != 1 {
        return Err(EngineError::BadParams { message: "name one of ticket, gate, session, merge, or a bar (track and range)".into() });
    }
    let at = params.at.as_ref().map(When::time).transpose()?;
    let subject = match (&params.ticket, &params.gate, &params.session, &params.merge) {
        (Some(issue), ..) => Subject::Ticket { issue: *issue },
        (_, Some(sub), ..) => Subject::Gate { sub: sub.clone(), at },
        (_, _, Some(instance), _) => Subject::Session { instance: instance.clone(), at },
        (.., Some(commit)) => Subject::Merge { commit: commit.clone() },
        (None, None, None, None) => {
            let (Some(track), Some(range)) = (&params.track, params.range) else {
                return Err(EngineError::BadParams { message: "a bar is its track and its range".into() });
            };
            let merges = session.loaded.as_ref().map(|l| l.tracking.merges.as_slice()).unwrap_or_default();
            return bar_subject(&session.rows, merges, track, range).ok_or_else(|| {
                Refusal::NotFound { what: swarmception_domain::titles::short(&format!("ticket or merge drawn as bar {range} of row {track}"), 120) }.into()
            });
        }
    };
    Ok(subject)
}

/// What was asked for, for the refusal: its name cut to a line.
fn described(subject: &Subject) -> String {
    let text = match subject {
        Subject::Ticket { issue } => format!("ticket #{issue}"),
        Subject::Gate { sub, .. } => format!("gate run {sub}"),
        Subject::Session { instance, .. } => format!("session of {instance}"),
        Subject::Merge { commit } => format!("merge onto main {commit}"),
    };
    swarmception_domain::titles::short(&text, 120)
}
