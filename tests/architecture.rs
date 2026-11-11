//! Holds the layering the refactoring builds: shell → domains → kernel →
//! platform, the domain edges `docs/domains.md` allows, and the `gui` gate that
//! keeps egui in the shell alone. `make check` proves the gate with `cargo
//! clippy --lib --no-default-features`.
//!
//! The shell sits under `src/shell/`; `main.rs`, the binaries and the panels
//! still at the crate root are shell by the same rule. A domain owns its folder
//! under `src/domains/`, and the root alias `lib.rs` keeps for an old path
//! belongs to the code that now owns it. This test is the ratchet: classify a
//! module when it moves, and a low layer can never name a high one, nor a
//! domain name another outside the edges, without turning it red.

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

/// What a module belongs to: its layer, and its domain when it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Platform,
    Kernel,
    /// A folder under `src/domains/`; `None` for the tree's own `mod.rs`.
    Domain(Option<&'static str>),
    Shell,
}

impl Owner {
    fn layer(self) -> Layer {
        match self {
            Self::Platform => Layer::Platform,
            Self::Kernel => Layer::Kernel,
            Self::Domain(_) => Layer::Domain,
            Self::Shell => Layer::Shell,
        }
    }
}

/// The domains, by the folder each owns under `src/domains/`.
const DOMAINS: &[&str] = &[
    "agent",
    "authoring",
    "board",
    "delivery",
    "design",
    "insights",
    "onboarding",
    "preferences",
    "workspace",
];

/// The root aliases `lib.rs` keeps for the old paths: the first segment a
/// domain writes, and the code that owns it now.
const ALIASES: &[(&str, Owner)] = &[
    ("agent", Owner::Domain(Some("agent"))),
    ("build_log", Owner::Domain(Some("agent"))),
    ("plan", Owner::Domain(Some("agent"))),
    ("prompts", Owner::Domain(Some("agent"))),
    ("refinement", Owner::Domain(Some("agent"))),
    ("reviews", Owner::Domain(Some("agent"))),
    ("titles", Owner::Domain(Some("agent"))),
    ("diff", Owner::Domain(Some("authoring"))),
    ("new_issue", Owner::Domain(Some("authoring"))),
    ("prose", Owner::Domain(Some("authoring"))),
    ("sheet", Owner::Domain(Some("authoring"))),
    ("commits", Owner::Domain(Some("board"))),
    ("config", Owner::Domain(Some("board"))),
    ("discovery", Owner::Domain(Some("board"))),
    ("filter", Owner::Domain(Some("board"))),
    ("history", Owner::Domain(Some("board"))),
    ("labels", Owner::Domain(Some("board"))),
    ("priority", Owner::Domain(Some("board"))),
    ("trash", Owner::Domain(Some("board"))),
    ("release", Owner::Domain(Some("delivery"))),
    ("design", Owner::Domain(Some("design"))),
    ("agenda", Owner::Domain(Some("insights"))),
    ("stats", Owner::Domain(Some("insights"))),
    ("onboarding", Owner::Domain(Some("onboarding"))),
    ("general", Owner::Domain(Some("preferences"))),
    ("notifications", Owner::Domain(Some("preferences"))),
    ("preferences", Owner::Domain(Some("preferences"))),
    ("settings", Owner::Domain(Some("preferences"))),
    ("workspace", Owner::Domain(Some("workspace"))),
    ("issue", Owner::Kernel),
    ("files", Owner::Platform),
    ("format", Owner::Platform),
    ("git", Owner::Platform),
    ("job", Owner::Platform),
    ("markdown", Owner::Platform),
    ("process", Owner::Platform),
];

/// The modules at the crate root that are not aliases.
const ROOT: &[(&str, Owner)] = &[
    ("bin", Owner::Shell),
    ("design_ui", Owner::Shell),
    ("main", Owner::Shell),
    ("pasteboard", Owner::Shell),
    ("scratch_git", Owner::Platform),
    ("settings_ui", Owner::Shell),
    ("shell", Owner::Shell),
];

/// The only domain-to-domain names the map allows, one row per edge.
const EDGES: &[(&str, &str)] = &[
    ("preferences", "agent"),
    ("preferences", "board"),
    ("preferences", "delivery"),
    ("preferences", "insights"),
    ("preferences", "workspace"),
    ("workspace", "board"),
    ("agent", "board"),
    ("agent", "design"),
    ("delivery", "board"),
    ("insights", "board"),
    ("insights", "agent"),
    ("onboarding", "board"),
];

const EGUI: &[&str] = &["egui::", "egui_", "eframe::", "use egui", "use eframe"];

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

/// The owner of a root module or alias named `name`.
fn root_owner(name: &str) -> Option<Owner> {
    match name {
        "domains" => Some(Owner::Domain(None)),
        "kernel" => Some(Owner::Kernel),
        "platform" => Some(Owner::Platform),
        _ => ALIASES
            .iter()
            .chain(ROOT)
            .find(|(module, _)| *module == name)
            .map(|(_, owner)| *owner),
    }
}

