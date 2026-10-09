//! The user configuration: `$XDG_CONFIG_HOME/iboard/iboard.toml`, or
//! `~/.config/iboard/iboard.toml`, where each Settings tab keeps a section
//! (`[ai]`, `[titles]`, `[delete]`, `[labels]`) next to the hand-written
//! `[filter]`.
//!
//! The file is read once, at startup. A file or a section that cannot be used
//! falls back to the defaults and is reported, never silently dropped. Saving
//! rewrites only the sections it is given, through `toml_edit`, so comments
//! and everything else stay as written; a file that does not parse is never
//! written over.

use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use toml_edit::{DocumentMut, Item, Table};

use crate::files;

const DIR: &str = "iboard";
const FILE: &str = "iboard.toml";

/// Where the user configuration lives, or `None` when neither
/// `XDG_CONFIG_HOME` nor `HOME` is set.
pub fn config_file() -> Option<PathBuf> {
    dir_from_env("XDG_CONFIG_HOME", ".config").map(|base| base.join(DIR).join(FILE))
}

/// iboard's cache folder: `$XDG_CACHE_HOME/iboard`, or `~/.cache/iboard`.
pub fn cache_dir() -> Option<PathBuf> {
    dir_from_env("XDG_CACHE_HOME", ".cache").map(|base| base.join(DIR))
}

/// The folder `$var` names, else `fallback` under the home folder. An empty
/// variable counts as unset.
pub fn dir_from_env(var: &str, fallback: &str) -> Option<PathBuf> {
    env_dir(var).or_else(|| home().map(|home| home.join(fallback)))
}

/// The user's home folder, from `HOME`.
pub fn home() -> Option<PathBuf> {
    env_dir("HOME")
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// The user configuration as read from disk.
#[derive(Debug, Default)]
pub struct UserConfig {
    table: toml::Table,
    /// What could not be used and fell back to the defaults, for the user to
    /// fix.
    pub problems: Vec<String>,
}

impl UserConfig {
    /// Reads `file`. No file, or a missing one, is an empty configuration.
    pub fn load(file: Option<&Path>) -> Self {
        let Some(file) = file else {
            return Self::default();
        };
        match std::fs::read_to_string(file) {
            Ok(text) => Self::parse(&text, file),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => Self::broken(format!("Couldn't read {}: {err}", file.display())),
        }
    }

    /// The configuration in `text`, as read from `file`.
    pub fn parse(text: &str, file: &Path) -> Self {
        match toml::from_str::<toml::Table>(text) {
            Ok(table) => Self {
                table,
                problems: Vec::new(),
            },
            Err(err) => Self::broken(format!(
                "{}, {}. Settings use their defaults until it is fixed.",
                file.display(),
                describe(text, err.span(), err.message())
            )),
        }
    }

    fn broken(problem: String) -> Self {
        Self {
            table: toml::Table::new(),
            problems: vec![problem],
        }
    }

    /// The `[name]` section, or its defaults when it is missing. A section
    /// that does not have the expected shape gives the defaults too, and a
    /// problem.
    pub fn section<T: DeserializeOwned + Default>(&mut self, name: &str) -> T {
        let Some(value) = self.table.get(name).cloned() else {
            return T::default();
        };
        match value.try_into() {
            Ok(section) => section,
            Err(err) => {
                self.problem(format!("[{name}]: {}", one_line(err.message())));
                T::default()
            }
        }
    }

    /// Records a value a section's reader could not use.
    pub fn problem(&mut self, problem: String) {
        self.problems.push(problem);
    }
}

/// A section to write: its new content, or nothing to remove it.
pub struct Section {
    name: &'static str,
    body: Option<Table>,
}

impl Section {
    /// `[name]` holding `value`, its keys in `value`'s own order.
    pub fn new(name: &'static str, value: &impl Serialize) -> Result<Self, String> {
        let failed = |err: &dyn std::fmt::Display| format!("Couldn't write [{name}]: {err}");
        let text = toml::to_string(value).map_err(|err| failed(&err))?;
        let document: DocumentMut = text.parse().map_err(|err| failed(&err))?;
        Ok(Self {
            name,
            body: Some(document.as_table().clone()),
        })
    }

    /// Removes `[name]` from the file.
    pub fn removed(name: &'static str) -> Self {
        Self { name, body: None }
    }
}

/// Writes `sections` into `file`, keeping everything else as written. A file
/// that does not parse is left alone, so a typo is never written over.
pub fn save(file: &Path, sections: &[Section]) -> Result<(), String> {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Couldn't read {}: {err}", file.display())),
    };
    let mut document: DocumentMut = text.parse().map_err(|err: toml_edit::TomlError| {
        format!(
            "{}, {}. Fix it or delete the file, then save again.",
            file.display(),
            describe(&text, err.span(), err.message())
        )
    })?;
    for section in sections {
        match &section.body {
            Some(body) => merge(&mut document, section.name, body),
            None => {
                document.remove(section.name);
            }
        }
    }
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    }
    files::atomic_write(file, document.to_string())
        .map_err(|err| format!("Couldn't save {}: {err}", file.display()))
}

