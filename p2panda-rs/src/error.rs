use validator::ValidationErrors;

// @TODO: Replace this with own `Validation` trait
pub type ValidationResult = anyhow::Result<(), ValidationErrors>;

/// A specialized `Result` type for p2panda-rs.
pub type Result<T> = anyhow::Result<T>;
