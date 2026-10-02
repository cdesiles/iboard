//! Locating the roadmap directory and turning its folders into kanban columns.
//!
//! Because layouts differ between projects, discovery is deliberately lenient:
//! any known column name (`todo`, `doing`, `done`, ...) is always shown, any
//! other sub-directory is shown only if it directly contains markdown files,
//! and markdown files sitting directly in the roadmap root form the first
//! column. A `roadmap-viewer.toml` can override all of this.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::Config;
use crate::history;
use crate::ticket::Ticket;

/// A resolved project: where the roadmap lives and how it was configured.
#[derive(Clone)]
pub struct Located {
    pub project_dir: PathBuf,
    pub root: PathBuf,
    pub config: Option<Config>,
}

pub struct Column {
    pub name: String,
    /// The directory holding this column's tickets. Dropping a card here moves
    /// its markdown file into this directory.
    pub dir: PathBuf,
    pub tickets: Vec<Ticket>,
}

impl Column {
    /// Whether the "+" / new-ticket affordance is offered here. New tickets
    /// start life in the todo column.
    pub fn accepts_new_tickets(&self) -> bool {
        self.name.eq_ignore_ascii_case("todo")
    }
}

pub struct Roadmap {
    pub project_dir: PathBuf,
    pub root: PathBuf,
    pub title: Option<String>,
    pub columns: Vec<Column>,
    /// Non-fatal problems encountered while scanning (unreadable files, ...).
    pub warnings: Vec<String>,
}

impl Roadmap {
    /// An empty roadmap, used when a scan fails so the UI can still render.
    pub fn empty(located: &Located) -> Self {
        Self {
            project_dir: located.project_dir.clone(),
            root: located.root.clone(),
            title: located.config.as_ref().and_then(|c| c.title.clone()),
            columns: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn find(&self, path: &Path) -> Option<&Ticket> {
        self.columns
            .iter()
            .flat_map(|column| column.tickets.iter())
            .find(|ticket| ticket.path == path)
    }

    pub fn ticket_count(&self) -> usize {
        self.columns.iter().map(|c| c.tickets.len()).sum()
    }
}

/// Walks up from `start` looking for a configured project or a `roadmap/`
/// directory. Running from inside the roadmap (or any sub-folder) also works.
pub fn locate(start: &Path) -> Result<Located> {
    let start = if start.is_file() {
        start.parent().unwrap_or(start)
    } else {
        start
    };
    let origin = start.to_path_buf();
    let mut dir = origin.clone();

    loop {
        if let Some(loaded) = crate::config::load(&dir) {
            let root = match loaded.config.root.as_deref() {
                Some(relative) => dir.join(relative),
                None => dir.join("roadmap"),
            };
            if !root.is_dir() {
                bail!(
                    "config {} points to `{}`, which is not a directory",
                    loaded.path.display(),
                    root.display()
                );
            }
            return Ok(Located {
                project_dir: dir,
                root,
                config: Some(loaded.config),
            });
        }

        let candidate = dir.join("roadmap");
        if candidate.is_dir() {
            return Ok(Located {
                project_dir: dir,
                root: candidate,
                config: None,
            });
        }

        if dir.file_name().and_then(|n| n.to_str()) == Some("roadmap") && dir.is_dir() {
            let project_dir = dir.parent().unwrap_or(&dir).to_path_buf();
            return Ok(Located {
                project_dir,
                root: dir,
                config: None,
            });
        }

        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => bail!(
                "no `roadmap/` directory found from `{}` (and no parent contains one)",
                origin.display()
            ),
        }
    }
}

/// Scans the roadmap directory into columns.
pub fn build(located: &Located) -> Result<Roadmap> {
    let root = &located.root;
    let config = located.config.as_ref();
    let mut warnings = Vec::new();

    // 1. Establish the ordered list of sub-directory columns.
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(columns) = config.and_then(|c| c.columns.as_ref()) {
        for name in columns {
            let dir = root.join(name);
            if dir.is_dir() {
                dirs.push(dir);
            } else {
                warnings.push(format!(
                    "configured column `{name}` not found at {}",
                    dir.display()
                ));
            }
        }
    }

    if dirs.is_empty() {
        let entries = fs::read_dir(root)
            .with_context(|| format!("cannot read roadmap directory {}", root.display()))?;
        let mut subs: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && !is_hidden(path))
            .filter(|path| is_known_column(path) || contains_markdown(path))
            .collect();
        subs.sort_by(|a, b| {
            column_rank(a).cmp(&column_rank(b)).then_with(|| {
                display_name(a)
                    .to_lowercase()
                    .cmp(&display_name(b).to_lowercase())
            })
        });
        dirs = subs;
    }

