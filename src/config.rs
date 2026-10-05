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
use std::path::{Path, PathBuf};

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

pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
}

/// Look for a config file in `dir`, returning the first one that parses.
pub fn load(dir: &Path) -> Option<LoadedConfig> {
    for name in ["iboard.toml"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("warning: cannot read {}: {err}", path.display());
                return None;
            }
        };
        return match toml::from_str::<Config>(&text) {
            Ok(config) => Some(LoadedConfig { config, path }),
            Err(err) => {
                eprintln!("warning: invalid {}: {err}", path.display());
                None
            }
        };
    }
    None
}
