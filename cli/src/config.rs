use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub admin_api_key: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_format")]
    pub default_format: String,
    #[serde(default = "default_limit")]
    pub default_limit: u32,
}

fn default_base_url() -> String { "http://localhost:3000".into() }
fn default_timeout()  -> u64    { 30 }
fn default_format()   -> String { "table".into() }
fn default_limit()    -> u32    { 25 }

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url:      default_base_url(),
            api_key:       String::new(),
            admin_api_key: String::new(),
            timeout_secs:  default_timeout(),
            default_format: default_format(),
            default_limit: default_limit(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("reading config at {}", path.display()))?;
        toml::from_str(&raw).context("parsing config file")
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).context("serialising config")?;
        fs::write(&path, content)
            .with_context(|| format!("writing config to {}", path.display()))
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "base_url"       => self.base_url = value.into(),
            "api_key"        => self.api_key = value.into(),
            "admin_api_key"  => self.admin_api_key = value.into(),
            "timeout_secs"   => self.timeout_secs = value.parse().context("timeout_secs must be a number")?,
            "default_format" => {
                if !["json", "csv", "table"].contains(&value) {
                    anyhow::bail!("default_format must be one of: json, csv, table");
                }
                self.default_format = value.into();
            }
            "default_limit"  => self.default_limit = value.parse().context("default_limit must be a number")?,
            other => anyhow::bail!("unknown config key '{other}'"),
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<String> {
        Ok(match key {
            "base_url"       => self.base_url.clone(),
            "api_key"        => self.api_key.clone(),
            "admin_api_key"  => self.admin_api_key.clone(),
            "timeout_secs"   => self.timeout_secs.to_string(),
            "default_format" => self.default_format.clone(),
            "default_limit"  => self.default_limit.to_string(),
            other => anyhow::bail!("unknown config key '{other}'"),
        })
    }

    pub fn path() -> Result<PathBuf> { config_path() }
}

fn config_path() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("io", "soroban-pulse", "spulse")
        .context("could not determine config directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}
