//! The theme as data (#152): swarm-core's `ui.theme` (its docs/app-architecture.md › Colours from
//! the theme), here `theme.get`, as `ui.*` is the window's own: each token's name, meaning and
//! colour in each look, and each range's entries, so every surface can ask which colour a meaning
//! has; and `theme.set` (#154), the look the window draws in, which the timeline's colours are
//! built in.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swarmception_domain::theme::{self, Look, ThemeData};

use crate::command::Command;
use crate::engine_cmds::NoParams;
use crate::error::EngineError;
use crate::session::Session;

/// `theme.get`: the theme's tokens and ranges, dark and light.
pub struct Get;

impl Command for Get {
    const ID: &'static str = "theme.get";
    const LABEL: &'static str = "Theme Colours";
    const READ_ONLY: bool = true;
    type Params = NoParams;
    type Result = ThemeData;

    fn run(_session: &mut Session, _params: NoParams) -> Result<ThemeData, EngineError> {
        Ok(theme::data())
    }
}

/// `theme.set`'s params, and its answer: the look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookParams {
    /// `dark` or `light`.
    pub look: Look,
}

/// `theme.set`: the look the window draws in (#154). The timeline is built again in it when it
/// changes, its generation moved on; the same look changes nothing.
pub struct Set;

impl Command for Set {
    const ID: &'static str = "theme.set";
    const LABEL: &'static str = "Set the Look";
    const READ_ONLY: bool = false;
    type Params = LookParams;
    type Result = LookParams;

    fn run(session: &mut Session, params: LookParams) -> Result<LookParams, EngineError> {
        if session.look != params.look {
            session.look = params.look;
            session.rebuild();
        }
        Ok(LookParams { look: session.look })
    }
}
