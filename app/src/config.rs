use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Theme {
    #[serde(rename = "light")]
    Light,
    #[serde(rename = "dark")]
    Dark,
}
impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Settings {
    #[serde(default)]
    pub keyboard_path: String,
    #[serde(default)]
    pub cdc_path: String,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default)]
    pub debug: bool,
}

impl Settings {
    pub fn path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("org", "keyboard-bridge", "keyboard-bridge")
            .context("cannot determine XDG config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let content =
            fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        toml::from_str(&content).context("parse config.toml")
    }
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("write {}", path.display()))
    }
    #[cfg(test)]
    pub fn configured(&self) -> bool {
        !self.keyboard_path.is_empty() && !self.cdc_path.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_are_safe() {
        assert!(!Settings::default().configured());
        assert!(!Settings::default().debug);
    }
}