/// Gives `[name]` exactly `body`'s keys. A key that stays keeps its place and
/// its comments.
fn merge(document: &mut DocumentMut, name: &str, body: &Table) {
    if !document.get(name).is_some_and(Item::is_table) {
        document.insert(name, Item::Table(Table::new()));
    }
    let Some(section) = document.get_mut(name).and_then(Item::as_table_mut) else {
        return;
    };
    let gone: Vec<String> = section
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| !body.contains_key(key))
        .collect();
    for key in gone {
        section.remove(&key);
    }
    for (key, item) in body.iter() {
        let mut item = item.clone();
        match section.get_mut(key) {
            Some(slot) => {
                if let (Some(new), Some(old)) = (item.as_value_mut(), slot.as_value()) {
                    *new.decor_mut() = old.decor().clone();
                }
                *slot = item;
            }
            None => {
                section.insert(key, item);
            }
        }
    }
}

/// "line 3: expected `=`" for a parse error at `span` in `text`.
fn describe(text: &str, span: Option<Range<usize>>, message: &str) -> String {
    let message = one_line(message);
    match span.and_then(|span| text.get(..span.start)) {
        Some(before) => format!("line {}: {message}", before.matches('\n').count() + 1),
        None => message,
    }
}

fn one_line(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Deserialize, Serialize)]
    struct Raw {
        tool: String,
        model: String,
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "iboard-settings-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(FILE)
    }

    fn raw(tool: &str) -> Raw {
        Raw {
            tool: tool.into(),
            model: "default".into(),
        }
    }

    #[test]
    fn a_missing_file_is_an_empty_configuration() {
        let mut config = UserConfig::load(Some(&scratch("missing").join("absent.toml")));
        assert_eq!(config.section::<Raw>("ai"), Raw::default());
        assert!(config.problems.is_empty());
        assert!(UserConfig::load(None).problems.is_empty());
    }

    #[test]
    fn a_broken_file_is_reported_with_its_line() {
        let mut config = UserConfig::parse("[ai]\ntool = \"x\"\nmodel =\n", Path::new("c.toml"));
        assert_eq!(config.section::<Raw>("ai"), Raw::default());
        assert_eq!(config.problems.len(), 1);
        assert!(
            config.problems[0].starts_with("c.toml, line 3: "),
            "{config:?}"
        );
    }

    #[test]
    fn a_misshapen_section_falls_back_alone() {
        let text = "[ai]\ntool = 5\n[titles]\nengine = \"manual\"\n";
        let mut config = UserConfig::parse(text, Path::new("c.toml"));
        assert_eq!(config.section::<Raw>("ai"), Raw::default());
        assert_eq!(config.problems.len(), 1);
        assert!(config.problems[0].starts_with("[ai]: "), "{config:?}");
        let table: toml::Table = config.section("titles");
        assert_eq!(table["engine"].as_str(), Some("manual"));
    }

    #[test]
    fn saving_keeps_comments_order_and_other_sections() {
        let file = scratch("keep");
        let before = "# my settings\n\
                      [delete]\n\
                      confirm = false # asked too often\n\
                      \n\
                      [ai]\n\
                      # the one I pay for\n\
                      tool = \"claude\" # for now\n\
                      model = \"opus\"\n\
                      extra = 1\n";
        std::fs::write(&file, before).unwrap();

        save(&file, &[Section::new("ai", &raw("opencode")).unwrap()]).unwrap();

        let after = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            after,
            "# my settings\n\
             [delete]\n\
             confirm = false # asked too often\n\
             \n\
             [ai]\n\
             # the one I pay for\n\
             tool = \"opencode\" # for now\n\
             model = \"default\"\n"
        );
    }

    #[test]
    fn saving_adds_and_removes_whole_sections() {
        let file = scratch("sections");
        std::fs::write(&file, "[labels]\nbug = \"\"\n").unwrap();
        save(
            &file,
            &[
                Section::removed("labels"),
                Section::new("ai", &raw("claude")).unwrap(),
            ],
        )
        .unwrap();
        let mut config = UserConfig::load(Some(&file));
        assert_eq!(config.section::<Raw>("ai"), raw("claude"));
        let labels: toml::Table = config.section("labels");
        assert!(labels.is_empty());
        assert!(config.problems.is_empty());
    }

    #[test]
    fn saving_never_writes_over_a_broken_file() {
        let file = scratch("broken");
        let broken = "[ai]\ntool = \"claude\"\n[delete\nconfirm = false\n";
        std::fs::write(&file, broken).unwrap();

        let err = save(&file, &[Section::new("ai", &raw("opencode")).unwrap()]).unwrap_err();

        assert!(err.contains("line 3"), "{err}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
    }

    #[test]
    fn saving_creates_the_folder_and_the_file() {
        let file = scratch("create").with_file_name("nested").join(FILE);
        save(&file, &[Section::new("ai", &raw("claude")).unwrap()]).unwrap();
        assert_eq!(
            UserConfig::load(Some(&file)).section::<Raw>("ai"),
            raw("claude")
        );
    }
}
