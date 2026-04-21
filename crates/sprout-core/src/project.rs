//! Project types shared across crates.
//!
//! These live in `sprout-core` (zero I/O deps) so both the SDK (client-side)
//! and the DB layer (server-side) can use the same types without pulling in
//! sqlx/tokio.

use std::fmt;
use std::str::FromStr;

/// Environment where project agents execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectEnvironment {
    /// Local machine (dev laptop).
    Local,
    /// Remote Blox compute instance.
    Blox,
}

impl ProjectEnvironment {
    /// Canonical string representation (matches DB column and Nostr tags).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Blox => "blox",
        }
    }
}

impl fmt::Display for ProjectEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProjectEnvironment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local" => Ok(Self::Local),
            "blox" => Ok(Self::Blox),
            other => Err(format!("unknown environment: {other:?}")),
        }
    }
}
