//! The session: the repository open, what its tracking branch held, its GitHub issues, the rows
//! asked for and the timeline built from them. Reads run in the background when the session has
//! a waker (the UI) and land through [`Session::poll`]; the CLI and tests read in the foreground.
//! A live watch (#54, [`crate::live`]) fetches on an interval from [`Session::poll`], in the
//! background, and reads the branch again only when `tracking` or `main` moved. `repo.open`'s
//! check of the folder (git, with a deadline) runs in the background too with a waker (#161): the
//! UI's thread never starts a program, and the open lands through [`Session::poll`].

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use swarmception_domain::rows::{Style, rows_with};
use swarmception_domain::theme::Look;
use swarmception_domain::timeline::{Built, build};
use swarmception_domain::{Activity, Node, RowsSpec};
use swarmception_model::Timestamp;
use swarmception_source::{Github, Heads, SourceError, Tracking};

use crate::command::Command;
use crate::commands;
use crate::error::{EngineError, Refusal, Unavailable};
use crate::issues::{self, IssuesState};
use crate::live::{self, LiveCost, LiveState, millis};

/// Called when a background read lands, so a front end that draws only on input draws again.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// What a background read is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Job {
    /// Reading the tracking branch.
    Read,
    /// Fetching the remote's tracking branch, then reading it.
    Fetch,
    /// A live watch's check (#54): fetching `tracking` and `main`, and reading them again only
    /// when one moved.
    Live,
}

/// What the tracking branch held at the last read.
#[derive(Debug, Clone)]
pub(crate) struct Loaded {
    pub(crate) tracking: Tracking,
    pub(crate) activity: Activity,
}

/// What a read brought: the branch (none when a live check found nothing moved), and how long
/// the fetch and the read took.
#[derive(Debug)]
pub(crate) struct Landed {
    tracking: Option<Tracking>,
    fetch: Option<Duration>,
    read: Option<Duration>,
}

/// A bar as it is known across loads: its ticket (none for a merge onto main), its start and
/// its style. A row's bars by these, of one generation of rows.
pub(crate) type BarId = (Option<u64>, Timestamp, Style);
pub(crate) type RowIds = Vec<(String, Vec<BarId>)>;

/// How many generations of rows `timeline.carryBars` remembers.
const KEPT_GENERATIONS: usize = 8;

/// One swarm's timeline and its whole state.
#[derive(Default)]
pub struct Session {
    pub(crate) repo: Option<PathBuf>,
    pub(crate) loaded: Option<Loaded>,
    pub(crate) spec: RowsSpec,
    /// The look the timeline's colours are built in (#154): the window's, by `theme.set`.
    pub(crate) look: Look,
    pub(crate) rows: Vec<Node>,
    pub(crate) built: Option<Built>,
    pub(crate) error: Option<String>,
    pub(crate) job: Option<(Job, Receiver<Result<Landed, SourceError>>)>,
    /// The repository's GitHub issues, and their read going on.
    pub(crate) issues: IssuesState,
    /// The live watch (#54).
    pub(crate) live: LiveState,
    /// The bars of the last generations of rows, newest last: what `timeline.carryBars` maps.
    pub(crate) kept: VecDeque<(u64, RowIds)>,
    /// Where the issues come from: `gh` and the cache directory.
    github: Github,
    waker: Option<Waker>,
    /// Moved on by every new set of rows: a front end builds its timeline again.
    generation: u64,
    /// Moved on by every command that isn't read-only.
    revision: u64,
    /// The repositories opened, newest first (#69, `repo.recent`).
    recent: Vec<PathBuf>,
    /// Moved on by every repository opened (#69): a front end switches its state to the new one.
    opens: u64,
    /// `repo.open`'s check going on in the background (#161): the path asked for, and its answer.
    pub(crate) probing: Option<(String, Receiver<Result<PathBuf, Refusal>>)>,
    /// The last `repo.open` refused in the background, until the next is asked for (#161).
    pub(crate) refused: Option<crate::repo_cmds::OpenRefused>,
}

