//! Settings › General, stored in the user configuration
//! (`~/.config/iboard/iboard.toml`):
//!
//! ```toml
//! [delete]
//! confirm = true   # false: delete without asking
//! ```

use std::path::Path;

use serde::Deserialize;

/// Whether deleting an issue asks first; on unless the file says otherwise.
pub fn load_confirm_delete(file: Option<&Path>) -> bool {
    #[derive(Default, Deserialize)]
    struct Raw {
        confirm: Option<bool>,
    }
    #[derive(Default, Deserialize)]
    struct FileConfig {
        #[serde(default)]
        delete: Raw,
    }
    file.and_then(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| toml::from_str::<FileConfig>(&text).ok())
        .and_then(|config| config.delete.confirm)
        .unwrap_or(true)
}

/// Writes `[delete] confirm`, keeping the file's other sections.
pub fn save_confirm_delete(file: &Path, confirm: bool) -> Result<(), String> {
    let parent = file
        .parent()
        .ok_or_else(|| "the configuration has no folder".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|err| format!("Couldn't create {}: {err}", parent.display()))?;
    let mut document = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| toml::from_str::<toml::Value>(&text).ok())
        .and_then(|value| value.as_table().cloned())
        .unwrap_or_default();
    let mut delete = toml::map::Map::new();
    delete.insert("confirm".into(), toml::Value::Boolean(confirm));
    document.insert("delete".into(), toml::Value::Table(delete));
    let body =
        toml::to_string(&document).map_err(|err| format!("Couldn't write the config: {err}"))?;
    let temp = file.with_extension("tmp");
    std::fs::write(&temp, body)
        .map_err(|err| format!("Couldn't write {}: {err}", file.display()))?;
    std::fs::rename(&temp, file).map_err(|err| format!("Couldn't save {}: {err}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_is_on_by_default_and_can_be_turned_off() {
        assert!(load_confirm_delete(None));
        let dir = std::env::temp_dir().join(format!("iboard-general-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("iboard.toml");
        std::fs::write(&file, "[titles]\nengine = \"off\"\n").unwrap();
        assert!(load_confirm_delete(Some(&file)));
        save_confirm_delete(&file, false).unwrap();
        assert!(!load_confirm_delete(Some(&file)));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("engine = \"off\""),
            "other sections stay: {text}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
