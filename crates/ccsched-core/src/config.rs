use crate::error::{CcschedError, Result};
use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub claude_path: String,
    pub env_vars: HashMap<String, String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:./db.sqlite".to_string());

        let host = env::var("CCSCHED_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        let port = env::var("CCSCHED_PORT")
            .unwrap_or_else(|_| "39512".to_string())
            .parse()
            .map_err(|e| CcschedError::Config(format!("Invalid port: {e}")))?;

        let claude_path = env::var("CLAUDE_PATH")
            .unwrap_or_else(|_| "claude".to_string());

        let env_vars = env::vars().collect();

        Ok(Self {
            database_url,
            host,
            port,
            claude_path,
            env_vars,
        })
    }

    pub fn with_overrides(
        host: Option<String>,
        port: Option<u16>,
        claude_path: Option<String>,
        env_file: Option<String>,
    ) -> Result<Self> {
        if let Some(env_file) = env_file {
            dotenvy::from_filename(env_file).map_err(|e| {
                CcschedError::Config(format!("Failed to load env file: {e}"))
            })?;
        } else {
            dotenvy::dotenv().ok();
        }

        let mut config = Self::from_env()?;

        if let Some(host) = host {
            config.host = host;
        }
        if let Some(port) = port {
            config.port = port;
        }
        if let Some(claude_path) = claude_path {
            config.claude_path = claude_path;
        }

        // Capture environment variables after loading env file
        config.env_vars = env::vars().collect();

        Ok(config)
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}