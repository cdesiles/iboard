//! Holds the layering the refactoring builds: shell → domains → kernel →
//! platform, and the `gui` gate that keeps egui in the shell alone. `make
//! check` proves the gate with `cargo check --lib --no-default-features`.
//!
//! The shell is not extracted yet, so most of its files still sit at the crate
//! root. This test is the ratchet: classify a module when it moves, and a low
//! layer can never name a high one without turning it red.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Platform,
    Kernel,
    Domain,
    Shell,
}

/// The shared kernel and the aliases `lib.rs` keeps for its old paths.
const KERNEL: &[&str] = &["issue", "kernel"];

/// The platform layer until `git mv` puts it under `src/platform/`.
const PLATFORM: &[&str] = &[
    "files",
    "format",
    "git",
    "job",
    "markdown",
    "platform",
    "process",
    "scratch_git",
];

/// The domains already extracted or still at the crate root.
const DOMAINS: &[&str] = &[
    "agent",
    "agenda",
    "build_log",
    "commits",
    "config",
    "design",
    "diff",
    "discovery",
    "domains",
    "filter",
    "general",
    "history",
    "labels",
    "new_issue",
    "notifications",
    "plan",
    "preferences",
    "priority",
    "prompts",
    "prose",
    "refinement",
    "release",
    "reviews",
    "settings",
    "sheet",
    "stats",
    "titles",
    "trash",
    "workspace",
];

/// What names every domain, and the only layer allowed to draw or use egui.
const SHELL: &[&str] = &[
    "app",
    "assets",
    "bin",
    "design_ui",
    "dialog",
    "groups_ui",
    "instance",
    "label_picker",
    "main",
    "menu",
    "pasteboard",
    "project_menu",
    "recents",
    "settings_ui",
    "shell",
    "theme",
    "widgets",
];

const EGUI: &[&str] = &["egui::", "egui_", "eframe::", "use egui", "use eframe"];

impl Layer {
    fn of(module: &str) -> Option<Self> {
        if KERNEL.contains(&module) {
            Some(Self::Kernel)
        } else if PLATFORM.contains(&module) {
            Some(Self::Platform)
        } else if DOMAINS.contains(&module) {
            Some(Self::Domain)
        } else if SHELL.contains(&module) {
            Some(Self::Shell)
        } else {
            None
        }
    }
}

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn sources() -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(&src(), &mut files);
    files.sort();
    files
}

fn relative(file: &Path) -> String {
    file.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .display()
        .to_string()
}

/// The module a file belongs to: its first path component under `src`.
fn module(file: &Path) -> String {
    let relative = file.strip_prefix(src()).unwrap();
    relative
        .iter()
        .next()
        .unwrap()
        .to_str()
        .unwrap()
        .trim_end_matches(".rs")
        .to_string()
}

/// The text with its comments and string literals dropped, so a doc link or a
/// generator's template that names another layer is not a dependency.
fn code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i = block_comment(bytes, i);
            out.push(b' ');
        } else if bytes[i] == b'"' {
            i = quoted(bytes, i);
            out.push(b' ');
        } else if bytes[i] == b'\'' {
            if let Some(end) = character(bytes, i) {
                i = end;
                out.push(b' ');
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else if bytes[i] == b'r' {
            if let Some(end) = raw(bytes, i) {
                i = end;
                out.push(b' ');
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn block_comment(bytes: &[u8], mut i: usize) -> usize {
    i += 2;
    let mut depth = 1;
    while i < bytes.len() && depth > 0 {
        if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    i
}

fn quoted(bytes: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

/// A char literal's end, or `None` for a lifetime such as `'a`.
fn character(bytes: &[u8], i: usize) -> Option<usize> {
    if *bytes.get(i + 1)? == b'\\' {
        let mut j = i + 2;
        while j < bytes.len() && bytes[j] != b'\'' {
            j += 1;
        }
        return Some((j + 1).min(bytes.len()));
    }
    (*bytes.get(i + 2)? == b'\'').then_some(i + 3)
}

fn raw(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    let mut hashes = 0;
    while bytes.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    if bytes.get(j) != Some(&b'"') {
        return None;
    }
    j += 1;
    while j < bytes.len() {
        if bytes[j] == b'"' && (0..hashes).all(|k| bytes.get(j + 1 + k) == Some(&b'#')) {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    Some(j)
}

/// The first module of every `crate::…` and `iboard::…` path in `text`.
fn references(text: &str) -> BTreeSet<String> {
    let mut modules = BTreeSet::new();
    for marker in ["crate::", "iboard::"] {
        let mut rest = text;
        while let Some(at) = rest.find(marker) {
            let tail = &rest[at + marker.len()..];
            let (found, used) = names(tail);
            modules.extend(found);
            rest = &tail[used..];
        }
    }
    modules
}

/// The modules named by a path tail, and how much of it was read.
fn names(tail: &str) -> (Vec<String>, usize) {
    if let Some(inner) = tail.strip_prefix('{') {
        let Some(close) = inner.find('}') else {
            return (vec![], tail.len());
        };
        let found = inner[..close]
            .split(',')
            .filter_map(|item| {
                item.trim()
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()
            })
            .filter(|name| !name.is_empty() && !matches!(*name, "self" | "crate" | "super"))
            .map(str::to_string)
            .collect();
        return (found, close + 2);
    }
    let used = tail
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    let name = &tail[..used];
    let found = match name {
        "" | "self" | "crate" | "super" => vec![],
        _ => vec![name.to_string()],
    };
    (found, used)
}

fn text(file: &Path) -> String {
    code(&std::fs::read_to_string(file).unwrap())
}

#[test]
fn every_source_file_sits_in_a_layer() {
    let unknown: Vec<String> = sources()
        .iter()
        .filter_map(|file| {
            let module = module(file);
            (module != "lib" && Layer::of(&module).is_none())
                .then(|| format!("{} ({module})", relative(file)))
        })
        .collect();
    assert!(
        unknown.is_empty(),
        "classify these modules in tests/architecture.rs: {unknown:#?}"
    );
}

#[test]
fn a_lower_layer_names_no_higher_one() {
    let mut leaks = BTreeSet::new();
    for file in sources() {
        let module = module(&file);
        let Some(own) = Layer::of(&module) else {
            continue;
        };
        for target in references(&text(&file)) {
            match Layer::of(&target) {
                Some(theirs) if theirs > own => {
                    leaks.insert(format!(
                        "{} ({own:?}) names {target} ({theirs:?})",
                        relative(&file)
                    ));
                }
                None => {
                    leaks.insert(format!(
                        "{} names the unknown module {target}",
                        relative(&file)
                    ));
                }
                _ => {}
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "the layering is shell → domains → kernel → platform, so these go up:\n{}",
        leaks.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn only_the_shell_uses_egui() {
    let mut leaks = BTreeSet::new();
    for file in sources() {
        let module = module(&file);
        let Some(layer) = Layer::of(&module) else {
            continue;
        };
        if layer == Layer::Shell {
            continue;
        }
        let text = text(&file);
        for marker in EGUI {
            if text.contains(marker) {
                leaks.insert(format!("{} ({layer:?}) uses {marker:?}", relative(&file)));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "egui and the `gui` feature belong to the shell:\n{}",
        leaks.into_iter().collect::<Vec<_>>().join("\n")
    );
}
