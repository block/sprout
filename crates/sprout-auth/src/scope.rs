//! API token scopes.
//!
//! Stored as `TEXT[]` in the database so new scopes don't require migrations.

use std::fmt;
use std::str::FromStr;

/// An authorization scope granted to an authenticated connection or API token.
///
/// Scopes are stored as `TEXT[]` in the database so new variants can be added
/// without schema migrations. Unknown scope strings are preserved via [`Scope::Unknown`]
/// to allow forward-compatibility with future scope additions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Read messages from channels the user is a member of.
    MessagesRead,
    /// Send messages to channels the user is a member of.
    MessagesWrite,
    /// List and read channel metadata.
    ChannelsRead,
    /// Create and update channels.
    ChannelsWrite,
    /// Administrative channel operations (e.g. delete, force-remove members).
    AdminChannels,
    /// Read user profile information.
    UsersRead,
    /// Update user profile information.
    UsersWrite,
    /// Administrative user operations (e.g. suspend, impersonate).
    AdminUsers,
    /// Read background job status.
    JobsRead,
    /// Submit and cancel background jobs.
    JobsWrite,
    /// Read subscription/plan information.
    SubscriptionsRead,
    /// Modify subscription/plan information.
    SubscriptionsWrite,
    /// Download files and attachments.
    FilesRead,
    /// Upload files and attachments.
    FilesWrite,
    /// Submit events on behalf of other pubkeys (proxy service accounts only).
    ProxySubmit,
    /// A scope string not recognised by this version of the relay.
    ///
    /// Preserved as-is to allow forward-compatibility with future scope additions.
    Unknown(String),
}

impl Scope {
    /// Return a `Vec` containing every known scope variant.
    ///
    /// Used in dev mode (`require_auth_token=false`) where `X-Pubkey` header
    /// auth grants unrestricted access — there is no token to derive scopes from.
    pub fn all_known() -> Vec<Scope> {
        vec![
            Self::MessagesRead,
            Self::MessagesWrite,
            Self::ChannelsRead,
            Self::ChannelsWrite,
            Self::AdminChannels,
            Self::UsersRead,
            Self::UsersWrite,
            Self::AdminUsers,
            Self::JobsRead,
            Self::JobsWrite,
            Self::SubscriptionsRead,
            Self::SubscriptionsWrite,
            Self::FilesRead,
            Self::FilesWrite,
        ]
    }

    /// Return a `Vec` containing every known scope variant except admin scopes.
    ///
    /// Used in dev mode (`require_auth_token=false`) where `X-Pubkey` header auth grants
    /// access without a real token. Admin operations (`AdminChannels`, `AdminUsers`) require
    /// a real token even in dev mode, so they are excluded here.
    pub fn all_non_admin() -> Vec<Scope> {
        vec![
            Self::MessagesRead,
            Self::MessagesWrite,
            Self::ChannelsRead,
            Self::ChannelsWrite,
            Self::UsersRead,
            Self::UsersWrite,
            Self::JobsRead,
            Self::JobsWrite,
            Self::SubscriptionsRead,
            Self::SubscriptionsWrite,
            Self::FilesRead,
            Self::FilesWrite,
        ]
    }

    /// Return the canonical wire-format string for this scope (e.g. `"messages:read"`).
    pub fn as_str(&self) -> &str {
        match self {
            Self::MessagesRead => "messages:read",
            Self::MessagesWrite => "messages:write",
            Self::ChannelsRead => "channels:read",
            Self::ChannelsWrite => "channels:write",
            Self::AdminChannels => "admin:channels",
            Self::UsersRead => "users:read",
            Self::UsersWrite => "users:write",
            Self::AdminUsers => "admin:users",
            Self::JobsRead => "jobs:read",
            Self::JobsWrite => "jobs:write",
            Self::SubscriptionsRead => "subscriptions:read",
            Self::SubscriptionsWrite => "subscriptions:write",
            Self::FilesRead => "files:read",
            Self::FilesWrite => "files:write",
            Self::ProxySubmit => "proxy:submit",
            Self::Unknown(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "messages:read" => Self::MessagesRead,
            "messages:write" => Self::MessagesWrite,
            "channels:read" => Self::ChannelsRead,
            "channels:write" => Self::ChannelsWrite,
            "admin:channels" => Self::AdminChannels,
            "users:read" => Self::UsersRead,
            "users:write" => Self::UsersWrite,
            "admin:users" => Self::AdminUsers,
            "jobs:read" => Self::JobsRead,
            "jobs:write" => Self::JobsWrite,
            "subscriptions:read" => Self::SubscriptionsRead,
            "subscriptions:write" => Self::SubscriptionsWrite,
            "files:read" => Self::FilesRead,
            "files:write" => Self::FilesWrite,
            "proxy:submit" => Self::ProxySubmit,
            other => Self::Unknown(other.to_string()),
        })
    }
}

