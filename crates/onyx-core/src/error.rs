use std::io;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Serialization(String),
    NotFound(String),
    InvalidData(String),
    WorkspaceNotFound(String),
    ListNotFound(String),
    TaskNotFound(String),
    WebDav(String),
    Sync(String),
    Credential(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "IO error: {}", e),
            Error::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            Error::NotFound(msg) => write!(f, "Not found: {}", msg),
            Error::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            Error::WorkspaceNotFound(name) => write!(f, "Workspace not found: {}", name),
            Error::ListNotFound(id) => write!(f, "List not found: {}", id),
            Error::TaskNotFound(id) => write!(f, "Task not found: {}", id),
            Error::WebDav(msg) => write!(f, "WebDAV error: {}", msg),
            Error::Sync(msg) => write!(f, "Sync error: {}", msg),
            Error::Credential(msg) => write!(f, "Credential error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::WebDav(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
        let err = Error::Io(io_err);
        assert_eq!(err.to_string(), "IO error: file missing");
    }

    #[test]
    fn test_display_serialization() {
        let err = Error::Serialization("bad json".to_string());
        assert_eq!(err.to_string(), "Serialization error: bad json");
    }

    #[test]
    fn test_display_not_found() {
        let err = Error::NotFound("item".to_string());
        assert_eq!(err.to_string(), "Not found: item");
    }

    #[test]
    fn test_display_invalid_data() {
        let err = Error::InvalidData("corrupt".to_string());
        assert_eq!(err.to_string(), "Invalid data: corrupt");
    }

    #[test]
    fn test_display_workspace_not_found() {
        let err = Error::WorkspaceNotFound("myws".to_string());
        assert_eq!(err.to_string(), "Workspace not found: myws");
    }

    #[test]
    fn test_display_list_not_found() {
        let err = Error::ListNotFound("abc-123".to_string());
        assert_eq!(err.to_string(), "List not found: abc-123");
    }

    #[test]
    fn test_display_task_not_found() {
        let err = Error::TaskNotFound("task-456".to_string());
        assert_eq!(err.to_string(), "Task not found: task-456");
    }

    #[test]
    fn test_display_webdav() {
        let err = Error::WebDav("connection refused".to_string());
        assert_eq!(err.to_string(), "WebDAV error: connection refused");
    }

    #[test]
    fn test_display_sync() {
        let err = Error::Sync("conflict".to_string());
        assert_eq!(err.to_string(), "Sync error: conflict");
    }

    #[test]
    fn test_display_credential() {
        let err = Error::Credential("keychain locked".to_string());
        assert_eq!(err.to_string(), "Credential error: keychain locked");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{{bad").unwrap_err();
        let err: Error = json_err.into();
        assert!(matches!(err, Error::Serialization(_)));
    }

    #[test]
    fn test_from_serde_yaml_error() {
        let yaml_err = serde_yaml::from_str::<serde_yaml::Value>(":\n  :\n    ::: bad").unwrap_err();
        let err: Error = yaml_err.into();
        assert!(matches!(err, Error::Serialization(_)));
    }

    #[test]
    fn test_error_is_std_error() {
        let err = Error::NotFound("x".to_string());
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_error_debug() {
        let err = Error::NotFound("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("NotFound"));
        assert!(debug.contains("test"));
    }
}