    // 2. Files sitting directly in the roadmap root form the first column.
    let mut columns = Vec::new();
    let root_files = collect_markdown(root, &mut warnings);
    if !root_files.is_empty() {
        let has_todo_dir = dirs.iter().any(|dir| column_rank(dir) == 1);
        let name = config
            .and_then(|c| c.root_column_name.clone())
            .unwrap_or_else(|| {
                if has_todo_dir {
                    "Backlog".to_string()
                } else {
                    "Todo".to_string()
                }
            });
        columns.push(Column {
            name,
            dir: root.clone(),
            tickets: load_tickets(root_files, &mut warnings),
        });
    }

    // 3. One column per sub-directory.
    for dir in &dirs {
        let files = collect_markdown(dir, &mut warnings);
        let mut tickets = load_tickets(files, &mut warnings);
        match canonical_name(dir) {
            Some("Done") => sort_recent_first(dir, &mut tickets, true),
            Some("Doing") => sort_recent_first(dir, &mut tickets, false),
            _ => {}
        }
        columns.push(Column {
            name: display_name(dir),
            dir: dir.clone(),
            tickets,
        });
    }

    Ok(Roadmap {
        project_dir: located.project_dir.clone(),
        root: root.clone(),
        title: config.and_then(|c| c.title.clone()),
        columns,
        warnings,
    })
}

/// Puts the most recently edited tickets first (Doing and Done), and with
/// `show_date` keeps the date on each card. Tickets with the same date (one
/// commit) go in reverse file-name order, so numbered tickets stay newest
/// first too.
fn sort_recent_first(dir: &Path, tickets: &mut [Ticket], show_date: bool) {
    let paths: Vec<PathBuf> = tickets.iter().map(|t| t.path.clone()).collect();
    let dates = history::done_dates(dir, &paths);
    tickets.reverse();
    tickets.sort_by_key(|t| std::cmp::Reverse(dates.get(&t.path).copied()));
    if show_date {
        for ticket in tickets.iter_mut() {
            ticket.done_at = dates.get(&ticket.path).copied();
        }
    }
}

/// Whether `dir` is the Ready column, whose tickets carry a priority prefix.
pub fn is_ready_column(dir: &Path) -> bool {
    canonical_name(dir) == Some("Ready")
}

/// Whether a column's order is its file-name order, so dropping a card at a
/// position can renumber the files. Todo and Ready; Doing and Done are sorted
/// by date instead.
pub fn is_orderable_column(dir: &Path) -> bool {
    matches!(canonical_name(dir), Some("Todo" | "Ready"))
}

/// The width of the priority prefix in Ready: `001-make-colour-green.md`.
const PRIORITY_DIGITS: usize = 3;

/// The file name without its Ready priority prefix, if it has one.
pub fn strip_priority(name: &str) -> &str {
    let bytes = name.as_bytes();
    let numbered = bytes.len() > PRIORITY_DIGITS
        && bytes[..PRIORITY_DIGITS].iter().all(u8::is_ascii_digit)
        && bytes[PRIORITY_DIGITS] == b'-';
    if numbered {
        &name[PRIORITY_DIGITS + 1..]
    } else {
        name
    }
}

/// The name a ticket takes when moved from one folder into another: entering
/// Ready numbers it after the last ticket there, leaving Ready drops the
/// number. Other moves keep the name.
pub fn moved_name(name: &str, from_dir: &Path, to_dir: &Path) -> String {
    if is_ready_column(to_dir) {
        let last = fs::read_dir(to_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| priority(&entry.file_name().to_string_lossy()))
            .max()
            .unwrap_or(0);
        format!(
            "{:0width$}-{}",
            last + 1,
            strip_priority(name),
            width = PRIORITY_DIGITS
        )
    } else if is_ready_column(from_dir) {
        strip_priority(name).to_string()
    } else {
        name.to_string()
    }
}

/// The priority number of a Ready file name: `001-x.md` → 1.
fn priority(name: &str) -> Option<u32> {
    let rest = strip_priority(name);
    if rest.len() == name.len() {
        return None;
    }
    name[..PRIORITY_DIGITS].parse().ok()
}

