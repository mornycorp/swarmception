//! `cargo xtask writes` (#164), a step of `ci`: only the layer that touches the outside writes a
//! file or starts a program (swarm-core's docs/app-architecture.md › Layers and State on disk).
//! Swarmception's is `swarmception-source`: its one atomic writer (`crates/source/src/files.rs`)
//! and its one place a program starts (`crates/source/src/process.rs`). This check fails on a file
//! write (`fs::write`, `File::create`, `OpenOptions`, a rename, a directory made, a file removed or
//! copied, permissions set) or a process start (`Command::new`) in the non-test code of any other
//! crate or app: everything else writes and runs through those two. Tests (`tests/`, a module after
//! `#[cfg(test)]`) are left out, as is `xtask`, the swarm's tool. A line that must anyway carries the
//! marker [`ALLOW`] and says why.

use std::path::{Path, PathBuf};

/// The command line, for `cargo xtask`'s usage.
pub const USAGE: &str = "cargo xtask writes   only crates/source's files and process modules write a file or start a program (#164)";

/// The marker that lets one line through, with its reason after it.
pub const ALLOW: &str = "writes: allowed";

/// The directories whose Rust sources are checked, under the workspace root.
const DIRS: &[&str] = &["apps", "crates"];

/// The two modules that may: the one writer and the one place a program starts.
const THE_ONES: &[&str] = &["crates/source/src/files.rs", "crates/source/src/process.rs"];

/// What writes a file or starts a program.
const CALLS: &[(&str, &str)] = &[
    ("fs::write(", "a file written"),
    ("File::create(", "a file created"),
    ("OpenOptions", "a file opened to write"),
    ("fs::rename(", "a file renamed"),
    ("create_dir_all(", "a directory made"),
    ("fs::create_dir(", "a directory made"),
    ("fs::remove_file(", "a file removed"),
    ("fs::remove_dir", "a directory removed"),
    ("fs::copy(", "a file copied"),
    ("set_permissions(", "a file's permissions set"), // fakes: allowed: the pattern this check looks for
    ("Command::new(", "a program started"),
];

/// `cargo xtask writes`: each offending line, and an error when there is one.
///
/// # Errors
///
/// An argument it doesn't take, a source it couldn't read, or a line that offends.
pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    if let Some(arg) = args.first() {
        return Err(format!("writes: unknown argument `{arg}`\n{USAGE}"));
    }
    let mut files = Vec::new();
    for dir in DIRS {
        sources(&root.join(dir), &mut files)?;
    }
    files.sort();
    let mut found = Vec::new();
    for file in &files {
        let shown = file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/");
        if THE_ONES.contains(&shown.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(file).map_err(|e| format!("writes: {shown}: {e}"))?;
        found.extend(offences(&text).into_iter().map(|(line, why)| format!("{shown}:{line}: {why}")));
    }
    if found.is_empty() {
        println!("writes: only {} write a file or start a program ({} files)", THE_ONES.join(" and "), files.len());
        return Ok(());
    }
    for line in &found {
        eprintln!("{line}");
    }
    Err(format!(
        "writes: {} line(s) write a file or start a program outside the layer that touches the outside. Use \
         swarmception_source::files (write_atomic, write_private) or swarmception_source::process::Program, or mark a \
         line that must with `// {ALLOW}: <why>`",
        found.len()
    ))
}

/// The Rust sources under `dir` that aren't tests: build output, hidden directories and `tests/`
/// left out.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("writes: {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("writes: {}: {e}", dir.display()))?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && !name.starts_with("target") && name != "tests" {
                sources(&path, out)?;
            }
        } else if name.ends_with(".rs") && name != "tests.rs" {
            out.push(path);
        }
    }
    Ok(())
}

/// Each line of `text`'s non-test code (up to its first `#[cfg(test)]`) that writes a file or
/// starts a program: its number (from 1) and what.
fn offences(text: &str) -> Vec<(usize, &'static str)> {
    text.lines()
        .take_while(|line| line.trim() != "#[cfg(test)]")
        .enumerate()
        .filter(|(_, line)| !line.contains(ALLOW) && !line.trim_start().starts_with("//"))
        .filter_map(|(i, line)| CALLS.iter().find(|(call, _)| line.contains(call)).map(|(_, why)| (i + 1, *why)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_write_or_a_program_outside_the_ones_is_found() {
        let code = r#"
    std::fs::write(&path, text)?;
    let file = std::fs::File::create(path)?;
    let mut options = std::fs::OpenOptions::new();
    std::fs::rename(&temporary, file)?;
    std::fs::create_dir_all(parent)?;
    let child = std::process::Command::new("osascript").spawn()?;
    // std::fs::write in a comment is fine
    std::fs::write(&path, text)?; // writes: allowed: a reason
    let text = std::fs::read_to_string(&path)?;
#[cfg(test)]
mod tests {
    std::fs::write(&scratch, "a test's").unwrap();
}
"#;
        let lines: Vec<usize> = offences(code).into_iter().map(|(line, _)| line).collect();
        assert_eq!(lines, [2, 3, 4, 5, 6, 7], "each write and the program; not a comment, an allowed line, a read or a test");
    }

    #[test]
    fn this_workspace_writes_only_through_the_ones() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default();
        assert_eq!(run(&root, &[]), Ok(()));
        assert!(run(&root, &["--fix"]).is_err_and(|e| e.contains("unknown argument")));
    }
}
