//! A command: an id, a label, a typed params type and result type with their JSON Schemas, an
//! `enabled` check, and `run`. The one adapter, [`Entry::of`], erases its types for the
//! registry: it bounds and parses the params, catches a panic in `enabled` or `run`, and
//! serializes the result, so no command reads raw JSON. A command a person runs from the menus says
//! where ([`MenuPlace`], #160): the UI builds its menus and shortcuts from the registry, and greys
//! out what can't run now with [`Entry::state`]'s reason.

use std::panic::{AssertUnwindSafe, catch_unwind};

use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{EngineError, Unavailable};
use crate::session::Session;

/// The largest params text a command takes.
pub const MAX_PARAMS: usize = 1024 * 1024;

/// Where a command sits in the menus (#160, swarm-core's contract: the UI builds its menus,
/// toolbars and shortcuts from the registry), so a front end names no command to make a menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuPlace {
    /// Its menu, then any submenu: `["File"]`, `["File", "Open Recent"]`.
    pub path: &'static [&'static str],
    /// What the item shows.
    pub label: &'static str,
    /// Its group in the menu: groups in order, a separator between them; within one, the
    /// registry's order.
    pub group: u8,
    pub shortcut: Option<Shortcut>,
    /// What the front end asks the person for before it runs, put in the params.
    pub asks: Option<Ask>,
}

/// A menu item's shortcut: the platform's command key (⌘ on macOS, Ctrl elsewhere), Shift with
/// it or not, and a letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub shift: bool,
    /// `A` to `Z`.
    pub key: char,
}

impl Shortcut {
    /// ⌘ (Ctrl) and `key`.
    #[must_use]
    pub const fn command(key: char) -> Self {
        Self { shift: false, key }
    }

    /// ⌘ (Ctrl), Shift and `key`.
    #[must_use]
    pub const fn command_shift(key: char) -> Self {
        Self { shift: true, key }
    }
}

/// What a front end asks for before it runs a menu's command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// A folder (the host's picker), as the param named.
    Folder { param: &'static str },
}

/// One action or query. Its params type refuses unknown fields (`#[serde(deny_unknown_fields)]`).
pub trait Command {
    /// `area.name`, lowerCamelCase segments: `rows.set`.
    const ID: &'static str;
    /// What a menu or a listing shows.
    const LABEL: &'static str;
    type Params: DeserializeOwned + JsonSchema;
    type Result: Serialize + JsonSchema;
    /// Whether it only reads: a read-only command leaves the session's revision where it was.
    const READ_ONLY: bool = false;
    /// Where it sits in the menus, when a person runs it from there (#160).
    const MENU: Option<MenuPlace> = None;

    /// Whether it can run now, or why not.
    ///
    /// # Errors
    ///
    /// The reason it can't, for the caller to show.
    fn enabled(_session: &Session) -> Result<(), Unavailable> {
        Ok(())
    }

    /// Runs it.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] for params whose values it refuses, or what it ran into.
    fn run(session: &mut Session, params: Self::Params) -> Result<Self::Result, EngineError>;
}

/// A command with its types erased, as the registry holds it.
#[derive(Clone, Copy)]
pub struct Entry {
    pub id: &'static str,
    pub label: &'static str,
    pub read_only: bool,
    pub menu: Option<MenuPlace>,
    enabled: fn(&Session) -> Result<(), Unavailable>,
    params_schema: fn() -> Schema,
    result_schema: fn() -> Schema,
    execute: fn(&mut Session, &str) -> Result<String, EngineError>,
}

impl Entry {
    /// The entry for `C`: the adapter every command runs through.
    #[must_use]
    pub fn of<C: Command>() -> Self {
        Self {
            id: C::ID,
            label: C::LABEL,
            read_only: C::READ_ONLY,
            menu: C::MENU,
            enabled: C::enabled,
            params_schema: || schema_for!(C::Params),
            result_schema: || schema_for!(C::Result),
            execute: execute::<C>,
        }
    }

