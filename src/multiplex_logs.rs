/// Represents the parsed components of a multiplexed log line
#[derive(Debug, Clone, PartialEq)]
pub struct MultiplexedLogLine {
    /// Process ID that generated the log
    pub pid: u32,
    /// Stream name (stdout or stderr)
    pub stream_name: String,
    /// The actual log content (without the prefix)
    pub content: String,
}

/// Error types that can occur during parsing of multiplexed log lines
#[derive(Debug)]
pub enum MultiplexedLogLineError {
    InvalidFormat(String),
    PidParseError(std::num::ParseIntError),
    MissingComponent(String),
}

impl std::fmt::Display for MultiplexedLogLineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat(msg) => write!(f, "Invalid log format: {}", msg),
            Self::PidParseError(err) => write!(f, "Failed to parse PID: {}", err),
            Self::MissingComponent(msg) => write!(f, "Missing component: {}", msg),
        }
    }
}

impl std::error::Error for MultiplexedLogLineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PidParseError(err) => Some(err),
            _ => None,
        }
    }
}

/// Robustly parses a line using our multiplex logging convention
/// Format: [PID:{pid}:{stream_name}] {content}
///
/// # Returns
/// - `Ok(MultiplexedLogLine)` if the line matches the expected format
/// - `Err(MultiplexedLogLineError)` if parsing fails
pub fn parse_multiplexed_line(line: &str) -> Result<MultiplexedLogLine, MultiplexedLogLineError> {
    // Check for the opening pattern
    if !line.starts_with("[PID:") {
        return Err(MultiplexedLogLineError::InvalidFormat(
            "Line does not start with [PID:".to_string(),
        ));
    }

    // Find the closing bracket that ends the prefix
    let closing_bracket_pos = match line.find(']') {
        Some(pos) => pos,
        None => {
            return Err(MultiplexedLogLineError::InvalidFormat(
                "Missing closing bracket after prefix".to_string(),
            ))
        }
    };

    // Extract the prefix content (without the brackets)
    let prefix = &line[5..closing_bracket_pos];

    // Split the prefix by colon to get pid and stream_name
    let parts: Vec<&str> = prefix.split(':').collect();

    if parts.len() != 2 {
        return Err(MultiplexedLogLineError::InvalidFormat(format!(
            "Expected format [PID:pid:stream_name], got [PID:{}]",
            prefix
        )));
    }

    // Parse the PID
    let pid = parts[0]
        .parse::<u32>()
        .map_err(MultiplexedLogLineError::PidParseError)?;

    // Get the stream name
    let stream_name = parts[1].to_string();

    if stream_name.is_empty() {
        return Err(MultiplexedLogLineError::MissingComponent(
            "Stream name is empty".to_string(),
        ));
    }

    // Extract the content (everything after the closing bracket)
    let content = if closing_bracket_pos + 1 < line.len() {
        line[closing_bracket_pos + 1..].to_string()
    } else {
        // If there's nothing after the closing bracket, return empty content
        String::new()
    };

    Ok(MultiplexedLogLine {
        pid,
        stream_name,
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_line_format() {
        let test_line = "[PID:12345:stdout]Hello, world!";
        let result = parse_multiplexed_line(test_line).unwrap();
        assert_eq!(result.pid, 12345);
        assert_eq!(result.stream_name, "stdout");
        assert_eq!(result.content, "Hello, world!");
    }

    #[test]
    fn test_valid_line_with_stderr() {
        let test_line = "[PID:9876:stderr]Error message";
        let result = parse_multiplexed_line(test_line).unwrap();
        assert_eq!(result.pid, 9876);
        assert_eq!(result.stream_name, "stderr");
        assert_eq!(result.content, "Error message");
    }

    #[test]
    fn test_empty_content() {
        let test_line = "[PID:12345:stdout]";
        let result = parse_multiplexed_line(test_line).unwrap();
        assert_eq!(result.content, "");
    }

    #[test]
    fn test_missing_prefix() {
        let test_line = "Hello, world!";
        let result = parse_multiplexed_line(test_line);
        assert!(result.is_err());
        match result {
            Err(MultiplexedLogLineError::InvalidFormat(msg)) => {
                assert!(msg.contains("does not start with [PID:"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_invalid_pid_format() {
        let test_line = "[PID:abc:stdout]Hello, world!";
        let result = parse_multiplexed_line(test_line);
        assert!(result.is_err());
        match result {
            Err(MultiplexedLogLineError::PidParseError(_)) => {
                // This is expected
            }
            _ => panic!("Expected PidParseError"),
        }
    }

    #[test]
    fn test_missing_closing_bracket() {
        let test_line = "[PID:12345:stdout Hello, world!";
        let result = parse_multiplexed_line(test_line);
        assert!(result.is_err());
        match result {
            Err(MultiplexedLogLineError::InvalidFormat(msg)) => {
                assert!(msg.contains("Missing closing bracket"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }

    #[test]
    fn test_missing_stream_name() {
        let test_line = "[PID:12345:]Hello, world!";
        let result = parse_multiplexed_line(test_line);
        assert!(result.is_err());
        match result {
            Err(MultiplexedLogLineError::MissingComponent(msg)) => {
                assert!(msg.contains("Stream name is empty"));
            }
            _ => panic!("Expected MissingComponent error"),
        }
    }

    #[test]
    fn test_malformed_prefix() {
        let test_line = "[PID:12345]Hello, world!";
        let result = parse_multiplexed_line(test_line);
        assert!(result.is_err());
        match result {
            Err(MultiplexedLogLineError::InvalidFormat(msg)) => {
                assert!(msg.contains("Expected format"));
            }
            _ => panic!("Expected InvalidFormat error"),
        }
    }
}
