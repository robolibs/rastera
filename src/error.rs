use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum Error {
    #[error("File error in '{path}' during {operation}: {reason}")]
    File {
        path: String,
        operation: &'static str,
        reason: String,
    },

    #[error("Tag {tag} ({tag_name}) at offset 0x{offset:x}: {reason}")]
    Tag {
        tag: u16,
        tag_name: &'static str,
        offset: u64,
        reason: String,
    },

    #[error("Validation failed for {field}: expected {expected}, got {actual}")]
    Validation {
        field: &'static str,
        expected: String,
        actual: String,
    },

    #[error("Unsupported feature: {feature}")]
    Unsupported { feature: String },

    #[error("{0}")]
    Message(String),
}

pub type Result<T> = std::result::Result<T, Error>;
