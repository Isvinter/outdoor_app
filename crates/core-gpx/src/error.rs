use thiserror::Error;

/// Errors of GPX import and export. `line` is the 1-based line in the input.
#[derive(Debug, Error)]
pub enum GpxError {
    #[error("not a GPX document")]
    NotGpx,
    #[error("malformed XML in line {line}: {message}")]
    Xml { line: usize, message: String },
    #[error("track point in line {line} has no `{attribute}` attribute")]
    MissingAttribute {
        attribute: &'static str,
        line: usize,
    },
    #[error("invalid number `{value}` in line {line}")]
    InvalidNumber { value: String, line: usize },
    #[error("invalid time `{value}` in line {line}")]
    InvalidTime { value: String, line: usize },
    #[error("track point has a non-finite coordinate")]
    InvalidCoordinate,
    #[error("track point time is outside the representable range")]
    TimeOutOfRange,
}