/// The `001-x.md` name for `name` at a one-based position, dropping any prefix
/// it already has so renumbering is stable.
pub fn numbered_name(position: usize, name: &str) -> String {
    format!(
        "{:0width$}-{}",
        position,
        strip_priority(name),
        width = PRIORITY_DIGITS
    )
}

fn load_tickets(files: Vec<PathBuf>, warnings: &mut Vec<String>) -> Vec<Ticket> {
    let mut tickets = Vec::with_capacity(files.len());
    for path in files {
        match Ticket::load(&path) {
            Ok(ticket) => tickets.push(ticket),
            Err(err) => warnings.push(format!("cannot read {}: {err}", path.display())),
        }
    }
    tickets
}

fn collect_markdown(dir: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            warnings.push(format!("cannot read {}: {err}", dir.display()));
            return Vec::new();
        }
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && !is_hidden(path) && is_markdown(path))
        .collect();
    files.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)));
    files
}

/// Maps a directory name to its canonical title and sort rank.
fn canonical(name: &str) -> Option<(&'static str, u8)> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "backlog" | "icebox" => Some(("Backlog", 0)),
        "todo" | "to-do" | "to_do" => Some(("Todo", 1)),
        "ready" => Some(("Ready", 2)),
        "doing" | "in-progress" | "in_progress" | "inprogress" | "wip" => Some(("Doing", 3)),
        "review" => Some(("Review", 4)),
        "later" => Some(("Later", 5)),
        "blocked" => Some(("Blocked", 6)),
        "done" | "completed" => Some(("Done", 7)),
        _ => None,
    }
}

fn display_name(dir: &Path) -> String {
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("Column");
    match canonical(raw) {
        Some((name, _)) => name.to_string(),
        None => title_case(raw),
    }
}

fn is_known_column(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(|name| canonical(name).is_some())
        .unwrap_or(false)
}

fn column_rank(dir: &Path) -> u8 {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(canonical)
        .map(|(_, rank)| rank)
        .unwrap_or(u8::MAX)
}

fn canonical_name(dir: &Path) -> Option<&'static str> {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(canonical)
        .map(|(name, _)| name)
}

