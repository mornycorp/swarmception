//! The repository's GitHub issues in the session (#17): read on every load (`repo.open`,
//! `repo.reload`, `repo.fetch`) beside the tracking branch, never in its way. The cache comes
//! first, then `gh api`'s answer, which is cached in turn; in the background with a waker, else
//! now. No GitHub remote, no `gh`, no network: an error the status shows, the timeline loaded all
//! the same. With a cache, only what changed since is asked for and merged in (#62:
//! `github::refresh`); `repo.status` says how the last refresh went ([`IssuesRefresh`]).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_model::{IssueCache, Issues, Timestamp};
use swarmception_source::github::{self, FullWhy, Github, GithubError, Refresh, Slug};

/// Where the issues shown came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IssuesSource {
    /// The cache on disk, from an earlier fetch.
    Cache,
    /// GitHub, this load.
    Github,
}

/// How a refresh of the issues went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RefreshKind {
    /// Every page.
    Full,
    /// Only what changed since the cache's newest change.
    Incremental,
    /// GitHub said nothing changed.
    Unchanged,
}

/// The last refresh of the issues from GitHub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuesRefresh {
    pub kind: RefreshKind,
    /// A full one's: why (no cache, another version's, a week old, too many changed).
    pub why: Option<String>,
    /// An incremental one's: how many came, and how many of them were new or different.
    pub came: usize,
    pub changed: usize,
    /// How long it took, `gh` and all.
    pub ms: u64,
}

impl IssuesRefresh {
    fn of(refresh: Refresh, ms: u64) -> Self {
        let (kind, why, came, changed) = match refresh {
            Refresh::Full(why) => (RefreshKind::Full, Some(why), 0, 0),
            Refresh::Incremental { came, changed } => (RefreshKind::Incremental, None, came, changed),
            Refresh::Unchanged => (RefreshKind::Unchanged, None, 0, 0),
        };
        Self { kind, why: why.as_ref().map(FullWhy::to_string), came, changed, ms }
    }
}

/// What the session knows of the repository's issues, for `repo.status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssuesStatus {
    /// `owner/name` on GitHub, from the `origin` remote.
    pub github: Option<String>,
    pub source: Option<IssuesSource>,
    /// Issues and pull requests held, and how many of them are pull requests.
    pub issues: usize,
    pub pulls: usize,
    /// When they were fetched from GitHub.
    pub fetched: Option<Timestamp>,
    /// A fetch is going on.
    pub fetching: bool,
    /// Why the last fetch (or the cache) failed.
    pub error: Option<String>,
    /// The cache file.
    pub cache: Option<String>,
    /// How the last refresh from GitHub went.
    #[serde(default)]
    pub refresh: Option<IssuesRefresh>,
}

/// What the issues' reader says, as it goes.
#[derive(Debug)]
pub(crate) enum News {
    Slug(Slug),
    Cached(IssueCache),
    Fetched { cache: IssueCache, saved: Result<Option<PathBuf>, GithubError>, refresh: IssuesRefresh },
    Failed(GithubError),
}

/// The issues the session holds, and the read going on.
#[derive(Debug, Default)]
pub(crate) struct IssuesState {
    pub(crate) slug: Option<Slug>,
    pub(crate) issues: Issues,
    pub(crate) source: Option<IssuesSource>,
    pub(crate) fetched: Option<Timestamp>,
    pub(crate) error: Option<String>,
    pub(crate) cache: Option<PathBuf>,
    pub(crate) refresh: Option<IssuesRefresh>,
    pub(crate) job: Option<Receiver<News>>,
}

impl IssuesState {
    pub(crate) fn status(&self) -> IssuesStatus {
        IssuesStatus {
            github: self.slug.as_ref().map(ToString::to_string),
            source: self.source,
            issues: self.issues.len(),
            pulls: self.issues.pulls(),
            fetched: self.fetched,
            fetching: self.job.is_some(),
            error: self.error.clone(),
            cache: self.cache.as_ref().map(|p| p.display().to_string()),
            refresh: self.refresh.clone(),
        }
    }

    /// Takes in one piece of news: whether the issues changed, so the rows are built again.
    pub(crate) fn land(&mut self, news: News) -> bool {
        match news {
            News::Slug(slug) => {
                self.slug = Some(slug);
                false
            }
            News::Cached(cache) => {
                // A refresh after a fetch landed keeps what GitHub said.
                if self.source.is_some() {
                    return false;
                }
                self.fetched = cache.fetched;
                self.source = Some(IssuesSource::Cache);
                self.replace(Issues::new(cache.issues))
            }
            News::Fetched { cache, saved, refresh } => {
                self.fetched = cache.fetched;
                self.refresh = Some(refresh);
                self.source = Some(IssuesSource::Github);
                match saved {
                    Ok(path) => {
                        self.cache = path;
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
                self.replace(Issues::new(cache.issues))
            }
            News::Failed(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn replace(&mut self, issues: Issues) -> bool {
        let changed = issues != self.issues;
        self.issues = issues;
        changed
    }

    /// Takes in what the background read sent since: whether anything moved, and whether the
    /// issues changed.
    pub(crate) fn poll(&mut self) -> (bool, bool) {
        let (mut moved, mut changed) = (false, false);
        while let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(news) => {
                    moved = true;
                    changed |= self.land(news);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.job = None;
                    moved = true;
                }
            }
        }
        (moved, changed)
    }
}

/// Reads `repo`'s issues: its GitHub slug, the cache, then `gh api` (all of them, or what changed
/// since the cache), cached in turn; each piece of news `send` as it comes. A failed fetch leaves
/// the cache as it was.
pub(crate) fn read(repo: &std::path::Path, from: &Github, mut send: impl FnMut(News)) {
    let slug = match github::origin(repo) {
        Ok(slug) => slug,
        Err(error) => return send(News::Failed(error)),
    };
    send(News::Slug(slug.clone()));
    let cached = from.cache_dir.as_ref().and_then(|dir| match github::load_cache(dir, &slug) {
        Ok(cache) => cache,
        Err(error) => {
            send(News::Failed(error));
            None
        }
    });
    if let Some(cache) = &cached {
        send(News::Cached(cache.clone()));
    }
    let started = Instant::now();
    match github::refresh(&from.gh, repo, &slug, cached, github::now()) {
        Ok((cache, how)) => {
            let refresh = IssuesRefresh::of(how, u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
            let saved = from.cache_dir.as_ref().map(|dir| github::save_cache(dir, &slug, &cache)).transpose();
            send(News::Fetched { cache, saved, refresh });
        }
        Err(error) => send(News::Failed(error)),
    }
}
