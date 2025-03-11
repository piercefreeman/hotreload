use log::{debug, error, info, trace, warn};
use owo_colors::OwoColorize;
use serde_json::{self};
use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::process::Child;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::async_resolve::AsyncResolve;
use crate::messages::Message;
use crate::multiplex_logs::parse_multiplexed_line;

/// Result from the initial fork
#[derive(Debug, Clone)]
pub enum ForkResult {
    /// Fork completed successfully with an optional return value
    Complete(Option<String>),
    /// Fork failed with an error message
    Error(String),
}

/// Result from a forked process
#[derive(Debug, Clone)]
pub enum ProcessResult {
    /// Process completed successfully with an optional return value
    Complete(Option<String>),
    /// Process failed with an error message
    Error(String),
    // Raw log output from the process
    //Log(MultiplexedLogLine),
}

/// Runtime layer for executing Python code. This is a single "built" layer that should be immutable. Any client executed code will be in a forked process and any
pub struct Layer {
    pub child: Child,                    // The forkable process with all imports loaded
    pub stdin: std::process::ChildStdin, // The stdin of the forkable process
    pub reader: Option<std::io::Lines<BufReader<std::process::ChildStdout>>>, // The reader of the forkable process
    pub stderr_reader: Option<std::io::Lines<BufReader<std::process::ChildStderr>>>, // The stderr reader of the forkable process

    pub forked_processes: Arc<Mutex<HashMap<String, i32>>>, // Map of UUID to PID
    pub forked_names: Arc<Mutex<HashMap<String, String>>>,  // Map of UUID to name

    // These are pinged when the forked process finishes startup - either successful or failure
    pub fork_resolvers: Arc<Mutex<HashMap<String, AsyncResolve<ForkResult>>>>, // Map of UUID to fork resolver

    // These are pinged when the process completes execution
    pub completion_resolvers: Arc<Mutex<HashMap<String, AsyncResolve<ProcessResult>>>>, // Map of UUID to completion resolver

    pub stdout_thread: Option<JoinHandle<()>>, // Thread handle for stdout monitoring
    pub stderr_thread: Option<JoinHandle<()>>, // Thread handle for stderr monitoring
    pub thread_terminate_tx: Arc<Mutex<Option<Sender<()>>>>, // Channel to signal thread termination
    pub stderr_terminate_tx: Arc<Mutex<Option<Sender<()>>>>, // Channel to signal stderr thread termination
}

