//! Optional per-project configuration.
//!
//! The viewer works with zero configuration by auto-detecting the `issues/`
//! directory and its columns. When the layout is unusual, an `iboard.toml`
//! placed at the project root can override it:
//!
//! ```toml
//! root = "issues"                       # defaults to "issues"
//! columns = ["backlog", "doing", "done"] # explicit order; omit to auto-detect
//! root_column_name = "Backlog"           # name of the root-level files column
//! title = "Moon Cull"                    # window title override
//!
//! [reader]
//! sections = ["Context", "Acceptance criteria"] # asked for by the reader; [] turns it off
//! ```

use serde::Deserialize;
use std::path::Path;

use crate::platform::files;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directory holding the issues, relative to the config file.
    pub root: Option<String>,
    /// Explicit, ordered list of column directories inside the issues root.
    /// When omitted, columns are auto-detected from the sub-directories.
    pub columns: Option<Vec<String>>,
    /// Display name for the column made of files sitting directly in the
    /// issues root. Defaults to `Todo`, or `Backlog` when a `todo/` dir exists.
    pub root_column_name: Option<String>,
    /// Window title override.
    pub title: Option<String>,
    pub reader: Option<ReaderConfig>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReaderConfig {
    /// The `##` sections the reader offers to add when an issue lacks them.
    /// Defaults to Context, Acceptance criteria, Out of scope, Open questions.
    pub sections: Option<Vec<String>>,
}

/// Writes `root` to the project's `iboard.toml`, keeping every other key and
/// creating the file when there is none.
pub fn write_root(project_dir: &Path, name: &str) -> Result<(), String> {
    let file = project_dir.join("iboard.toml");
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Couldn't read {}: {err}", file.display())),
    };
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|err| format!("Couldn't edit {}: {err}", file.display()))?;
    document["root"] = toml_edit::value(name);
    files::write_creating(&file, document.to_string()).map_err(|err| match err {
        files::WriteError::Folder(dir, err) => format!("Couldn't create {}: {err}", dir.display()),
        files::WriteError::File(err) => format!("Couldn't write {}: {err}", file.display()),
    })
}

/// The `iboard.toml` in `dir`, when there is one and it parses.
pub fn load(dir: &Path) -> Option<Config> {
    let path = dir.join("iboard.toml");
    if !path.is_file() {
        return None;
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            eprintln!("warning: cannot read {}: {err}", path.display());
            return None;
        }
    };
    match toml::from_str::<Config>(&text) {
        Ok(config) => Some(config),
        Err(err) => {
            eprintln!("warning: invalid {}: {err}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_is_read_and_one_with_an_unknown_key_is_not() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        assert!(load(dir).is_none(), "no file, no config");

        let text = "title = \"Moon\"\ncolumns = [\"todo\"]\n[reader]\nsections = []\n";
        std::fs::write(dir.join("iboard.toml"), text).unwrap();
        let config = load(dir).unwrap();
        assert_eq!(config.title.as_deref(), Some("Moon"));
        assert_eq!(config.columns, Some(vec!["todo".to_string()]));
        assert_eq!(config.reader.unwrap().sections, Some(Vec::new()));

        std::fs::write(dir.join("iboard.toml"), "titel = \"a typo\"\n").unwrap();
        assert!(load(dir).is_none());
    }

    #[test]
    fn writing_the_root_keeps_every_other_key() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path();
        std::fs::write(
            dir.join("iboard.toml"),
            "# mine\nroot = \"issues\"\ntitle = \"Moon\"\n\n[reader]\nsections = []\n",
        )
        .unwrap();
        write_root(dir, "tickets").unwrap();
        let text = std::fs::read_to_string(dir.join("iboard.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "{text}");
        assert!(text.contains("title = \"Moon\""), "{text}");
        assert!(text.contains("[reader]\nsections = []"), "{text}");
        assert_eq!(load(dir).unwrap().root.as_deref(), Some("tickets"));

        std::fs::remove_file(dir.join("iboard.toml")).unwrap();
        write_root(dir, "notes").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("iboard.toml")).unwrap(),
            "root = \"notes\"\n"
        );
    }
}
