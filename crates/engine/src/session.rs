//! The session: the repository open, what its tracking branch held, the rows asked for and the
//! timeline built from them. Reads run in the background when the session has a waker (the UI)
//! and land through [`Session::poll`]; the CLI and tests read in the foreground.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use swarmception_domain::rows::rows;
use swarmception_domain::timeline::{Built, build};
use swarmception_domain::{Activity, Node, RowsSpec};
use swarmception_source::{SourceError, Tracking};

use crate::command::Command;
use crate::commands;
use crate::error::{EngineError, Refusal, Unavailable};

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
}

/// What the tracking branch held at the last read.
#[derive(Debug, Clone)]
pub(crate) struct Loaded {
    pub(crate) tracking: Tracking,
    pub(crate) activity: Activity,
}

/// One swarm's timeline and its whole state.
#[derive(Default)]
pub struct Session {
    pub(crate) repo: Option<PathBuf>,
    pub(crate) loaded: Option<Loaded>,
    pub(crate) spec: RowsSpec,
    pub(crate) rows: Vec<Node>,
    pub(crate) built: Option<Built>,
    pub(crate) error: Option<String>,
    pub(crate) job: Option<(Job, Receiver<Result<Tracking, SourceError>>)>,
    waker: Option<Waker>,
    /// Moved on by every new set of rows: a front end builds its timeline again.
    generation: u64,
    /// Moved on by every command that isn't read-only.
    revision: u64,
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

    /// The rows' generation and the state's revision: when the first moves, build the timeline
    /// again; when either moves, ask again.
    #[must_use]
    pub const fn revision(&self) -> (u64, u64) {
        (self.generation, self.revision)
    }

    pub(crate) const fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Takes in a background read that finished: whether anything changed.
    pub fn poll(&mut self) -> bool {
        let Some((_, receiver)) = &self.job else { return false };
        match receiver.try_recv() {
            Ok(result) => {
                self.job = None;
                self.land(result);
                self.touch();
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.error = Some("the read stopped without an answer".into());
                self.touch();
                true
            }
        }
    }

    /// Reads the repository open, fetching first for [`Job::Fetch`]: in the background with a
    /// waker, else now.
    pub(crate) fn start(&mut self, job: Job) -> Result<(), EngineError> {
        let repo = self.repo.clone().ok_or(EngineError::Disabled { command: "repo.reload".into(), reason: Unavailable::NoRepo })?;
        let work = move || {
            if job == Job::Fetch {
                swarmception_source::fetch(&repo)?;
            }
            swarmception_source::read(&repo)
        };
        let Some(waker) = self.waker.clone() else {
            let result = work();
            let failed = result.as_ref().err().map(|e| Refusal::Source { message: e.to_string() });
            self.land(result);
            return failed.map_or(Ok(()), |refusal| Err(refusal.into()));
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(work());
            waker();
        });
        self.job = Some((job, receiver));
        Ok(())
    }

    fn land(&mut self, result: Result<Tracking, SourceError>) {
        match result {
            Ok(tracking) => {
                let activity = Activity::of(&tracking.files);
                self.loaded = Some(Loaded { tracking, activity });
                self.error = None;
                self.rebuild();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// The rows again, from the activity and the spec, and the timeline's data from them.
    pub(crate) fn rebuild(&mut self) {
        self.rows = self.loaded.as_ref().map(|l| rows(&l.activity, self.spec)).unwrap_or_default();
        self.built = match build(&self.rows, !self.spec.collapse) {
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
}