impl Layer {
    // New constructor for Layer with shared state
    pub fn new(
        child: Child,
        stdin: std::process::ChildStdin,
        reader: std::io::Lines<BufReader<std::process::ChildStdout>>,
        stderr_reader: std::io::Lines<BufReader<std::process::ChildStderr>>,
    ) -> Self {
        Self {
            child,
            stdin,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
            forked_processes: Arc::new(Mutex::new(HashMap::new())),
            forked_names: Arc::new(Mutex::new(HashMap::new())),
            fork_resolvers: Arc::new(Mutex::new(HashMap::new())),
            completion_resolvers: Arc::new(Mutex::new(HashMap::new())),
            stdout_thread: None,
            stderr_thread: None,
            thread_terminate_tx: Arc::new(Mutex::new(None)),
            stderr_terminate_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Start monitoring threads that concurrently read from the child process stdout and stderr
    /// to avoid blocking if one stream has no content while the other does
    pub fn start_monitor_thread(&mut self) {
        // Create channels for signaling thread termination
        let (stdout_terminate_tx, stdout_terminate_rx) = mpsc::channel();
        let (stderr_terminate_tx, stderr_terminate_rx) = mpsc::channel();

        // Store the termination channels
        {
            let mut tx_guard = self.thread_terminate_tx.lock().unwrap();
            *tx_guard = Some(stdout_terminate_tx.clone());

            let mut stderr_tx_guard = self.stderr_terminate_tx.lock().unwrap();
            *stderr_tx_guard = Some(stderr_terminate_tx.clone());
        }

        // Take ownership of the readers
        let stdout_reader = self.reader.take().expect("Reader should be available");
        let stderr_reader = self
            .stderr_reader
            .take()
            .expect("Stderr reader should be available");

        // Clone the shared resolver maps for the monitor threads
        let fork_resolvers_stdout = Arc::clone(&self.fork_resolvers);
        let completion_resolvers_stdout = Arc::clone(&self.completion_resolvers);
        let forked_processes_stdout = Arc::clone(&self.forked_processes);
        let forked_names_stdout = Arc::clone(&self.forked_names);

        let fork_resolvers_stderr = Arc::clone(&self.fork_resolvers);
        let completion_resolvers_stderr = Arc::clone(&self.completion_resolvers);
        let forked_processes_stderr = Arc::clone(&self.forked_processes);
        let forked_names_stderr = Arc::clone(&self.forked_names);

        // Start a separate thread for stderr monitoring
        let stderr_thread = thread::spawn(move || {
            Self::monitor_stream(
                stderr_reader,
                "stderr",
                stderr_terminate_rx,
                &fork_resolvers_stderr,
                &completion_resolvers_stderr,
                &forked_processes_stderr,
                &forked_names_stderr,
                None, // No need to send termination to other threads
            );
        });

        // Store the stderr thread handle
        self.stderr_thread = Some(stderr_thread);

        // Start the stdout monitor thread
        let stdout_thread = thread::spawn(move || {
            Self::monitor_stream(
                stdout_reader,
                "stdout",
                stdout_terminate_rx,
                &fork_resolvers_stdout,
                &completion_resolvers_stdout,
                &forked_processes_stdout,
                &forked_names_stdout,
                Some(stderr_terminate_tx), // Ability to terminate stderr thread
            );

            info!("Stdout monitor thread exiting");
        });

        // Store the stdout thread handle
        self.stdout_thread = Some(stdout_thread);
    }

    /// Common function to monitor a stream (stdout or stderr)
    #[allow(clippy::too_many_arguments)]
    fn monitor_stream<R: BufRead>(
        reader: std::io::Lines<R>,
        stream_name: &str,
        terminate_rx: mpsc::Receiver<()>,
        fork_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ForkResult>>>>,
        completion_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ProcessResult>>>>,
        forked_processes: &Arc<Mutex<HashMap<String, i32>>>,
        forked_names: &Arc<Mutex<HashMap<String, String>>>,
        stderr_terminate_tx: Option<mpsc::Sender<()>>,
    ) {
        info!("Monitor thread for {} started", stream_name);
        let mut reader = reader;

        loop {
            // Check if we've been asked to terminate
            if terminate_rx.try_recv().is_ok() {
                info!(
                    "{} monitor thread received terminate signal, breaking out of loop",
                    stream_name
                );
                break;
            }

            // Try to read a line from the stream
            match reader.next() {
                Some(Ok(line)) => {
                    trace!("{} monitor thread read line: {}", stream_name, line);
                    Self::process_output_line(
                        &line,
                        fork_resolvers,
                        completion_resolvers,
                        forked_processes,
                        forked_names,
                    );
                }
                Some(Err(e)) => {
                    error!("Error reading from child process {}: {}", stream_name, e);
                    // Terminate stderr thread if needed
                    if let Some(tx) = &stderr_terminate_tx {
                        let _ = tx.send(());
                    }
                    break;
                }
                None => {
                    // End of stream
                    info!(
                        "End of child process {} stream detected, exiting {} monitor thread",
                        stream_name, stream_name
                    );
                    // Terminate stderr thread if needed
                    if let Some(tx) = &stderr_terminate_tx {
                        let _ = tx.send(());
                    }
                    break;
                }
            }
        }

        info!("{} monitor thread exiting", stream_name);
    }

    /// Helper function to process an output line from either stdout or stderr
    fn process_output_line(
        line: &str,
        fork_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ForkResult>>>>,
        completion_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ProcessResult>>>>,
        forked_processes: &Arc<Mutex<HashMap<String, i32>>>,
        forked_names: &Arc<Mutex<HashMap<String, String>>>,
    ) {
        // All lines streamed from the forked process (even our own messages)
        // should be multiplexed lines
        match parse_multiplexed_line(line) {
            Ok(log_line) => {
                // Find which process this log belongs to based on PID
                let forked_definitions = forked_processes.lock().unwrap();
                let mut process_uuid = None;

                for (uuid, pid) in forked_definitions.iter() {
                    if *pid == log_line.pid as i32 {
                        process_uuid = Some(uuid.clone());
                        break;
                    }
                }

                // Just print the log, don't store it
                if let Some(uuid) = process_uuid {
                    // If we're resolved a UUID from the PID, we should also have a name
                    let forked_names_guard = forked_names.lock().unwrap();
                    let process_name = forked_names_guard.get(&uuid.clone());

                    match Self::handle_message(
                        &log_line.content,
                        Some(&uuid),
                        fork_resolvers,
                        completion_resolvers,
                        forked_processes,
                        forked_names,
                    ) {
                        Ok(_) => {
                            // Successfully handled the message, nothing more to do
                        }
                        Err(_e) => {
                            // Expected error condition in the case that we didn't receive a message
                            // but instead standard stdout
                            println!(
                                "[{}]: {}",
                                process_name
                                    .unwrap_or(&String::from("unknown"))
                                    .cyan()
                                    .bold(),
                                log_line.content
                            );
                        }
                    }
                } else {
                    // If we can't match it to a specific process, log it with PID
                    println!(
                        "Unmatched log: [{}] {}",
                        format!("{}:{}", log_line.pid, log_line.stream_name)
                            .cyan()
                            .bold(),
                        log_line.content
                    );
                }
            }
            Err(_e) => {
                // If parsing fails, treat the line as a raw message. We will log the contents
                // separately if we fail processing
                if let Err(_e) = Self::handle_message(
                    line,
                    None,
                    fork_resolvers,
                    completion_resolvers,
                    forked_processes,
                    forked_names,
                ) {
                    error!("Error handling log format: {}", line);
                }
            }
        }
    }

    /// Handle various messages from the child process
    fn handle_message(
        content: &str,
        uuid: Option<&String>,
        fork_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ForkResult>>>>,
        completion_resolvers: &Arc<Mutex<HashMap<String, AsyncResolve<ProcessResult>>>>,
        forked_processes: &Arc<Mutex<HashMap<String, i32>>>,
        forked_names: &Arc<Mutex<HashMap<String, String>>>,
    ) -> Result<(), String> {
        if let Ok(message) = serde_json::from_str::<Message>(content) {
            match message {
                Message::ForkResponse(response) => {
                    // Handle fork response and update the forked processes map
                    debug!("Monitor thread received fork response: {:?}", response);

                    // Store the PID in the forked processes map
                    let mut forked_processes_guard = forked_processes.lock().unwrap();
                    forked_processes_guard.insert(response.request_id.clone(), response.child_pid);
                    drop(forked_processes_guard);

                    // Store the process name in the forked names map
                    let mut forked_names_guard = forked_names.lock().unwrap();
                    forked_names_guard.insert(response.request_id.clone(), response.request_name);
                    drop(forked_names_guard);

                    // Resolve the fork status
                    let fork_resolvers_guard = fork_resolvers.lock().unwrap();
                    if let Some(resolver) = fork_resolvers_guard.get(&response.request_id) {
                        resolver
                            .resolve(ForkResult::Complete(Some(response.child_pid.to_string())));
                    } else {
                        error!("No resolver found for UUID: {}", response.request_id);
                    }
                    drop(fork_resolvers_guard);
                    Ok(())
                }
                Message::ChildComplete(complete) => {
                    trace!("Monitor thread received function result: {:?}", complete);

                    // We should always have a known UUID to receive this status, since it's issued
                    // from the child process
                    let uuid = uuid.expect("UUID should be known");

                    // Resolve the completion
                    let completion_resolvers_guard = completion_resolvers.lock().unwrap();
                    if let Some(resolver) = completion_resolvers_guard.get(uuid) {
                        resolver.resolve(ProcessResult::Complete(complete.result.clone()));
                    } else {
                        error!("No resolver found for UUID: {}", uuid);
                    }
                    drop(completion_resolvers_guard);
                    Ok(())
                }
                Message::ChildError(error) => {
                    trace!("Monitor thread received error result: {:?}", error);

                    // We should always have a known UUID to receive this status, since it's issued
                    // from the child process
                    let uuid = uuid.expect("UUID should be known");

                    // Resolve the completion with an error, include both error message and traceback
                    let completion_resolvers_guard = completion_resolvers.lock().unwrap();
                    if let Some(resolver) = completion_resolvers_guard.get(uuid) {
                        // Create a complete error message with both the error text and traceback if available
                        let full_error = if let Some(traceback) = &error.traceback {
                            format!("{}\n\n{}", error.error, traceback)
                        } else {
                            error.error.clone()
                        };
                        resolver.resolve(ProcessResult::Error(full_error));
                    } else {
                        error!("No resolver found for UUID: {}", uuid);
                    }
                    drop(completion_resolvers_guard);
                    Ok(())
                }
                /*Message::ForkError(error) => {
                    warn!(
                        "Monitor thread received fork error: {:?}",
                        error
                    );

                    // Resolve the fork status with an error
                    let fork_resolvers_guard = fork_resolvers.lock().unwrap();
                    if let Some(resolver) = fork_resolvers_guard.get(&error.request_id) {
                        resolver.resolve(ForkResult::Error(error.error.clone()));
                    }
                    drop(fork_resolvers_guard);
                }*/
                Message::UnknownError(error) => {
                    // For unknown errors, we don't have a UUID, so we can't resolve a specific promise
                    // Only log the error for now
                    error!("Monitor thread received unknown error: {}", error.error);
                    Ok(())
                }
                _ => {
                    // We should have a handler implemented for all messages types, capture the
                    // unknown ones
                    warn!("Monitor thread received unknown message type: {}", content);
                    Ok(())
                }
            }
        } else {
            // Not a message
            Err(format!(
                "Failed to parse message, received raw content: {}",
                content
            ))
        }
    }

    /// Stop the monitoring threads if they're running
    pub fn stop_monitor_thread(&mut self) {
        info!("Stopping monitor threads");

        // ---------- Stop stdout thread ----------
        // Send termination signal to the stdout monitor thread
        {
            let tx_guard = self.thread_terminate_tx.lock().unwrap();
            match &*tx_guard {
                Some(_) => info!("Stdout termination sender exists - will attempt to send signal"),
                None => warn!(
                    "No stdout termination sender found in the mutex - already taken or never created"
                ),
            }
        }

        if let Some(terminate_tx) = self.thread_terminate_tx.lock().unwrap().take() {
            info!("Acquired stdout termination sender, sending terminate signal");
            if let Err(e) = terminate_tx.send(()) {
                // Avoid logging warning for expected error
                // If the channel is closed, it means the thread has already exited
                if e.to_string().contains("sending on a closed channel") {
                    info!("Stdout monitor thread already exited (channel closed)");
                } else {
                    warn!(
                        "Failed to send terminate signal to stdout monitor thread: {}",
                        e
                    );
                }
            } else {
                info!("Successfully sent termination signal to stdout channel");
            }
        } else {
            warn!("No stdout termination channel found - monitor thread might not be running or already being shut down");
        }

        // Wait for stdout thread to complete
        match &self.stdout_thread {
            Some(_) => info!("Stdout thread handle exists - will attempt to join"),
            None => warn!("No stdout thread handle found - already taken or never created"),
        }

        if let Some(handle) = self.stdout_thread.take() {
            info!("Acquired stdout thread handle, waiting for thread to terminate");
            if let Err(e) = handle.join() {
                error!("Failed to join stdout thread: {:?}", e);
            } else {
                info!("Successfully joined stdout thread");
            }
        } else {
            warn!("No stdout thread handle found - already taken or never created");
        }

        // ---------- Stop stderr thread ----------
        // Send termination signal to the stderr monitor thread
        {
            let tx_guard = self.stderr_terminate_tx.lock().unwrap();
            match &*tx_guard {
                Some(_) => info!("Stderr termination sender exists - will attempt to send signal"),
                None => warn!(
                    "No stderr termination sender found in the mutex - already taken or never created"
                ),
            }
        }

        if let Some(terminate_tx) = self.stderr_terminate_tx.lock().unwrap().take() {
            info!("Acquired stderr termination sender, sending terminate signal");
            if let Err(e) = terminate_tx.send(()) {
                // Avoid logging warning for expected error
                // If the channel is closed, it means the thread has already exited
                if e.to_string().contains("sending on a closed channel") {
                    info!("Stderr monitor thread already exited (channel closed)");
                } else {
                    warn!(
                        "Failed to send terminate signal to stderr monitor thread: {}",
                        e
                    );
                }
            } else {
                info!("Successfully sent termination signal to stderr channel");
            }
        } else {
            warn!("No stderr termination channel found - monitor thread might not be running or already being shut down");
        }

        // Wait for stderr thread to complete
        match &self.stderr_thread {
            Some(_) => info!("Stderr thread handle exists - will attempt to join"),
            None => warn!("No stderr thread handle found - already taken or never created"),
        }

        if let Some(handle) = self.stderr_thread.take() {
            info!("Acquired stderr thread handle, waiting for thread to terminate");
            if let Err(e) = handle.join() {
                error!("Failed to join stderr thread: {:?}", e);
            } else {
                info!("Successfully joined stderr thread");
            }
        } else {
            warn!("No stderr thread handle found - already taken or never created");
        }

        info!("All monitor threads stopped");
    }
}

#[cfg(test)]
mod tests {
    use crate::environment::Environment;
    use tempfile::TempDir;

    #[test]
    fn test_stderr_handling() -> Result<(), String> {
        // Import gag for capturing stdout in tests
        use gag::BufferRedirect;
        use std::io::Read;

        // Create a temporary directory for our test
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().to_str().unwrap();

        // Create a Python script that writes to stderr
        let python_script = r#"
def function_with_stderr_output():
    # Write to stderr with a unique string we can look for
    import sys
    sys.stderr.write("UNIQUE_STDERR_OUTPUT_FOR_TESTING_12345\n")
    sys.stderr.flush()
    
    # Also write to stdout with a different unique string
    sys.stdout.write("UNIQUE_STDOUT_OUTPUT_FOR_TESTING_67890\n")
    sys.stdout.flush()
    
    # Return success
    return "Function executed successfully"

def main():
    return function_with_stderr_output()
        "#;

        // Prepare the script for isolation
        let (pickled_data, _python_env) =
            crate::harness::prepare_script_for_isolation(python_script, "main")?;

        // Create a buffer to redirect stdout for capturing the output
        let mut buf = BufferRedirect::stdout().unwrap();

        // Create and boot the Environment
        let mut runner = Environment::new("test_package", dir_path);
        runner.boot_main()?;

        // Execute the script in isolation
        let process_uuid = runner.exec_isolated(&pickled_data, "test_stderr_script")?;

        // Wait a moment for the process to execute and logs to be processed
        std::thread::sleep(std::time::Duration::from_millis(500));

        // Communicate with the isolated process to get the result
        let result = runner.communicate_isolated(&process_uuid)?;

        // Clean up first to ensure all output is generated
        runner.stop_isolated(&process_uuid)?;

        // Verify we got the return value from the function
        assert_eq!(
            result,
            Some("Function executed successfully".to_string()),
            "Incorrect return value from isolated process"
        );

        // Get the captured output
        let mut output = String::new();
        buf.read_to_string(&mut output).unwrap();

        // Drop the buffer to restore stdout
        drop(buf);

        // This assertion should PASS because stdout is being properly captured
        assert!(
            output.contains("UNIQUE_STDOUT_OUTPUT_FOR_TESTING_67890"),
            "Expected to find stdout message in the captured output"
        );

        // This assertion should FAIL because stderr is not being properly captured
        // When stderr capture is properly implemented, this test will pass
        assert!(
            output.contains("UNIQUE_STDERR_OUTPUT_FOR_TESTING_12345"),
            "Failed to find stderr message in the captured output - stderr is not being properly captured"
        );

        Ok(())
    }
}