impl Session {
    /// A session that reads in the foreground.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A session that reads in the background, calling `waker` when a read lands.
    #[must_use]
    pub fn with_waker(waker: Waker) -> Self {
        Self { waker: Some(waker), ..Self::default() }
    }

    /// Reads the issues through `github` (its `gh`, its cache directory) from the next load on;
    /// by default `gh` on the path and the user's cache directory.
    pub fn set_github(&mut self, github: Github) {
        self.github = github;
    }

    /// The repository open (its top level once `repo.open` checked it), if any.
    #[must_use]
    pub fn repo(&self) -> Option<&Path> {
        self.repo.as_deref()
    }

    /// How many repositories this session has opened (#69): when it moves, another repository
    /// (or the same again) was opened, by whatever front end.
    #[must_use]
    pub const fn opens(&self) -> u64 {
        self.opens
    }

    pub(crate) const fn opened(&mut self) {
        self.opens = self.opens.wrapping_add(1);
    }

    /// The repositories opened, newest first (`repo.recent`).
    #[must_use]
    pub fn recent(&self) -> &[PathBuf] {
        &self.recent
    }

    /// Puts `repo` first in the recent list, once, at most [`crate::repo_cmds::MAX_RECENT`].
    pub(crate) fn remember(&mut self, repo: &Path) {
        self.recent.retain(|r| r != repo);
        self.recent.insert(0, repo.to_path_buf());
        self.recent.truncate(crate::repo_cmds::MAX_RECENT);
    }

    /// Adds `repos` (newest first, as kept across runs) after the ones opened this run, each once,
    /// at most [`crate::repo_cmds::MAX_RECENT`] in all (#69: the app hands back what it kept).
    pub fn remember_all(&mut self, repos: impl IntoIterator<Item = PathBuf>) {
        for repo in repos {
            if !repo.as_os_str().is_empty() && !self.recent.contains(&repo) && self.recent.len() < crate::repo_cmds::MAX_RECENT {
                self.recent.push(repo);
            }
        }
    }

    pub(crate) fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// Whether the issues are still being read.
    #[must_use]
    pub const fn reading_issues(&self) -> bool {
        self.issues.job.is_some()
    }

    /// Runs the command `id` with `params` (JSON text): its result as JSON text.
    ///
    /// # Errors
    ///
    /// [`EngineError::UnknownCommand`], or the command's own.
    pub fn execute(&mut self, id: &str, params: &str) -> Result<String, EngineError> {
        let entry = commands::find(id).ok_or_else(|| EngineError::UnknownCommand { command: id.to_owned() })?;
        entry.execute(self, params)
    }

    /// Runs `C` with typed params: its typed result.
    ///
    /// # Errors
    ///
    /// The command's errors.
    pub fn call<C: Command>(&mut self, params: C::Params) -> Result<C::Result, EngineError> {
        crate::command::call::<C>(self, params)
    }

    /// The look the timeline's colours are built in (#154).
    #[must_use]
    pub const fn look(&self) -> Look {
        self.look
    }

    /// The rows' generation and the state's revision: when the first moves, build the timeline
    /// again; when either moves, ask again.
    #[must_use]
    pub const fn revision(&self) -> (u64, u64) {
        (self.generation, self.revision)
    }

    pub(crate) const fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// The waker a background job calls when it lands; none for a session that works in the
    /// foreground.
    pub(crate) fn waker(&self) -> Option<Waker> {
        self.waker.clone()
    }

    /// The folder `repo.open` is checking in the background, if any (#161).
    #[must_use]
    pub fn opening(&self) -> Option<&str> {
        self.probing.as_ref().map(|(path, _)| path.as_str())
    }

    /// The last `repo.open` refused in the background, until the next is asked for (#161).
    #[must_use]
    pub const fn refused(&self) -> Option<&crate::repo_cmds::OpenRefused> {
        self.refused.as_ref()
    }

