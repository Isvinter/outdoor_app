use core_app::AppError;
use serde::Serialize;

/// Error as the frontend sees it: a stable code for translation plus details.
#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(e: AppError) -> Self {
        Self {
            code: e.code(),
            message: e.to_string(),
        }
    }
}

impl CommandError {
    pub fn io(path: &str, e: &std::io::Error) -> Self {
        Self {
            code: "io",
            message: format!("{path}: {e}"),
        }
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
