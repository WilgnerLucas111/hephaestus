use thiserror::Error;

/// The error type for the Hephaestus system.
#[derive(Debug, Error)]
pub enum HephaestusError {
    /// Errors related to SQLite database operations.
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Errors related to file I/O operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Errors related to Tree-sitter parsing.
    #[error("Tree-sitter error: {0}")]
    TreeSitter(#[from] tree_sitter::LanguageError),

    /// Errors related to JSON serialization/deserialization.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Errors related to sandbox execution.
    #[error("Sandbox error: {0}")]
    Sandbox(#[from] crate::sandbox::executor::SandboxError),

    /// Custom errors for specific domain logic.
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Operation not permitted: {0}")]
    NotPermitted(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Internal inconsistency: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, HephaestusError>;
