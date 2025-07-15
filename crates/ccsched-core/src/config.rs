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
        // 1. Load .env file (lowest priority)
        if let Some(env_file) = env_file {
            dotenvy::from_filename(env_file).map_err(|e| {
                CcschedError::Config(format!("Failed to load env file: {e}"))
            })?;
        } else {
            dotenvy::dotenv().ok();
        }

        // 2. Start with defaults
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite:./db.sqlite".to_string());

        // 3. Environment variables override .env file values
        let env_host = env::var("CCSCHED_HOST").ok();
        let env_port = env::var("CCSCHED_PORT").ok();
        let env_claude_path = env::var("CLAUDE_PATH").ok();

        // 4. CLI arguments override environment variables (highest priority)
        let final_host = host
            .or(env_host)
            .unwrap_or_else(|| "127.0.0.1".to_string());

        let final_port = port
            .or_else(|| env_port.and_then(|p| p.parse().ok()))
            .unwrap_or(39512);

        let final_claude_path = claude_path
            .or(env_claude_path)
            .unwrap_or_else(|| "claude".to_string());

        let env_vars = env::vars().collect();

        Ok(Self {
            database_url,
            host: final_host,
            port: final_port,
            claude_path: final_claude_path,
            env_vars,
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}