/// Scopes that can be self-minted via `POST /api/tokens`.
///
/// Admin-only scopes (`AdminChannels`, `UsersWrite`, `AdminUsers`, `JobsRead`, `JobsWrite`,
/// `SubscriptionsRead`, `SubscriptionsWrite`) are intentionally excluded — they require
/// `sprout-admin mint-token`.
pub const SELF_MINTABLE_SCOPES: &[Scope] = &[
    Scope::MessagesRead,
    Scope::MessagesWrite,
    Scope::ChannelsRead,
    Scope::ChannelsWrite,
    Scope::UsersRead,
    Scope::FilesRead,
    Scope::FilesWrite,
];

/// Returns `true` if the given scope may be requested via `POST /api/tokens`.
///
/// Admin-only scopes and `Scope::Unknown` always return `false`.
/// Unknown scope strings are rejected at mint time rather than silently accepted —
/// a client sending an unrecognised scope string likely has a bug.
pub fn is_self_mintable(scope: &Scope) -> bool {
    matches!(
        scope,
        Scope::MessagesRead
            | Scope::MessagesWrite
            | Scope::ChannelsRead
            | Scope::ChannelsWrite
            | Scope::UsersRead
            | Scope::FilesRead
            | Scope::FilesWrite
    )
}

/// Parse a slice of scope strings into `Vec<Scope>`.
pub fn parse_scopes(raw: &[impl AsRef<str>]) -> Vec<Scope> {
    raw.iter()
        .map(|s| {
            s.as_ref()
                .parse::<Scope>()
                .expect("infallible: Scope::from_str cannot fail")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for scope in [Scope::MessagesRead, Scope::AdminChannels, Scope::FilesRead] {
            let parsed: Scope = scope.as_str().parse().unwrap();
            assert_eq!(parsed.as_str(), scope.as_str());
        }
    }

    #[test]
    fn unknown_scope_preserved() {
        let scope: Scope = "future:capability".parse().unwrap();
        assert_eq!(scope.as_str(), "future:capability");
        assert!(matches!(scope, Scope::Unknown(_)));
    }

    #[test]
    fn parse_scopes_slice() {
        let scopes = parse_scopes(&["messages:read", "channels:write"]);
        assert_eq!(scopes, vec![Scope::MessagesRead, Scope::ChannelsWrite]);
    }

    #[test]
    fn all_non_admin_excludes_admin_scopes() {
        let scopes = Scope::all_non_admin();
        assert_eq!(scopes.len(), 12, "expected 12 non-admin scope variants");
        // Verify no duplicates
        let unique: std::collections::HashSet<_> = scopes.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            unique.len(),
            12,
            "all_non_admin() must not contain duplicates"
        );
        // Verify no Unknown variants
        for scope in &scopes {
            assert!(
                !matches!(scope, Scope::Unknown(_)),
                "all_non_admin() must not contain Unknown variants"
            );
        }
        // Verify admin scopes are excluded
        assert!(
            !scopes.contains(&Scope::AdminChannels),
            "all_non_admin() must not contain AdminChannels"
        );
        assert!(
            !scopes.contains(&Scope::AdminUsers),
            "all_non_admin() must not contain AdminUsers"
        );
    }

    #[test]
    fn all_known_returns_all_14_variants() {
        let all = Scope::all_known();
        assert_eq!(all.len(), 14, "expected 14 known scope variants");
        // Verify no duplicates
        let unique: std::collections::HashSet<_> = all.iter().map(|s| s.as_str()).collect();
        assert_eq!(unique.len(), 14, "all_known() must not contain duplicates");
        // Verify no Unknown variants
        for scope in &all {
            assert!(
                !matches!(scope, Scope::Unknown(_)),
                "all_known() must not contain Unknown variants"
            );
        }
    }
}
