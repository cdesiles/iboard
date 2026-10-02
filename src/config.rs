//! Optional per-project configuration.
//!
//! The viewer works with zero configuration by auto-detecting the `roadmap/`
//! directory and its columns. When the layout is unusual, a `roadmap-viewer.toml`
//! (or `roadmap.toml`) placed at the project root can override it:
//!
//! ```toml
//! root = "roadmap"                       # defaults to "roadmap"
//! columns = ["backlog", "doing", "done"] # explicit order; omit to auto-detect
//! root_column_name = "Backlog"           # name of the root-level files column
//! title = "Moon Cull"                    # window title override
//! ```

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Directory holding the roadmap, relative to the config file.
    pub root: Option<String>,
    /// Explicit, ordered list of column directories inside the roadmap root.
    /// When omitted, columns are auto-detected from the sub-directories.
    pub columns: Option<Vec<String>>,
    /// Display name for the column made of files sitting directly in the
    /// roadmap root. Defaults to `Todo`, or `Backlog` when a `todo/` dir exists.
    pub root_column_name: Option<String>,
    /// Window title override.
    pub title: Option<String>,
}

pub struct LoadedConfig {
    pub config: Config,
    pub path: PathBuf,
}

/// Look for a config file in `dir`, returning the first one that parses.
pub fn load(dir: &Path) -> Option<LoadedConfig> {
    for name in ["roadmap-viewer.toml", "roadmap.toml"] {
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
