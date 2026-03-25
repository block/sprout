mod backend;
mod discovery;
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
pub fn default_agent_workdir() -> Option<std::path::PathBuf> {
    dirs::home_dir().filter(|p| p.is_dir())
}
