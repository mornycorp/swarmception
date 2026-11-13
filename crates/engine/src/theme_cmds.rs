//! The theme as data (#152): swarm-core's `ui.theme` (its docs/app-architecture.md › Colours from
//! the theme), here `theme.get`, as `ui.*` is the window's own: each token's name, meaning and
//! colour in each look, and each range's entries, so every surface can ask which colour a meaning
//! has.

use swarmception_domain::theme::{self, ThemeData};

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
