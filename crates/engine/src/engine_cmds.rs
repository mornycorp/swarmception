//! The engine's own commands: what it can do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::Command;
use crate::commands;
use crate::error::EngineError;
use crate::session::Session;

/// No params.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoParams {}

/// One command, as `engine.commands` lists it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommandWire {
    pub id: String,
    pub label: String,
    pub read_only: bool,
    pub params: serde_json::Value,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandsResult {
    pub commands: Vec<CommandWire>,
}

/// `engine.commands`: every command with its params and result schemas.
pub struct Commands;

impl Command for Commands {
    const ID: &'static str = "engine.commands";
    const LABEL: &'static str = "List Commands";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = CommandsResult;

    fn run(_session: &mut Session, _params: NoParams) -> Result<CommandsResult, EngineError> {
        let commands = commands::all()
            .iter()
            .map(|e| CommandWire {
                id: e.id.to_owned(),
                label: e.label.to_owned(),
                read_only: e.read_only,
                params: e.params_schema().to_value(),
                result: e.result_schema().to_value(),
            })
            .collect();
        Ok(CommandsResult { commands })
    }
}
