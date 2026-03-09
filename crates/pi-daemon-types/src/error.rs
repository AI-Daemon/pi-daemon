use thiserror::Error;

/// Top-level error type for pi-daemon.
#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Memory error: {0}")]
    Memory(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

pub type DaemonResult<T> = Result<T, DaemonError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_daemon_error_display() {
        let error = DaemonError::Agent("test error".to_string());
        assert_eq!(error.to_string(), "Agent error: test error");

        let error = DaemonError::Config("invalid config".to_string());
        assert_eq!(error.to_string(), "Config error: invalid config");

        let error = DaemonError::Api("bad request".to_string());
        assert_eq!(error.to_string(), "API error: bad request");

        let error = DaemonError::Memory("out of memory".to_string());
        assert_eq!(error.to_string(), "Memory error: out of memory");

        let error = DaemonError::AgentNotFound("agent-123".to_string());
        assert_eq!(error.to_string(), "Agent not found: agent-123");

        let error = DaemonError::SessionNotFound("session-456".to_string());
        assert_eq!(error.to_string(), "Session not found: session-456");
    }

    #[test]
    fn test_daemon_error_from_io_error() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let daemon_error = DaemonError::from(io_error);

        assert!(matches!(daemon_error, DaemonError::Io(_)));
        assert!(daemon_error.to_string().contains("file not found"));
    }

    #[test]
    fn test_daemon_error_from_serde_error() {
        let invalid_json = "{ invalid json }";
        let serde_error = serde_json::from_str::<serde_json::Value>(invalid_json).unwrap_err();
        let daemon_error = DaemonError::from(serde_error);

        assert!(matches!(daemon_error, DaemonError::Serde(_)));
        assert!(daemon_error.to_string().contains("Serialization error"));
    }

    #[test]
    fn test_daemon_result_alias() {
        fn test_function() -> DaemonResult<String> {
            Ok("success".to_string())
        }

        let result = test_function();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[test]
    fn test_daemon_result_error() {
        fn test_function() -> DaemonResult<String> {
            Err(DaemonError::Agent("test failure".to_string()))
        }

        let result = test_function();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "Agent error: test failure");
    }
}
