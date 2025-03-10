use serde::{Deserialize, Serialize};

/// Represents the different types of messages that can be sent between parent and child processes
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageType {
    ForkRequest,
    ForkResponse,
    ChildComplete,
    ChildError,
    UnknownCommand,
    UnknownError,
    ImportError,
    ImportComplete,
    ExitRequest,
}

/// Base trait for all messages
pub trait MessageBase {
    fn name(&self) -> MessageType;
}

/// Request to fork a process and execute code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkRequest {
    pub request_id: String,
    pub request_name: String,

    pub code: String,
}

impl MessageBase for ForkRequest {
    fn name(&self) -> MessageType {
        MessageType::ForkRequest
    }
}

impl ForkRequest {
    pub fn new(request_id: String, code: String, request_name: String) -> Self {
        Self {
            request_id,
            code,
            request_name,
        }
    }
}

/// Request to exit the process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExitRequest {}

impl MessageBase for ExitRequest {
    fn name(&self) -> MessageType {
        MessageType::ExitRequest
    }
}

impl Default for ExitRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl ExitRequest {
    pub fn new() -> Self {
        Self {}
    }
}

/// Response to a fork request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkResponse {
    pub request_id: String,
    pub request_name: String,

    pub child_pid: i32,
}

impl MessageBase for ForkResponse {
    fn name(&self) -> MessageType {
        MessageType::ForkResponse
    }
}

impl ForkResponse {
    pub fn new(request_id: String, request_name: String, child_pid: i32) -> Self {
        Self {
            request_id,
            request_name,
            child_pid,
        }
    }
}

/// Message indicating a child process has completed successfully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildComplete {
    pub result: Option<String>,
}

impl MessageBase for ChildComplete {
    fn name(&self) -> MessageType {
        MessageType::ChildComplete
    }
}

impl ChildComplete {
    pub fn new(result: Option<String>) -> Self {
        Self { result }
    }
}

/// Message indicating a child process has encountered an error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildError {
    pub error: String,
    pub traceback: Option<String>,
}

impl MessageBase for ChildError {
    fn name(&self) -> MessageType {
        MessageType::ChildError
    }
}

impl ChildError {
    pub fn new(error: String, traceback: Option<String>) -> Self {
        Self { error, traceback }
    }
}

/// Message indicating an unknown command was received
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownCommandError {
    pub command: String,
}

impl MessageBase for UnknownCommandError {
    fn name(&self) -> MessageType {
        MessageType::UnknownCommand
    }
}

impl UnknownCommandError {
    pub fn new(command: String) -> Self {
        Self { command }
    }
}

/// Message indicating an unknown error occurred
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnknownError {
    pub error: String,
    pub traceback: Option<String>,
}

impl MessageBase for UnknownError {
    fn name(&self) -> MessageType {
        MessageType::UnknownError
    }
}

impl UnknownError {
    pub fn new(error: String, traceback: Option<String>) -> Self {
        Self { error, traceback }
    }
}

/// Message indicating an import error occurred
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportError {
    pub error: String,
    pub traceback: Option<String>,
}

impl MessageBase for ImportError {
    fn name(&self) -> MessageType {
        MessageType::ImportError
    }
}

impl ImportError {
    pub fn new(error: String, traceback: Option<String>) -> Self {
        Self { error, traceback }
    }
}

/// Message indicating an import was completed successfully
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportComplete {}

impl MessageBase for ImportComplete {
    fn name(&self) -> MessageType {
        MessageType::ImportComplete
    }
}

impl Default for ImportComplete {
    fn default() -> Self {
        Self::new()
    }
}

impl ImportComplete {
    pub fn new() -> Self {
        Self {}
    }
}

/// Enum that can hold any message type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "name")]
pub enum Message {
    #[serde(rename = "FORK_REQUEST")]
    ForkRequest(ForkRequest),
    #[serde(rename = "FORK_RESPONSE")]
    ForkResponse(ForkResponse),
    #[serde(rename = "CHILD_COMPLETE")]
    ChildComplete(ChildComplete),
    #[serde(rename = "CHILD_ERROR")]
    ChildError(ChildError),
    #[serde(rename = "UNKNOWN_COMMAND")]
    UnknownCommand(UnknownCommandError),
    #[serde(rename = "UNKNOWN_ERROR")]
    UnknownError(UnknownError),
    #[serde(rename = "IMPORT_ERROR")]
    ImportError(ImportError),
    #[serde(rename = "IMPORT_COMPLETE")]
    ImportComplete(ImportComplete),
    #[serde(rename = "EXIT_REQUEST")]
    ExitRequest(ExitRequest),
}

