use log::{debug, trace, warn};
use std::sync::{Arc, Condvar, Mutex};

/// A generic data structure for asynchronously resolving values with blocking capability.
/// This allows a thread to wait for a value to be resolved, even if the resolution happens
/// before or after the thread starts waiting.
#[derive(Clone)]
pub struct AsyncResolve<T: Clone> {
    /// The optional resolved value
    value: Arc<Mutex<Option<T>>>,
    /// Condition variable for notification when value is resolved
    condition: Arc<(Mutex<bool>, Condvar)>,
}

impl<T: Clone> AsyncResolve<T> {
    /// Creates a new unresolved AsyncResolve instance
    pub fn new() -> Self {
        debug!("Creating new AsyncResolve instance");
        Self {
            value: Arc::new(Mutex::new(None)),
            condition: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    /// Sets the resolved value and notifies all waiters
    pub fn resolve(&self, value: T) {
        debug!("Resolving AsyncResolve value");
        // Set the value
        {
            debug!("Attempting to acquire value mutex for resolution");
            let value_lock_result = self.value.lock();

            if let Err(err) = &value_lock_result {
                warn!("Failed to acquire value mutex for resolution: {:?}", err);
                return;
            }

            let mut value_guard = value_lock_result.unwrap();
            debug!("Value mutex acquired for resolution");

            *value_guard = Some(value);
            debug!("Value has been set");
        }
        debug!("Value mutex released after resolution");

        // Notify all waiters
        let (mutex, condvar) = &*self.condition;
        debug!("Attempting to acquire completion mutex for notification");
        let completion_lock_result = mutex.lock();

        if let Err(err) = &completion_lock_result {
            warn!(
                "Failed to acquire completion mutex for notification: {:?}",
                err
            );
            return;
        }

        let mut completed = completion_lock_result.unwrap();
        debug!("Completion mutex acquired for notification");

        *completed = true;
        debug!("Notifying all waiters");
        condvar.notify_all();
        debug!("All waiters notified");
        // completed guard is dropped here, releasing the mutex
    }

    /// Blocks until the value is resolved or returns immediately if already resolved
    pub fn wait(&self) -> Result<T, String> {
        debug!("Waiting for AsyncResolve value");

        // First check if value is already resolved to avoid unnecessary locking
        {
            debug!("Attempting to acquire value mutex to check if already resolved");
            let value_lock_result = self.value.lock();

            if let Err(e) = &value_lock_result {
                let err_msg = format!("Failed to lock value mutex: {:?}", e);
                warn!("{}", err_msg);
                return Err(err_msg);
            }

            let value_guard = value_lock_result.unwrap();
            debug!("Value mutex acquired for initial check");

            if let Some(value) = &*value_guard {
                debug!("Value already resolved, returning immediately");
                return Ok(value.clone());
            }
            debug!("Value not yet resolved, proceeding to wait");
        }
        debug!("Value mutex released after initial check");

        // Otherwise, wait for resolution
        let (mutex, condvar) = &*self.condition;
        debug!("Attempting to acquire completion mutex for waiting");
        let completion_lock_result = mutex.lock();

        if let Err(e) = &completion_lock_result {
            let err_msg = format!("Failed to lock completion mutex: {:?}", e);
            warn!("{}", err_msg);
            return Err(err_msg);
        }

        let mut completed = completion_lock_result.unwrap();
        debug!("Completion mutex acquired for waiting");

        // If not completed, wait for the signal
        if !*completed {
            debug!("Value not completed, waiting on condition variable");
            let wait_result = condvar.wait(completed);

            if let Err(e) = &wait_result {
                let err_msg = format!("Failed to wait on condvar: {:?}", e);
                warn!("{}", err_msg);
                return Err(err_msg);
            }

            completed = wait_result.unwrap();
            debug!(
                "Condition variable signaled, wait completed: {:?}",
                completed
            );
        } else {
            debug!("Value already completed, skipping condvar wait");
        }
        // completed guard is dropped here, releasing the mutex
        debug!("Completion mutex released after waiting");

        // Now that we've been signaled, the value should be available
        debug!("Attempting to acquire value mutex after wait completion");
        let value_lock_result = self.value.lock();

        if let Err(e) = &value_lock_result {
            let err_msg = format!("Failed to lock value mutex after wait: {:?}", e);
            warn!("{}", err_msg);
            return Err(err_msg);
        }

        let value_guard = value_lock_result.unwrap();
        debug!("Value mutex acquired after wait completion");

        match &*value_guard {
            Some(value) => {
                debug!("Value successfully retrieved after wait");
                Ok(value.clone())
            }
            None => {
                let err_msg = "Value should be resolved but is not available".to_string();
                warn!("{}", err_msg);
                Err(err_msg)
            }
        }
    }

    /// Non-blocking check if value is resolved
    pub fn is_resolved(&self) -> bool {
        trace!("Checking if AsyncResolve is resolved");
        trace!("Attempting to acquire value mutex for is_resolved check");
        let value_lock_result = self.value.lock();

        if let Err(err) = &value_lock_result {
            warn!(
                "Failed to acquire value mutex for is_resolved check: {:?}",
                err
            );
            return false;
        }

        let value_guard = value_lock_result.unwrap();
        trace!("Value mutex acquired for is_resolved check");

        let is_some = value_guard.is_some();
        trace!("Value is_resolved: {}", is_some);
        is_some
    }

    /// Non-blocking attempt to get the resolved value
    pub fn get(&self) -> Option<T> {
        trace!("Getting AsyncResolve value without waiting");
        trace!("Attempting to acquire value mutex for get");
        let value_lock_result = self.value.lock();

        if let Err(err) = &value_lock_result {
            warn!("Failed to acquire value mutex for get: {:?}", err);
            return None;
        }

        let value_guard = value_lock_result.unwrap();
        trace!("Value mutex acquired for get");

        let result = value_guard.clone();
        trace!(
            "Value get result: {}",
            if result.is_some() { "Some" } else { "None" }
        );
        result
    }
}

impl<T: Clone> Default for AsyncResolve<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_resolve_before_wait() {
        let resolver = AsyncResolve::new();
        resolver.resolve(42);

        let result = resolver.wait().unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_resolve_after_wait() {
        let resolver = AsyncResolve::new();

        let resolver_clone = resolver.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            resolver_clone.resolve(42);
        });

        let result = resolver.wait().unwrap();
        assert_eq!(result, 42);

        handle.join().unwrap();
    }

    #[test]
    fn test_is_resolved() {
        let resolver = AsyncResolve::<i32>::new();
        assert!(!resolver.is_resolved());

        resolver.resolve(42);
        assert!(resolver.is_resolved());
    }

    #[test]
    fn test_get() {
        let resolver = AsyncResolve::<String>::new();
        assert_eq!(resolver.get(), None);

        resolver.resolve("hello".to_string());
        assert_eq!(resolver.get(), Some("hello".to_string()));
    }
}
