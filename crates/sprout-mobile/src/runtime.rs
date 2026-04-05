// ABOUTME: Tokio runtime lifecycle management for the mobile crate.
// ABOUTME: Creates a dedicated multi-thread runtime owned by SproutClient.

use crate::error::SproutError;

/// Owns a Tokio runtime dedicated to sprout-mobile async operations.
///
/// The runtime is created when [`SproutClient::new`](crate::SproutClient::new)
/// is called and shut down when the client is dropped.
pub(crate) struct Runtime {
    #[allow(dead_code)]
    rt: tokio::runtime::Runtime,
}

impl Runtime {
    /// Create a new Tokio runtime with 2 worker threads (suitable for mobile).
    pub fn new() -> Result<Self, SproutError> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("sprout-mobile")
            .build()
            .map_err(|e| SproutError::InternalError {
                message: e.to_string(),
            })?;
        Ok(Self { rt })
    }

    /// Get a handle to the runtime for spawning tasks.
    #[allow(dead_code)]
    pub fn handle(&self) -> tokio::runtime::Handle {
        self.rt.handle().clone()
    }
}