/// The code a file belongs to, from its path under `src`.
fn owner_of_file(file: &Path) -> Option<Owner> {
    let relative = file.strip_prefix(src()).unwrap();
    let parts: Vec<&str> = relative.iter().map(|part| part.to_str().unwrap()).collect();
    match parts.as_slice() {
        ["lib.rs"] => None,
        ["domains", "mod.rs"] => Some(Owner::Domain(None)),
        ["domains", domain, ..] => domain_owner(domain),
        [module, ..] => root_owner(module.trim_end_matches(".rs")),
        [] => None,
    }
}

fn domain_owner(name: &str) -> Option<Owner> {
    DOMAINS
        .iter()
        .copied()
        .find(|domain| *domain == name)
        .map(|domain| Owner::Domain(Some(domain)))
}

/// The code a `crate::…` or `iboard::…` path names.
fn owner_of_path(path: &str) -> Option<Owner> {
    let mut segments = path.split("::");
    let first = segments.next()?;
    if first != "domains" {
        return root_owner(first);
    }
    match segments.next() {
        None => Some(Owner::Domain(None)),
        Some(domain) => domain_owner(domain),
    }
}

fn in_domains(file: &Path) -> bool {
    file.starts_with(src().join("domains"))
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

/// The code paths named by every `crate::…` and `iboard::…` in `text`, as far
/// as owning one needs: a segment, or two under `domains`.
fn references(text: &str) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for marker in ["crate::", "iboard::"] {
        let mut rest = text;
        while let Some(at) = rest.find(marker) {
            let tail = &rest[at + marker.len()..];
            let (found, used) = names(tail);
            paths.extend(found);
            rest = &tail[used..];
        }
    }
    paths
}

/// The paths named by a path tail, and how much of it was read. A brace group
/// yields one path per item, and `domains::{…}` keeps its first segment.
fn names(tail: &str) -> (Vec<String>, usize) {
    if let Some(inner) = tail.strip_prefix('{') {
        let Some(close) = inner.find('}') else {
            return (vec![], tail.len());
        };
        let found = inner[..close]
            .split(',')
            .flat_map(|item| names(item.trim()).0)
            .collect();
        return (found, close + 2);
    }
    let used = tail
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    let head = &tail[..used];
    if head.is_empty() || matches!(head, "self" | "crate" | "super") {
        return (vec![], used);
    }
    if head == "domains" && tail[used..].starts_with("::") {
        let (found, deeper) = names(&tail[used + 2..]);
        let found = found
            .into_iter()
            .map(|name| format!("domains::{name}"))
            .collect();
        return (found, used + 2 + deeper);
    }
    (vec![head.to_string()], used)
}

fn text(file: &Path) -> String {
    code(&std::fs::read_to_string(file).unwrap())
}

#[test]
fn every_source_file_sits_in_a_layer() {
    let unknown: Vec<String> = sources()
        .iter()
        .filter(|file| **file != src().join("lib.rs"))
        .filter(|file| owner_of_file(file).is_none())
        .map(|file| relative(file))
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
        let Some(own) = owner_of_file(&file) else {
            continue;
        };
        for path in references(&text(&file)) {
            match owner_of_path(&path) {
                Some(theirs) if theirs.layer() > own.layer() => {
                    leaks.insert(format!(
                        "{} ({:?}) names {path} ({:?})",
                        relative(&file),
                        own.layer(),
                        theirs.layer()
                    ));
                }
                None => {
                    leaks.insert(format!(
                        "{} names the unknown module {path}",
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
fn domains_only_name_the_edges_the_map_allows() {
    let mut leaks = BTreeSet::new();
    for file in sources() {
        let Some(Owner::Domain(Some(from))) = owner_of_file(&file) else {
            continue;
        };
        for path in references(&text(&file)) {
            let Some(Owner::Domain(Some(to))) = owner_of_path(&path) else {
                continue;
            };
            if from != to && !EDGES.contains(&(from, to)) {
                leaks.insert(format!(
                    "{} ({from}) names the domain {to}",
                    relative(&file)
                ));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "docs/domains.md allows these domain edges only:\n{}",
        leaks.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn domains_import_from_the_owner_not_a_root_alias() {
    let mut leaks = BTreeSet::new();
    for file in sources() {
        if !in_domains(&file) {
            continue;
        }
        for path in references(&text(&file)) {
            let first = path.split("::").next().unwrap_or_default();
            if ALIASES.iter().any(|(name, _)| *name == first) {
                leaks.insert(format!(
                    "{} names the root alias crate::{path}",
                    relative(&file)
                ));
            }
        }
    }
    assert!(
        leaks.is_empty(),
        "import from kernel, platform or the owning domain directly:\n{}",
        leaks.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn only_the_shell_uses_egui() {
    let mut leaks = BTreeSet::new();
    for file in sources() {
        let Some(owner) = owner_of_file(&file) else {
            continue;
        };
        let layer = owner.layer();
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
