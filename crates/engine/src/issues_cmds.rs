//! A ticket on GitHub (#17): its issue's page and its pull request's, for a front end to open
//! (a bar's context menu, a factory chip's). The pages come from the issues fetched when they
//! have the ticket, else from the `origin` remote's `owner/name`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::tickets;
use swarmception_model::IssueKind;

use crate::command::Command;
use crate::error::{EngineError, Unavailable};
use crate::factory_cmds::TicketParams;
use crate::session::Session;

/// A ticket's pages on GitHub.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TicketUrlsResult {
    pub issue: u64,
    /// Its title, when the issues fetched have it.
    pub title: Option<String>,
    /// Its page: an issue's, or a pull request's when the number is one.
    pub issue_url: String,
    /// Its pull request: the one its events name, else the newest that closes it.
    pub pr: Option<u64>,
    pub pr_url: Option<String>,
    /// The number's own page as a pull request (`/pull/<n>`), for a number known to be one (a
    /// merge onto main's).
    pub pull_url: String,
}

/// `ticket.urls`: a ticket's issue page and its pull request's on GitHub.
pub struct Urls;

impl Command for Urls {
    const ID: &'static str = "ticket.urls";
    const LABEL: &'static str = "Ticket Pages";
    const READ_ONLY: bool = true;
    type Params = TicketParams;
    type Result = TicketUrlsResult;

    fn enabled(session: &Session) -> Result<(), Unavailable> {
        match (&session.repo, &session.issues.slug) {
            (None, _) => Err(Unavailable::NoRepo),
            (Some(_), None) if session.issues.job.is_some() => Err(Unavailable::Loading),
            (Some(_), None) => Err(Unavailable::NoGithub),
            (Some(_), Some(_)) => Ok(()),
        }
    }

    fn run(session: &mut Session, params: TicketParams) -> Result<TicketUrlsResult, EngineError> {
        let disabled = |reason| EngineError::Disabled { command: Self::ID.to_owned(), reason };
        let slug = session.issues.slug.as_ref().ok_or_else(|| disabled(Unavailable::NoGithub))?;
        let issues = &session.issues.issues;
        let issue = params.issue;
        let found = issues.get(issue);
        let named = session.loaded.as_ref().and_then(|l| tickets(&l.activity).into_iter().find(|t| t.issue == issue)).and_then(|t| t.pr);
        let own = found.filter(|i| i.kind == IssueKind::PullRequest).map(|i| i.number);
        let pr = issues.pr_of(issue, named.or(own));
        let issue_url = found.map(|i| i.url.clone()).filter(|u| u.starts_with("https://github.com/")).unwrap_or_else(|| slug.issue_url(issue));
        let pr_url = pr.as_ref().map(|p| if p.url.starts_with("https://github.com/") { p.url.clone() } else { slug.pull_url(p.number) });
        let pull_url = if own.is_some() { issue_url.clone() } else { slug.pull_url(issue) };
        let title = found.map(|i| i.title.clone()).filter(|t| !t.is_empty());
        Ok(TicketUrlsResult { issue, title, issue_url, pr: pr.map(|p| p.number), pr_url, pull_url })
    }
}

/// The page of `number` on GitHub, an issue's or (`pull`) a pull request's: the one the issues
/// fetched give, else the `origin` remote's; none when the repository isn't on GitHub.
pub(crate) fn page_url(session: &Session, pull: bool, number: u64) -> Option<String> {
    let slug = session.issues.slug.as_ref()?;
    let found = session.issues.issues.get(number).filter(|i| i.url.starts_with("https://github.com/"));
    Some(match found {
        Some(issue) if !pull || issue.kind == IssueKind::PullRequest => issue.url.clone(),
        _ if pull => slug.pull_url(number),
        _ => slug.issue_url(number),
    })
}
