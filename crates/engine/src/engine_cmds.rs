//! The engine's own commands: what it can do.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::command::{Ask, Command, MenuPlace};
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
    /// Where it sits in the menus (#160); none when a person doesn't run it from there.
    pub menu: Option<MenuWire>,
}

/// A command's place in the menus, as `engine.commands` gives it (#160).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MenuWire {
    /// Its menu, then any submenu.
    pub path: Vec<String>,
    pub label: String,
    /// Groups in order, a separator between them.
    pub group: u8,
    pub shortcut: Option<ShortcutWire>,
    /// The param the front end fills with a folder the person picks, if it asks for one.
    pub asks_folder: Option<String>,
}

/// A shortcut: the platform's command key (⌘, Ctrl), Shift or not, and a letter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShortcutWire {
    pub shift: bool,
    pub key: String,
}

impl MenuWire {
    #[must_use]
    pub fn of(place: &MenuPlace) -> Self {
        Self {
            path: place.path.iter().map(|p| (*p).to_owned()).collect(),
            label: place.label.to_owned(),
            group: place.group,
            shortcut: place.shortcut.map(|s| ShortcutWire { shift: s.shift, key: s.key.to_string() }),
            asks_folder: place.asks.map(|Ask::Folder { param }| param.to_owned()),
        }
    }
}

/// Whether a command can run now, as `engine.states` gives it: a menu greys out what can't, saying
/// why (#160).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommandState {
    pub id: String,
    pub enabled: bool,
    /// Why it can't, when it can't.
    pub reason: Option<String>,
}

/// `engine.states`' answer: every command's, in the registry's order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StatesResult {
    pub states: Vec<CommandState>,
}

/// `engine.states`: whether each command can run now, and why not (the dispatch's own check), for
/// a front end to grey out its menus with the reason (#160).
pub struct States;

impl Command for States {
    const ID: &'static str = "engine.states";
    const LABEL: &'static str = "Command States";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = StatesResult;

    fn run(session: &mut Session, _params: NoParams) -> Result<StatesResult, EngineError> {
        Ok(StatesResult { states: session.states() })
    }
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
                menu: e.menu.as_ref().map(MenuWire::of),
            })
            .collect();
        Ok(CommandsResult { commands })
    }
}
