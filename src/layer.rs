use log::{debug, error, info, trace, warn};
use owo_colors::OwoColorize;
use serde_json::{self};
use std::collections::HashMap;
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

    pub forked_processes: Arc<Mutex<HashMap<String, i32>>>, // Map of UUID to PID
    pub forked_names: Arc<Mutex<HashMap<String, String>>>,  // Map of UUID to name

    // These are pinged when the forked process finishes startup - either successful or failure
    pub fork_resolvers: Arc<Mutex<HashMap<String, AsyncResolve<ForkResult>>>>, // Map of UUID to fork resolver

    // These are pinged when the process completes execution
    pub completion_resolvers: Arc<Mutex<HashMap<String, AsyncResolve<ProcessResult>>>>, // Map of UUID to completion resolver

    pub monitor_thread: Option<JoinHandle<()>>, // Thread handle for monitoring output
    pub thread_terminate_tx: Arc<Mutex<Option<Sender<()>>>>, // Channel to signal thread termination
}

impl Layer {
    // New constructor for Layer with shared state
    pub fn new(
        child: Child,
        stdin: std::process::ChildStdin,
        reader: std::io::Lines<BufReader<std::process::ChildStdout>>,
    ) -> Self {
        Self {
            child,
            stdin,
            reader: Some(reader),
            forked_processes: Arc::new(Mutex::new(HashMap::new())),
            forked_names: Arc::new(Mutex::new(HashMap::new())),
            fork_resolvers: Arc::new(Mutex::new(HashMap::new())),
            completion_resolvers: Arc::new(Mutex::new(HashMap::new())),
            monitor_thread: None,
            thread_terminate_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Start a monitoring thread that continuously reads from the child process stdout
    /// and populates the result_map with parsed output
    pub fn start_monitor_thread(&mut self) {
        // Create a channel for signaling thread termination
        let (terminate_tx, terminate_rx) = mpsc::channel();
        {
            let mut tx_guard = self.thread_terminate_tx.lock().unwrap();
            *tx_guard = Some(terminate_tx);
        }

        // Take ownership of the reader
        let reader = self.reader.take().expect("Reader should be available");

        // Clone the shared resolver maps for the monitor thread
        let fork_resolvers = Arc::clone(&self.fork_resolvers);
        let completion_resolvers = Arc::clone(&self.completion_resolvers);
        let forked_processes = Arc::clone(&self.forked_processes);
        let forked_names = Arc::clone(&self.forked_names);

        // Start the monitor thread
        let thread_handle = thread::spawn(move || {
            info!("Monitor thread started");
            let mut reader = reader;

            loop {
                trace!("Monitor thread checking for termination signal");
                // Check if we've been asked to terminate
                if terminate_rx.try_recv().is_ok() {
                    info!("Monitor thread received terminate signal, breaking out of loop");
                    break;
                }

                trace!("Monitor thread attempting to read next line");
                // Try to read a line from the child process
                match reader.next() {
                    Some(Ok(line)) => {
                        trace!("Monitor thread read line: {}", line);
                        // All lines streamed from the forked process (even our own messages)
                        // should be multiplexed lines
                        match parse_multiplexed_line(&line) {
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
                                        &fork_resolvers,
                                        &completion_resolvers,
                                        &forked_processes,
                                        &forked_names,
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
                                    &line,
                                    None,
                                    &fork_resolvers,
                                    &completion_resolvers,
                                    &forked_processes,
                                    &forked_names,
                                ) {
                                    error!("Error handling log format: {}", line);
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        error!("Error reading from child process: {}", e);
                        info!("Breaking out of monitor loop due to read error");
                        break;
                    }
                    None => {
                        // End of stream
                        info!("End of child process output stream detected (stdout was closed), breaking out of monitor loop");
                        break;
                    }
                }

                // Check again for termination after processing a line
                if terminate_rx.try_recv().is_ok() {
                    info!("Monitor thread received terminate signal after processing, breaking out of loop");
                    break;
                }
            }

            info!("Monitor thread exiting");
        });

        self.monitor_thread = Some(thread_handle);
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

    /// Stop the monitoring thread if it's running
    pub fn stop_monitor_thread(&mut self) {
        info!("Beginning monitor thread shutdown procedure");

        {
            let tx_guard = self.thread_terminate_tx.lock().unwrap();
            match &*tx_guard {
                Some(_) => info!("Termination sender exists - will attempt to send signal"),
                None => warn!(
                    "No termination sender found in the mutex - already taken or never created"
                ),
            }
        }

        if let Some(terminate_tx) = self.thread_terminate_tx.lock().unwrap().take() {
            info!("Acquired termination sender, sending terminate signal to monitor thread");
            if let Err(e) = terminate_tx.send(()) {
                warn!("Failed to send terminate signal to monitor thread: {}", e);
            } else {
                info!("Successfully sent termination signal to channel");
            }
        } else {
            warn!("No termination channel found - monitor thread might not be running or already being shut down");
        }

        match &self.monitor_thread {
            Some(_) => info!("Monitor thread handle exists - will attempt to join"),
            None => warn!("No monitor thread handle found - already taken or never created"),
        }

        if let Some(handle) = self.monitor_thread.take() {
            info!("Acquired thread handle, waiting for monitor thread to terminate");
            if let Err(e) = handle.join() {
                error!("Failed to join monitor thread: {:?}", e);
            } else {
                info!("Successfully joined monitor thread");
            }
        } else {
            warn!("No thread handle found - monitor thread might not be running or already being shut down");
        }

        info!("Monitor thread shutdown procedure completed");
    }
}
