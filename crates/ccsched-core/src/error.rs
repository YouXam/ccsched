use thiserror::Error;

#[derive(Error, Debug)]
pub enum CcschedError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Task not found: {0}")]
    TaskNotFound(i64),
    
    #[error("Invalid task status transition from {from} to {to}")]
    InvalidStatusTransition { from: String, to: String },
    
    #[error("Circular dependency detected in task graph")]
    CircularDependency,
    
    #[error("Claude execution error: {0}")]
    ClaudeExecution(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, CcschedError>;