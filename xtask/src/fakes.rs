//! `cargo xtask fakes` (#113), a step of `ci`: no test execs a file it just wrote.
//!
//! macOS vets a new executable the first time it runs, which took 0.3 s idle and 10–34 s under
//! load (#106), past a deadline once. So a fake `gh` or `ssh` is a script the system's `sh` runs,
//! never an executable of its own: `crates/engine/tests/support`'s `fake_gh` and `fake_ssh`, and
//! `crates/source/src/github.rs`'s test `sh_gh`. This check fails on what an exec'd fake needs: a
//! `#!` line in a Rust source, or a file made executable (a mode set with an execute bit, or one
//! it can't read). A line that must do it anyway carries the marker [`ALLOW`] and says why.

use std::path::{Path, PathBuf};

/// The command line, for `cargo xtask`'s usage.
pub const USAGE: &str = "cargo xtask fakes    no test writes a script with a #! line or makes a file executable (#113)";

/// The marker that lets one line through, with its reason after it.
pub const ALLOW: &str = "fakes: allowed";

/// The directories whose Rust sources are checked, under the workspace root.
const DIRS: &[&str] = &["apps", "crates", "xtask"];

/// This file: its examples are what it looks for.
const ITSELF: &str = "xtask/src/fakes.rs";

/// `cargo xtask fakes`: each offending line, and an error when there is one.
///
/// # Errors
///
/// An argument it doesn't take, a source it couldn't read, or a line that offends.
pub fn run(root: &Path, args: &[&str]) -> Result<(), String> {
    if let Some(arg) = args.first() {
        return Err(format!("fakes: unknown argument `{arg}`\n{USAGE}"));
    }
    let mut files = Vec::new();
    for dir in DIRS {
        sources(&root.join(dir), &mut files)?;
    }
    files.sort();
    let mut found = Vec::new();
    for file in &files {
        let shown = file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/");
        if shown == ITSELF {
            continue;
        }
        let text = std::fs::read_to_string(file).map_err(|e| format!("fakes: {shown}: {e}"))?;
        found.extend(offences(&text).into_iter().map(|(line, why)| format!("{shown}:{line}: {why}")));
    }
    if found.is_empty() {
        println!("fakes: no source writes a fake to exec ({} files)", files.len());
        return Ok(());
    }
    for line in &found {
        eprintln!("{line}");
    }
    Err(format!(
        "fakes: {} line(s) write a fake to exec. Run a fake through `sh` (crates/engine/tests/support's `fake_gh` and `fake_ssh`), or mark a line that must with `// {ALLOW}: <why>`",
        found.len()
    ))
}

/// The Rust sources under `dir`, build output and hidden directories left out.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("fakes: {}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("fakes: {}: {e}", dir.display()))?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if !name.starts_with('.') && !name.starts_with("target") {
                sources(&path, out)?;
            }
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Each line of `text` that writes a fake to exec: its number (from 1) and why.
fn offences(text: &str) -> Vec<(usize, &'static str)> {
    text.lines().enumerate().filter(|(_, line)| !line.contains(ALLOW)).filter_map(|(i, line)| offence(line).map(|why| (i + 1, why))).collect()
}

/// Why `line` writes a fake to exec, if it does.
fn offence(line: &str) -> Option<&'static str> {
    if line.contains(concat!("#!", "/")) {
        return Some("a script with a `#!` line, to exec: write it without one and run it with `sh`");
    }
    let sets_mode = ["set_mode(", "from_mode(", "set_permissions(", "OpenOptionsExt"].iter().any(|call| line.contains(call)) || sets_mode_on_open(line);
    if !sets_mode {
        return None;
    }
    let modes = octal_literals(line);
    if !modes.is_empty() && modes.iter().all(|mode| mode & 0o111 == 0) {
        return None;
    }
    Some("a file made executable (or a mode it can't read): run the fake with `sh` instead")
}

/// Whether `line` sets a mode with `OpenOptions::mode(…)`, not reads one with `mode()`.
fn sets_mode_on_open(line: &str) -> bool {
    line.match_indices(".mode(").any(|(at, call)| !line[at + call.len()..].starts_with(')'))
}

/// The octal literals on `line` (`0o755`, `0o4_755`), as numbers.
fn octal_literals(line: &str) -> Vec<u32> {
    line.match_indices("0o")
        .filter(|(at, _)| !line[..*at].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
        .filter_map(|(at, _)| {
            let digits: String = line[at + 2..].chars().take_while(|c| matches!(c, '0'..='7' | '_')).filter(|c| *c != '_').collect();
            u32::from_str_radix(&digits, 8).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exec_d_fake_is_found() {
        let fake = r##"
    let gh = dir.join("gh");
    std::fs::write(&gh, "#!/bin/sh\ncat pages.json\n").unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o700);
    std::fs::set_permissions(&gh, perms).unwrap();
    std::fs::OpenOptions::new().write(true).mode(0o775).open(&gh).unwrap();
    std::fs::OpenOptions::new().write(true).mode(mode).open(&gh).unwrap();
"##;
        let lines: Vec<usize> = offences(fake).into_iter().map(|(line, _)| line).collect();
        assert_eq!(lines, [3, 4, 5, 6, 7, 8], "the #! line, each executable mode, and each mode it can't read");
    }

    #[test]
    fn what_execs_nothing_new_passes() {
        let fine = r#"
    #![allow(dead_code)]
    std::fs::write(repo.join("api"), "cat pages.json\n").unwrap();
    git(&clone, &["config", "core.sshCommand", &format!("sh '{}'", ssh.display())]);
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o077, 0);
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::OpenOptions::new().write(true).mode(0o6_00).open(&token).unwrap();
    let foo0o755 = 1;
    std::fs::set_permissions(&gh, perms).unwrap(); // fakes: allowed: read-only, never exec'd
"#;
        assert_eq!(offences(fine), []);
    }

    #[test]
    fn octal_literals_are_read_whole() {
        assert_eq!(octal_literals("from_mode(0o755) | 0o4_000, x0o7, 0o"), [0o755, 0o4000]);
    }

    #[test]
    fn this_workspace_writes_no_fake_to_exec() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(Path::to_path_buf).unwrap_or_default();
        assert_eq!(run(&root, &[]), Ok(()));
        assert!(run(&root, &["--fix"]).is_err_and(|e| e.contains("unknown argument")));
    }
}
