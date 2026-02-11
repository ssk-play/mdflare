use std::fs;
use std::path::PathBuf;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

// ============================================================================
// Storage Mode
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    Cloud,
    PrivateVault,
}

impl Default for StorageMode {
    fn default() -> Self {
        StorageMode::Cloud
    }
}

// ============================================================================
// Server Settings
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    pub api_base: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            api_base: "https://cloud.mdflare.com".to_string(),
        }
    }
}

impl ServerSettings {
    pub fn settings_path() -> PathBuf {
        let proj = ProjectDirs::from("com", "mdflare", "agent")
            .expect("Failed to get config directory");
        let dir = proj.config_dir();
        fs::create_dir_all(dir).ok();
        dir.join("server_settings.json")
    }

    pub fn load() -> Self {
        let path = Self::settings_path();
        if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::settings_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub storage_mode: StorageMode,
    pub local_path: String,

    #[serde(skip)]
    pub api_base: String,
    pub username: String,
    pub api_token: String,

    pub server_port: u16,
    pub server_token: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage_mode: StorageMode::Cloud,
            local_path: String::new(),
            api_base: String::new(),
            username: String::new(),
            api_token: String::new(),
            server_port: 7779,
            server_token: generate_token(),
        }
    }
}

pub fn generate_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{:x}{:x}", now.as_secs(), now.subsec_nanos())
}

pub fn generate_connection_token(port: u16, token: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let plain = format!("http://localhost:{}|{}", port, token);
    STANDARD.encode(plain.as_bytes())
}

pub fn generate_connection_token_with_url(url: &str, token: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let plain = format!("{}|{}", url, token);
    STANDARD.encode(plain.as_bytes())
}

impl Config {
    pub fn is_configured(&self) -> bool {
        match self.storage_mode {
            StorageMode::Cloud => {
                !self.username.is_empty() && !self.local_path.is_empty() && !self.api_token.is_empty()
            }
            StorageMode::PrivateVault => {
                !self.local_path.is_empty()
            }
        }
    }

    pub fn config_path() -> PathBuf {
        let proj = ProjectDirs::from("com", "mdflare", "agent")
            .expect("Failed to get config directory");
        let dir = proj.config_dir();
        fs::create_dir_all(dir).ok();
        dir.join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        let mut config: Self = if let Ok(data) = fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        };
        config.api_base = ServerSettings::load().api_base;
        if config.server_token.is_empty() {
            config.server_token = generate_token();
        }
        config
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(path, data).ok();
        }
    }
}
