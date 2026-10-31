//! A command: an id, a label, a typed params type and result type with their JSON Schemas, an
//! `enabled` check, and `run`. The one adapter, [`Entry::of`], erases its types for the
//! registry: it bounds and parses the params, catches a panic in `enabled` or `run`, and
//! serializes the result, so no command reads raw JSON.

use std::panic::{AssertUnwindSafe, catch_unwind};

use schemars::{JsonSchema, Schema, schema_for};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{EngineError, Unavailable};
use crate::session::Session;

/// The largest params text a command takes.
pub const MAX_PARAMS: usize = 1024 * 1024;

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
            params_schema: || schema_for!(C::Params),
            result_schema: || schema_for!(C::Result),
            execute: execute::<C>,
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

/// Checks `C` is enabled and runs it with typed params, catching a panic in either; a command
/// that isn't read-only moves the session's revision on when it succeeds.
///
/// # Errors
///
/// [`EngineError::Disabled`], [`EngineError::Panicked`], or the command's own error.
pub fn call<C: Command>(session: &mut Session, params: C::Params) -> Result<C::Result, EngineError> {
    let enabled = catch_unwind(AssertUnwindSafe(|| C::enabled(session))).map_err(|_| EngineError::Panicked { command: C::ID.to_owned() })?;
    enabled.map_err(|reason| EngineError::Disabled { command: C::ID.to_owned(), reason })?;
    let result = catch_unwind(AssertUnwindSafe(|| C::run(session, params))).map_err(|_| EngineError::Panicked { command: C::ID.to_owned() })?;
    if result.is_ok() && !C::READ_ONLY {
        session.touch();
    }
    result
}