    /// Takes in `repo.open`'s check when it landed: the repository opened, or the refusal kept.
    fn poll_probe(&mut self) -> bool {
        let Some((path, receiver)) = &self.probing else { return false };
        let answer = match receiver.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err(Refusal::Source { message: "the folder's check stopped without an answer".into() }),
        };
        let path = path.clone();
        self.probing = None;
        match answer {
            Ok(top) => {
                // With a waker the read starts in the background, so this fails only without one.
                if let Err(error) = crate::repo_cmds::open_checked(self, top) {
                    self.error = Some(error.to_string());
                }
            }
            Err(refusal) => self.refused = Some(crate::repo_cmds::OpenRefused { path, refusal }),
        }
        self.touch();
        true
    }

    /// Takes in a background read that finished, and the issues that came, then starts a live
    /// check when one is due (#54): whether anything changed.
    pub fn poll(&mut self) -> bool {
        let probed = self.poll_probe();
        let (moved, changed) = self.issues.poll();
        if changed {
            self.rebuild();
        }
        if moved {
            self.touch();
        }
        let landed = self.poll_tracking();
        self.tick(Instant::now()) || landed || moved || probed
    }

    /// Starts a live check when one is due at `now` and nothing else is being read: only for a
    /// session that reads in the background (a front end's). Whether one started.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.waker.is_none() || self.repo.is_none() || self.job.is_some() || !self.live.is_due(now) {
            return false;
        }
        self.live.due = None;
        let started = self.start(Job::Live).is_ok();
        if started {
            self.touch();
        }
        started
    }

    /// How long until the next live check is due, for a front end to wake then; none when the
    /// watch is off or a read is going on.
    #[must_use]
    pub fn live_wait(&self, now: Instant) -> Option<Duration> {
        if !self.live.on || self.job.is_some() || self.repo.is_none() {
            return None;
        }
        Some(self.live.due.map_or(Duration::ZERO, |due| due.saturating_duration_since(now)))
    }

    /// The read going on, if any.
    #[must_use]
    pub fn job(&self) -> Option<Job> {
        self.job.as_ref().map(|(job, _)| *job)
    }

    fn poll_tracking(&mut self) -> bool {
        let Some((job, receiver)) = &self.job else { return false };
        let job = *job;
        match receiver.try_recv() {
            Ok(result) => {
                self.job = None;
                self.land(job, result);
                self.touch();
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                let stopped = "the read stopped without an answer";
                if job == Job::Live {
                    self.live.checked(Instant::now(), swarmception_source::github::now(), Err(stopped.into()));
                } else {
                    self.error = Some(stopped.into());
                }
                self.touch();
                true
            }
        }
    }

    /// Reads the repository open, fetching first for [`Job::Fetch`] and [`Job::Live`]: in the
    /// background with a waker, else now. A live check reads only when `tracking` or `main` moved
    /// from what was read last; a Reload or a Fetch takes the place of a live check going on.
    pub(crate) fn start(&mut self, job: Job) -> Result<(), EngineError> {
        let repo = self.repo.clone().ok_or(EngineError::Disabled { command: "repo.reload".into(), reason: Unavailable::NoRepo })?;
        let known = self.loaded.as_ref().map(|l| Heads::of(&l.tracking)).filter(|_| job == Job::Live);
        let work = move || read_job(&repo, job, known.as_ref());
        let Some(waker) = self.waker.clone() else {
            let result = work();
            let failed = result.as_ref().err().map(|e| Refusal::Source { message: e.to_string() });
            self.land(job, result);
            return failed.filter(|_| job != Job::Live).map_or(Ok(()), |refusal| Err(refusal.into()));
        };
        if self.job.as_ref().is_some_and(|(running, _)| *running == Job::Live) {
            // A live check given way to: the next is one wait on.
            self.live.due = Some(Instant::now() + self.live.wait());
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(work());
            waker();
        });
        self.job = Some((job, receiver));
        Ok(())
    }

    /// Reads the repository's GitHub issues: the cache, then `gh api` (#17); in the background
    /// with a waker, else now. A new repository's issues start afresh.
    pub(crate) fn start_issues(&mut self, fresh: bool) {
        let Some(repo) = self.repo.clone() else { return };
        if fresh {
            self.issues = IssuesState::default();
        }
        let github = self.github.clone();
        let Some(waker) = self.waker.clone() else {
            let mut changed = false;
            issues::read(&repo, &github, |news| changed |= self.issues.land(news));
            if changed {
                self.rebuild();
            }
            return;
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            issues::read(&repo, &github, |news| {
                let _ = sender.send(news);
                waker();
            });
            // The read is over once the channel closes: woken once more after that (#96), so a
            // front end that draws only when woken sees it end (no longer fetching).
            drop(sender);
            waker();
        });
        self.issues.job = Some(receiver);
    }

    fn land(&mut self, job: Job, result: Result<Landed, SourceError>) {
        if job == Job::Live {
            return self.land_live(result);
        }
        match result {
            Ok(Landed { tracking: Some(tracking), .. }) => self.take(tracking),
            Ok(Landed { tracking: None, .. }) => {}
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// What a live check brought: new events read, and the issues fetched again whenever it read
    /// the branch again (#69); or why it failed, kept apart from the session's error so the
    /// timeline stays as it was.
    fn land_live(&mut self, result: Result<Landed, SourceError>) {
        let wall = swarmception_source::github::now();
        let landed = match result {
            Ok(landed) => landed,
            Err(error) => return self.live.checked(Instant::now(), wall, Err(error.to_string())),
        };
        let mut cost = LiveCost { fetch_ms: landed.fetch.map_or(0, millis), read_ms: landed.read.map(millis), build_ms: None, new_events: None, issues: false };
        if let Some(tracking) = landed.tracking {
            let events = self.events();
            let started = Instant::now();
            self.take(tracking);
            cost.build_ms = Some(millis(started.elapsed()));
            cost.new_events = Some(self.events().saturating_sub(events));
            // Every check that brought something (new events, or a merge onto main) reads the
            // issues again, in the background, so the PRs' states (merged, closed) keep up
            // (#69): with a cache, one page of what changed since (#62).
            if self.issues.job.is_none() {
                self.start_issues(false);
                cost.issues = true;
            }
        }
        self.live.checked(Instant::now(), wall, Ok(cost));
    }

    /// The branch read: the activity, the rows; the live watch's default at the first read.
    fn take(&mut self, tracking: Tracking) {
        let activity = Activity::with_merges(&tracking.files, &tracking.merges);
        // Main can finish an old ticket after tracking ended, but live mode's default is
        // still based on the swarm's own recent activity, not an unrelated new main commit.
        let last = activity.times.last().copied();
        self.loaded = Some(Loaded { tracking, activity });
        self.error = None;
        self.rebuild();
        self.live.decide(last, swarmception_source::github::now(), Instant::now());
    }

    fn events(&self) -> usize {
        self.loaded.as_ref().map_or(0, |l| l.activity.events())
    }

    /// The rows again, from the activity and the spec, and the timeline's data from them; the
    /// rows before are remembered for `timeline.carryBars`.
    pub(crate) fn rebuild(&mut self) {
        self.kept.push_back((self.generation, row_ids(&self.rows)));
        while self.kept.len() > KEPT_GENERATIONS {
            self.kept.pop_front();
        }
        self.rows = self.loaded.as_ref().map(|l| rows_with(&l.activity, &l.tracking.merges, self.spec, &self.issues.issues)).unwrap_or_default();

        self.built = match build(&self.rows, !self.spec.collapse.is_on(), self.look) {
            Ok(built) => Some(built),
            Err(error) => {
                self.error = Some(format!("the timeline refused the rows: {error}"));
                None
            }
        };
        self.generation = self.generation.wrapping_add(1);
    }

    /// The timeline widget's timeline over the rows, on its `Time` scale's data: none before a
    /// read, or with no events. Built afresh each call, for a front end to load when
    /// [`Session::revision`]'s generation moves.
    ///
    /// # Errors
    ///
    /// [`Refusal::Timeline`] when the widget refuses the data.
    pub fn widget_timeline(&self) -> Result<Option<timeline_engine::timeline::Timeline>, EngineError> {
        let Some(built) = self.built.as_ref().filter(|b| !b.times.is_empty()) else { return Ok(None) };
        timeline_engine::timeline::Timeline::new(built.data.clone(), built.scales.clone())
            .map(Some)
            .map_err(|e| Refusal::Timeline { message: e.to_string() }.into())
    }

    /// Whether there are events to show, or why not.
    ///
    /// # Errors
    ///
    /// The reason there aren't.
    pub fn has_data(&self) -> Result<(), Unavailable> {
        match (&self.repo, &self.loaded, &self.job) {
            (None, _, _) => Err(Unavailable::NoRepo),
            (_, None, Some(_)) => Err(Unavailable::Loading),
            (_, Some(loaded), _) if loaded.activity.events() > 0 => Ok(()),
            _ => Err(Unavailable::NoData),
        }
    }

    /// Where `bars` of the rows of `generation` are in the rows now (#54): each found by its
    /// ticket, start and style, on its own row when it is still there, else on any; a bar the
    /// rows no longer have is left out.
    ///
    /// # Errors
    ///
    /// [`Refusal::OldRows`] for a generation older than the session remembers.
    pub(crate) fn carry(&self, generation: u64, bars: &[(String, usize)]) -> Result<Vec<(String, usize)>, EngineError> {
        let now = row_ids(&self.rows);
        let then = if generation == self.generation {
            &now
        } else {
            self.kept.iter().rev().find(|(g, _)| *g == generation).map(|(_, rows)| rows).ok_or(Refusal::OldRows { generation })?
        };
        let find = |(track, range): &(String, usize)| {
            let id = then.iter().find(|(key, _)| key == track).and_then(|(_, ids)| ids.get(*range))?;
            let on = |(key, ids): &(String, Vec<BarId>)| ids.iter().position(|other| other == id).map(|range| (key.clone(), range));
            now.iter().filter(|(key, _)| key == track).find_map(on).or_else(|| now.iter().find_map(on))
        };
        Ok(bars.iter().filter_map(find).collect())
    }
}

/// `rows`' bars by their [`BarId`]s, row by row.
fn row_ids(rows: &[Node]) -> RowIds {
    let leaves = rows.iter().flat_map(Node::leaves);
    leaves
        .filter_map(|leaf| match leaf {
            Node::Ranges { key, bars, .. } => Some((key.clone(), bars.iter().map(|b| (b.ticket, b.start, b.style)).collect())),
            _ => None,
        })
        .collect()
}

/// A read: a fetch first for [`Job::Fetch`] and [`Job::Live`], then the branch; for a live check,
/// only when the heads moved from `known`.
fn read_job(repo: &Path, job: Job, known: Option<&Heads>) -> Result<Landed, SourceError> {
    let started = Instant::now();
    match job {
        Job::Read => {}
        Job::Fetch => swarmception_source::fetch(repo)?,
        Job::Live => swarmception_source::fetch_within(repo, live::FETCH_DEADLINE)?,
    }
    if let Some(known) = known
        && swarmception_source::heads(repo)? == *known
    {
        return Ok(Landed { tracking: None, fetch: Some(started.elapsed()), read: None });
    }
    let fetch = (job != Job::Read).then(|| started.elapsed());
    let started = Instant::now();
    let tracking = swarmception_source::read(repo)?;
    Ok(Landed { tracking: Some(tracking), fetch, read: Some(started.elapsed()) })
}