impl Message {
    pub fn name(&self) -> MessageType {
        match self {
            Message::ForkRequest(_) => MessageType::ForkRequest,
            Message::ForkResponse(_) => MessageType::ForkResponse,
            Message::ChildComplete(_) => MessageType::ChildComplete,
            Message::ChildError(_) => MessageType::ChildError,
            Message::UnknownCommand(_) => MessageType::UnknownCommand,
            Message::UnknownError(_) => MessageType::UnknownError,
            Message::ImportError(_) => MessageType::ImportError,
            Message::ImportComplete(_) => MessageType::ImportComplete,
            Message::ExitRequest(_) => MessageType::ExitRequest,
        }
    }
}

/// Helper functions for serialization and deserialization of messages
pub mod io {
    use super::*;
    use serde_json;
    use std::io::{Read, Write};

    /// Write a message to the given writer
    pub fn write_message<W: Write, M: Serialize>(
        writer: &mut W,
        message: &M,
    ) -> std::io::Result<()> {
        let json = serde_json::to_string(message)?;
        writeln!(writer, "{}", json)?;
        Ok(())
    }

    /// Read a message from the given reader
    pub fn read_message<R: Read>(
        reader: &mut R,
    ) -> Result<Option<Message>, Box<dyn std::error::Error>> {
        let mut buffer = String::new();
        let bytes_read = reader.read_to_string(&mut buffer)?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let message: Message = serde_json::from_str(&buffer)?;
        Ok(Some(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_import_complete() {
        // Wrap the inner struct in the Message enum so that the "name" tag is added.
        let msg = Message::ImportComplete(ImportComplete::new());
        let serialized = serde_json::to_string(&msg).unwrap();
        println!("Serialized ImportComplete: {}", serialized);
        // Expected output includes "IMPORT_COMPLETE" as the tag.
        assert!(serialized.contains("IMPORT_COMPLETE"));
    }

    #[test]
    fn test_deserialize_import_complete() {
        // This is the exact format we're seeing from Python.
        let json = r#"{"name": "IMPORT_COMPLETE"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ImportComplete: {:?}",
            parsed.err()
        );
    }

    #[test]
    fn test_deserialize_message_enum() {
        // This is the exact format we're seeing from Python.
        let json = r#"{"name": "IMPORT_COMPLETE"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse Message enum: {:?}",
            parsed.err()
        );

        if let Ok(message) = parsed {
            match message {
                Message::ImportComplete(_) => (), // Success
                _ => panic!("Parsed to wrong variant: {:?}", message),
            }
        }
    }

    #[test]
    fn test_deserialize_all_message_types() {
        // Test ImportComplete
        let json = r#"{"name": "IMPORT_COMPLETE"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ImportComplete: {:?}",
            parsed.err()
        );

        // Test ForkRequest
        let json = r#"{"name": "FORK_REQUEST", "code": "print('hello')", "request_id": "test-id", "request_name": "test-name"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ForkRequest: {:?}",
            parsed.err()
        );

        // Test ForkResponse
        let json = r#"{"name": "FORK_RESPONSE", "child_pid": 1234, "request_id": "test-id", "request_name": "test-name"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ForkResponse: {:?}",
            parsed.err()
        );

        // Test ChildComplete
        let json = r#"{"name": "CHILD_COMPLETE", "result": "success"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ChildComplete: {:?}",
            parsed.err()
        );

        // Test ChildError
        let json = r#"{"name": "CHILD_ERROR", "error": "Something went wrong", "traceback": "Traceback (most recent call last):\n  File \"<stdin>\", line 1, in <module>\nSomething went wrong"}"#;
        let parsed: Result<Message, _> = serde_json::from_str(json);
        assert!(
            parsed.is_ok(),
            "Failed to parse ChildError: {:?}",
            parsed.err()
        );
    }
}
