use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub username: String,
    pub password: String,
    #[serde(default = "default_adapter_name")]
    pub adapter_name: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}

fn default_adapter_name() -> String {
    "以太网".to_owned()
}

fn default_timeout() -> u64 {
    15
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("无法读取配置 {}: {e}", path.display()))?;
        let config: Self =
            toml::from_str(&text).map_err(|e| format!("配置格式错误 {}: {e}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.username.trim().is_empty() {
            return Err("username 不能为空".into());
        }
        if self.password.is_empty() {
            return Err("password 不能为空".into());
        }
        if self.adapter_name.trim().is_empty() {
            return Err("adapter_name 不能为空".into());
        }
        if self.timeout_seconds == 0 || self.timeout_seconds > 120 {
            return Err("timeout_seconds 必须在 1..=120 之间".into());
        }
        Ok(())
    }
}
