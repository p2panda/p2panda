use validator::ValidationErrors;

/// A specialized `Result` type for p2panda-rs.
pub type Result<T> = anyhow::Result<T>;

/// A specialized `Result` type for validation errors.
pub type ValidationResult = anyhow::Result<(), ValidationErrors>;
