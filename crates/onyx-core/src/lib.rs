pub mod models;
pub mod storage;
pub mod repository;
pub mod config;
pub mod error;
pub mod webdav;
pub mod sync;
pub mod google_tasks;

pub use models::{Task, TaskStatus, TaskList};
pub use repository::TaskRepository;
pub use config::{AppConfig, WorkspaceConfig};
pub use error::{Error, Result};

/// Sanitize a string for use as a filesystem path component.
/// Replaces filesystem-unsafe characters with underscores, trims leading/trailing
/// dots and spaces, and prefixes Windows reserved device names.
pub(crate) fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            '\0'..='\x1f' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim_matches(|c: char| c == '.' || c == ' ')
        .to_string();
    // Reject Windows reserved device names (CON, PRN, AUX, NUL, COM0-9, LPT0-9)
    let stem = sanitized.split('.').next().unwrap_or("").to_uppercase();
    let is_reserved = matches!(stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL"
        | "COM0" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
        | "LPT0" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    );
    if is_reserved {
        format!("_{}", sanitized)
    } else {
        sanitized
    }
}