fn contains_markdown(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .any(|entry| entry.path().is_file() && is_markdown(&entry.path()))
        })
        .unwrap_or(false)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
        .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn title_case(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Natural ordering so `2-foo` sorts before `10-bar`.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ac), Some(bc)) if ac.is_ascii_digit() && bc.is_ascii_digit() => {
                let mut an = String::new();
                while let Some(c) = ai.peek().copied() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    an.push(c);
                    ai.next();
                }
                let mut bn = String::new();
                while let Some(c) = bi.peek().copied() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    bn.push(c);
                    bi.next();
                }
                let av: u128 = an.parse().unwrap_or(0);
                let bv: u128 = bn.parse().unwrap_or(0);
                match av.cmp(&bv) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            (Some(ac), Some(bc)) => match ac.to_ascii_lowercase().cmp(&bc.to_ascii_lowercase()) {
                Ordering::Equal => {
                    ai.next();
                    bi.next();
                }
                other => return other,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_is_numeric() {
        let mut names = vec!["10-b.md", "2-a.md", "1-c.md"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, vec!["1-c.md", "2-a.md", "10-b.md"]);
    }

    #[test]
    fn canonical_titles() {
        assert_eq!(display_name(Path::new("/x/in-progress")), "Doing");
        assert_eq!(display_name(Path::new("/x/scratch-notes")), "Scratch Notes");
    }

    #[test]
    fn ready_sits_between_todo_and_doing() {
        let dir = std::env::temp_dir().join(format!("roadmap-viewer-ready-{}", std::process::id()));
        for name in ["done", "doing", "ready", "todo"] {
            fs::create_dir_all(dir.join("roadmap").join(name)).unwrap();
        }
        let roadmap = build(&locate(&dir).unwrap()).unwrap();
        let names: Vec<&str> = roadmap.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Todo", "Ready", "Doing", "Done"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn entering_ready_numbers_a_ticket_and_leaving_drops_the_number() {
        let dir = std::env::temp_dir().join(format!("roadmap-viewer-prio-{}", std::process::id()));
        let (todo, ready, doing) = (dir.join("todo"), dir.join("ready"), dir.join("doing"));
        for d in [&todo, &ready, &doing] {
            fs::create_dir_all(d).unwrap();
        }
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "001-make-green.md"
        );
        fs::write(ready.join("001-a.md"), "").unwrap();
        fs::write(ready.join("007-b.md"), "").unwrap();
        fs::write(ready.join("2026-05-29-dated.md"), "").unwrap();
        // After the last number, not in the first gap: priority is kept.
        assert_eq!(
            moved_name("make-green.md", &todo, &ready),
            "008-make-green.md"
        );
        // A renumbered ticket loses its old priority, other numbers stay.
        assert_eq!(moved_name("003-x.md", &doing, &ready), "008-x.md");
        assert_eq!(moved_name("02-ai.md", &todo, &ready), "008-02-ai.md");

        assert_eq!(moved_name("008-02-ai.md", &ready, &doing), "02-ai.md");
        assert_eq!(moved_name("008-x.md", &ready, &todo), "x.md");
        assert_eq!(moved_name("unnumbered.md", &ready, &doing), "unnumbered.md");
        assert_eq!(moved_name("001-x.md", &todo, &doing), "001-x.md");
        assert_eq!(strip_priority("2026-05-29-dated.md"), "2026-05-29-dated.md");
        fs::remove_dir_all(&dir).ok();
    }

    /// Doing is ordered by last edit too, but its cards carry no date.
    #[test]
    fn doing_tickets_are_most_recent_first_without_a_date() {
        let dir = std::env::temp_dir().join(format!("roadmap-viewer-doing-{}", std::process::id()));
        let doing = dir.join("roadmap/doing");
        fs::create_dir_all(&doing).unwrap();
        for name in ["a.md", "b.md"] {
            fs::write(doing.join(name), "# T\n").unwrap();
        }
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(doing.join("a.md"))
            .unwrap()
            .set_modified(later)
            .unwrap();

        let roadmap = build(&locate(&dir).unwrap()).unwrap();
        let column = roadmap.columns.iter().find(|c| c.name == "Doing").unwrap();
        let names: Vec<String> = column.tickets.iter().map(|t| t.file_name()).collect();
        assert_eq!(names, ["a.md", "b.md"]);
        assert!(column.tickets.iter().all(|t| t.done_at.is_none()));
        fs::remove_dir_all(&dir).ok();
    }

    /// Outside git, done tickets are dated by their file times.
    #[test]
    fn done_tickets_are_dated_and_most_recent_first() {
        let dir = std::env::temp_dir().join(format!("roadmap-viewer-done-{}", std::process::id()));
        let done = dir.join("roadmap/done");
        let todo = dir.join("roadmap/todo");
        fs::create_dir_all(&done).unwrap();
        fs::create_dir_all(&todo).unwrap();
        for name in ["01-old.md", "02-mid.md", "03-new.md"] {
            fs::write(done.join(name), "# T\n").unwrap();
        }
        fs::write(todo.join("a.md"), "# A\n").unwrap();
        // Edited after the others: first despite its number.
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(86_400);
        fs::File::options()
            .write(true)
            .open(done.join("01-old.md"))
            .unwrap()
            .set_modified(later)
            .unwrap();

        let roadmap = build(&locate(&dir).unwrap()).unwrap();
        let column = |name: &str| roadmap.columns.iter().find(|c| c.name == name).unwrap();
        let names: Vec<String> = column("Done")
            .tickets
            .iter()
            .map(|t| t.file_name())
            .collect();
        assert_eq!(names, ["01-old.md", "03-new.md", "02-mid.md"]);
        assert!(column("Done").tickets.iter().all(|t| t.done_at.is_some()));
        assert!(
            column("Todo").tickets[0].done_at.is_none(),
            "only Done is dated"
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn orderable_columns_and_numbered_names() {
        assert_eq!(numbered_name(1, "hello.md"), "001-hello.md");
        assert_eq!(numbered_name(2, "002-byebye.md"), "002-byebye.md");
        assert_eq!(numbered_name(10, "x.md"), "010-x.md");
        assert!(is_orderable_column(Path::new("/x/todo")));
        assert!(is_orderable_column(Path::new("/x/ready")));
        assert!(!is_orderable_column(Path::new("/x/doing")));
        assert!(!is_orderable_column(Path::new("/x/done")));
    }
}
