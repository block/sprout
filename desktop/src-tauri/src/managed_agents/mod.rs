mod backend;
mod discovery;
mod persona_avatars;
mod persona_card;
mod personas;
mod runtime;
mod storage;
mod teams;
mod types;

pub use backend::*;
pub use discovery::*;
pub use persona_card::*;
pub use personas::*;
pub use runtime::*;
pub use storage::*;
pub use teams::*;
pub use types::*;

/// Returns the user's home directory if it can be resolved and exists.
/// Used as the default working directory for spawned agent processes.
///
/// Cached for the process lifetime — home directory doesn't change at runtime.
/// Returns `None` in sandboxed/containerized environments where `$HOME` is
/// unset or points to a non-existent path; callers fall back to inheriting
/// the parent's CWD.
pub fn default_agent_workdir() -> Option<std::path::PathBuf> {
    use std::sync::OnceLock;
    static HOME: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    HOME.get_or_init(|| dirs::home_dir().filter(|p| p.is_dir()))
        .clone()
}