    /// Whether it can run now, or why not, as a menu says it: the dispatch's own check, a panic
    /// in it caught and said.
    ///
    /// # Errors
    ///
    /// Why it can't run now.
    pub fn state(&self, session: &Session) -> Result<(), String> {
        match catch_unwind(AssertUnwindSafe(|| (self.enabled)(session))) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(why)) => Err(why.to_string()),
            Err(_) => Err(format!("{}'s check failed", self.id)),
        }
    }

    #[must_use]
    pub fn params_schema(&self) -> Schema {
        (self.params_schema)()
    }

    #[must_use]
    pub fn result_schema(&self) -> Schema {
        (self.result_schema)()
    }

    /// Parses `params` (JSON text; empty or `null` for none), runs the command and serializes
    /// its result.
    ///
    /// # Errors
    ///
    /// Any [`EngineError`] but [`EngineError::UnknownCommand`].
    pub fn execute(&self, session: &mut Session, params: &str) -> Result<String, EngineError> {
        (self.execute)(session, params)
    }
}

/// `text` parsed as `P`: bounded, an empty text or `null` as `{}`, a bad field named by its path.
///
/// # Errors
///
/// [`EngineError::ParamsTooLarge`] or [`EngineError::BadParams`].
pub fn parse<P: DeserializeOwned>(text: &str) -> Result<P, EngineError> {
    if text.len() > MAX_PARAMS {
        return Err(EngineError::ParamsTooLarge { bytes: text.len(), limit: MAX_PARAMS });
    }
    let text = match text.trim() {
        "" | "null" => "{}",
        other => other,
    };
    let mut de = serde_json::Deserializer::from_str(text);
    let parsed = serde_path_to_error::deserialize(&mut de).map_err(|e| EngineError::BadParams { message: e.to_string() })?;
    de.end().map_err(|e| EngineError::BadParams { message: e.to_string() })?;
    Ok(parsed)
}

fn execute<C: Command>(session: &mut Session, params: &str) -> Result<String, EngineError> {
    let params = parse::<C::Params>(params)?;
    let result = call::<C>(session, params)?;
    serde_json::to_string(&result).map_err(|e| EngineError::BadResult { message: e.to_string() })
}

