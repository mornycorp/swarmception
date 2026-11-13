//! Where each part of the window keeps its egui state (#163, swarm-core's docs/app-architecture.md
//! › Colours from the theme, words from the catalog: an id the toolkit keeps state under is a typed
//! salt, an enum's variant, never a string). A part drawn many times carries its data beside it in
//! the salt: `(Part::FactoryChip, issue, n)`. A test holds the crate's sources to it.

/// A part of the window egui keeps state for: a panel, a window, a combo box, an area that takes
/// clicks or hovers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Part {
    MenuBar,
    Toolbar,
    StatusBar,
    /// The details pane, and its strip when folded.
    Details,
    DetailsStrip,
    /// A band under the timeline (#68), and the edge that drags its height.
    Band(crate::BandKind),
    BandEdge(crate::BandKind),
    LiveInterval,
    FactorySpeed,
    FactorySteps,
    /// The factory's keyboard focus.
    FactoryKeys,
    FactoryFloor,
    /// A brick, a gate run, a chip: each with its own data beside it.
    FactoryBlock,
    FactoryRun,
    FactoryChip,
    BenchesFolded,
    IdleBenches,
    BenchTip,
    HelperTip,
    DetailsEntry,
    WorkersWindow,
    WorkersHeader,
    OpenRefused,
    OpenPrompt,
}

impl Part {
    /// Its egui id.
    #[must_use]
    pub(crate) fn id(self) -> egui::Id {
        egui::Id::unique(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No source of the crate keys egui's state by a string: a panel, a combo box, a window or an
    /// `Id`'s salt (#163).
    #[test]
    fn no_id_is_a_string() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let strings = [
            "Panel::top(\"",
            "Panel::bottom(\"",
            "Panel::left(\"",
            "Panel::right(\"",
            "from_id_salt(\"",
            "id_salt(\"",
            "Id::new(\"",
            "Id::unique(\"",
            ".with(\"",
            ".with((\"",
            "push_id(\"",
            "make_persistent_id(\"",
        ];
        let mut found = Vec::new();
        for entry in std::fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == "part.rs") || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.trim_start().starts_with("//")) {
                if strings.iter().any(|s| line.contains(s)) {
                    found.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
                }
            }
        }
        assert!(found.is_empty(), "ids keyed by a string:\n{}", found.join("\n"));
        assert_ne!(Part::Band(crate::BandKind::Factory).id(), Part::BandEdge(crate::BandKind::Factory).id());
    }
}