/// A call's cancel token (#180, swarm-core's docs/app-architecture.md › One dispatch): whoever
/// holds a clone can cancel it. A call whose token is set before it runs is
/// [`EngineError::Cancelled`] and never runs; work a command hands to a thread looks at it before
/// it starts (the background folder check, [`crate::repo_cmds::Open`]).
#[derive(Debug, Clone, Default)]
pub struct Cancel(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Cancel {
    /// A token not yet cancelled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels it, for every clone.
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether it was cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Checks `C` is enabled and runs it with typed params, catching a panic in either; a command
/// that isn't read-only moves the session's revision on when it succeeds.
///
/// # Errors
///
/// [`EngineError::Disabled`], [`EngineError::Internal`] for a caught panic, or the command's own
/// error.
pub fn call<C: Command>(session: &mut Session, params: C::Params) -> Result<C::Result, EngineError> {
    call_cancellable::<C>(session, params, &Cancel::new())
}

/// [`call`], with a cancel token: one set before the command runs is [`EngineError::Cancelled`],
/// and the command never runs.
///
/// # Errors
///
/// As [`call`], and [`EngineError::Cancelled`].
pub fn call_cancellable<C: Command>(session: &mut Session, params: C::Params, cancel: &Cancel) -> Result<C::Result, EngineError> {
    let enabled = catch_unwind(AssertUnwindSafe(|| C::enabled(session))).map_err(|_| EngineError::panicked(C::ID))?;
    enabled.map_err(|reason| EngineError::Disabled { command: C::ID.to_owned(), reason })?;
    if cancel.is_cancelled() {
        return Err(EngineError::Cancelled { command: C::ID.to_owned() });
    }
    let result = catch_unwind(AssertUnwindSafe(|| C::run(session, params))).map_err(|_| EngineError::panicked(C::ID))?;
    if result.is_ok() && !C::READ_ONLY {
        session.touch();
    }
    result
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static RAN: AtomicUsize = AtomicUsize::new(0);

    /// A command that panics, or counts its runs.
    struct Boom;

    #[derive(serde::Deserialize, schemars::JsonSchema)]
    struct BoomParams {
        #[serde(default)]
        panic: bool,
    }

    impl Command for Boom {
        const ID: &'static str = "test.boom";
        const LABEL: &'static str = "Boom";
        const READ_ONLY: bool = true;
        type Params = BoomParams;
        type Result = u32;

        fn run(_session: &mut Session, params: BoomParams) -> Result<u32, EngineError> {
            RAN.fetch_add(1, Ordering::SeqCst);
            if params.panic {
                panic!("a secret param");
            }
            Ok(1)
        }
    }

    /// A panic in a command's run is `internal`, its message `panicked: ` and the command, its
    /// remediation [`crate::error::PANICKED`], with nothing the panic said in it; the session goes
    /// on. On the wire the typed error carries the same.
    #[test]
    fn a_caught_panic_is_internal_with_the_contracts_remediation() {
        let mut session = Session::new();
        let error = call::<Boom>(&mut session, BoomParams { panic: true }).unwrap_err();
        assert_eq!(error.code(), "internal");
        assert!(error.to_string().starts_with("panicked: `test.boom`"), "{error}");
        assert!(!error.to_string().contains("secret"), "what the panic said stays on stderr");
        assert_eq!(error.remediation(), crate::error::PANICKED);
        let wire = serde_json::to_value(&error).unwrap();
        assert_eq!((wire["code"].as_str(), wire["remediation"].as_str()), (Some("internal"), Some(crate::error::PANICKED)));
        assert_eq!(call::<Boom>(&mut session, BoomParams { panic: false }).unwrap(), 1, "the session still works");
    }

    static COUNTED: AtomicUsize = AtomicUsize::new(0);

    /// A command that counts its runs, for this test alone.
    struct Counted;

    impl Command for Counted {
        const ID: &'static str = "test.counted";
        const LABEL: &'static str = "Counted";
        const READ_ONLY: bool = true;
        type Params = BoomParams;
        type Result = usize;

        fn run(_session: &mut Session, _params: BoomParams) -> Result<usize, EngineError> {
            Ok(COUNTED.fetch_add(1, Ordering::SeqCst) + 1)
        }
    }

    /// A call whose token is set before it runs is `cancelled`, and the command never runs.
    #[test]
    fn a_call_cancelled_before_it_runs_never_runs() {
        let mut session = Session::new();
        let cancel = Cancel::new();
        assert_eq!(session.call_cancellable::<Counted>(BoomParams { panic: false }, &cancel).unwrap(), 1);
        cancel.clone().cancel();
        assert!(cancel.is_cancelled(), "a clone cancels it for every holder");
        let error = session.call_cancellable::<Counted>(BoomParams { panic: false }, &cancel).unwrap_err();
        assert_eq!((error.code(), error.remediation().as_str()), ("cancelled", "ask again if it is still wanted"));
        assert_eq!(COUNTED.load(Ordering::SeqCst), 1, "it never ran");
    }

    /// Every error has a remediation.
    #[test]
    fn every_error_says_what_to_do() {
        let errors = [
            EngineError::UnknownCommand { command: "x".into() },
            EngineError::BadParams { message: "m".into() },
            EngineError::ParamsTooLarge { bytes: 9, limit: 8 },
            EngineError::Disabled { command: "x".into(), reason: crate::error::Unavailable::NoRepo },
            EngineError::Invalid { refusal: crate::error::Refusal::NotFound { what: "ticket #9".into() } },
            EngineError::panicked("x"),
            EngineError::Cancelled { command: "x".into() },
            EngineError::BadResult { message: "m".into() },
        ];
        for error in errors {
            assert!(!error.remediation().is_empty(), "{error:?}");
        }
    }
}
