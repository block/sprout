//! # Toolset System
//!
//! Controls which MCP tools are exposed based on the `SPROUT_TOOLSETS` environment
//! variable (or `--toolsets` CLI flag).
//!
//! ## Syntax
//!
//! ```text
//! SPROUT_TOOLSETS="default,channel_admin:ro,canvas"
//! ```
//!
//! Comma-separated list of toolset names with optional `:ro` / `:rw` suffix.
//! Special keywords: `default`, `all`, `none`, `dynamic`.
//!
//! Later entries override earlier ones, so `all:ro,default:rw` gives read-only
//! access everywhere except the default toolset which gets full write access.
//!
//! ## Toolsets
//!
//! | Name            | Tools |
//! |-----------------|-------|
//! | `default`       | 25    |
//! | `channel_admin` | 6     |
//! | `dms`           | 2     |
//! | `canvas`        | 2     |
//! | `workflow_admin`| 5     |
//! | `media`         | 1     |
//! | `realtime`      | 2     |
//! | `identity`      | 1     |

use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Static data
// ---------------------------------------------------------------------------

/// `(tool_name, toolset_name, is_read)`
///
/// Single source of truth for every tool's toolset membership and read/write
/// classification. `is_read = true` means the tool is safe to include under
/// a `:ro` (read-only) mode restriction.
pub const ALL_TOOLS: &[(&str, &str, bool)] = &[
    // ── default ─────────────────────────────────────────────────────────────
    ("send_message", "default", false),
    ("send_diff_message", "default", false),
    ("edit_message", "default", false),
    ("delete_message", "default", false),
    ("get_messages", "default", true),
    ("get_thread", "default", true),
    ("search", "default", true),
    ("get_feed", "default", true),
    ("add_reaction", "default", false),
    ("remove_reaction", "default", false),
    ("get_reactions", "default", true),
    ("list_channels", "default", true),
    ("get_channel", "default", true),
    ("join_channel", "default", false),
    ("leave_channel", "default", false),
    ("update_channel", "default", false),
    ("set_channel_topic", "default", false),
    ("set_channel_purpose", "default", false),
    ("open_dm", "default", false),
    ("get_users", "default", true),
    ("set_profile", "default", false),
    ("get_presence", "default", true),
    ("set_presence", "default", false),
    ("trigger_workflow", "default", false),
    ("approve_step", "default", false),
    // ── channel_admin ────────────────────────────────────────────────────────
    ("create_channel", "channel_admin", false),
    ("archive_channel", "channel_admin", false),
    ("unarchive_channel", "channel_admin", false),
    ("add_channel_member", "channel_admin", false),
    ("remove_channel_member", "channel_admin", false),
    ("list_channel_members", "channel_admin", true),
    // ── dms ──────────────────────────────────────────────────────────────────
    ("add_dm_member", "dms", false),
    ("list_dms", "dms", true),
    // ── canvas ───────────────────────────────────────────────────────────────
    ("get_canvas", "canvas", true),
    ("set_canvas", "canvas", false),
    // ── workflow_admin ────────────────────────────────────────────────────────
    ("list_workflows", "workflow_admin", true),
    ("create_workflow", "workflow_admin", false),
    ("update_workflow", "workflow_admin", false),
    ("delete_workflow", "workflow_admin", false),
    ("get_workflow_runs", "workflow_admin", true),
    // ── media ─────────────────────────────────────────────────────────────────
    ("upload_file", "media", false),
    // ── realtime ──────────────────────────────────────────────────────────────
    ("subscribe", "realtime", false),
    ("unsubscribe", "realtime", false),
    // ── identity ──────────────────────────────────────────────────────────────
    ("set_channel_add_policy", "identity", false),
];

/// Backward-compatibility aliases: `(old_name, canonical_name)`.
///
/// Aliases are registered separately in the router — they are **not** members
/// of any toolset and are therefore not filtered by toolset logic.
pub const ALIASES: &[(&str, &str)] = &[
    ("send_reply", "send_message"),
    ("get_channel_history", "get_messages"),
    ("get_user_profile", "get_users"),
    ("get_users_batch", "get_users"),
    ("get_feed_mentions", "get_feed"),
    ("get_feed_actions", "get_feed"),
    ("approve_workflow_step", "approve_step"),
];

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Access mode for a toolset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// All tools in the toolset (read + write).
    ReadWrite,
    /// Read-only tools only.
    ReadOnly,
}

/// Metadata about a toolset (used by dynamic mode meta-tools).
#[derive(Debug, Clone)]
pub struct ToolsetDef {
    /// Toolset name, e.g. `"channel_admin"`.
    pub name: &'static str,
    /// All tools belonging to this toolset.
    pub tools: &'static [ToolDef],
}

/// Metadata about a single tool (used by dynamic mode meta-tools).
#[derive(Debug, Clone, Copy)]
pub struct ToolDef {
    /// Tool name, e.g. `"get_messages"`.
    pub name: &'static str,
    /// Whether the tool is safe under `:ro` mode.
    pub is_read: bool,
}

/// Parsed toolset configuration.
///
/// Construct via [`ToolsetConfig::parse`] or [`ToolsetConfig::from_env`].
#[derive(Debug, Clone)]
pub struct ToolsetConfig {
    /// `toolset_name → Mode`. Only explicitly enabled toolsets appear here.
    enabled: HashMap<&'static str, Mode>,
    /// Whether dynamic toolsets mode is active.
    dynamic: bool,
}

// ---------------------------------------------------------------------------
// Known toolset names (compile-time set for validation)
// ---------------------------------------------------------------------------

const KNOWN_TOOLSETS: &[&str] = &[
    "default",
    "channel_admin",
    "dms",
    "canvas",
    "workflow_admin",
    "media",
    "realtime",
    "identity",
];

// ---------------------------------------------------------------------------
// Lazy static toolset definitions (built from ALL_TOOLS)
// ---------------------------------------------------------------------------

/// Returns all toolset definitions, built lazily from [`ALL_TOOLS`].
///
/// Suitable for use by dynamic mode meta-tools that need to enumerate toolsets.
pub fn all_toolsets() -> Vec<ToolsetDef> {
    let mut map: std::collections::BTreeMap<&'static str, Vec<ToolDef>> =
        std::collections::BTreeMap::new();
    for &(tool, ts, is_read) in ALL_TOOLS {
        map.entry(ts).or_default().push(ToolDef {
            name: tool,
            is_read,
        });
    }
    map.into_iter()
        .map(|(name, tools)| ToolsetDef {
            name,
            tools: Box::leak(tools.into_boxed_slice()),
        })
        .collect()
}

/// Returns the tools belonging to `name`, or `None` if the toolset is unknown.
pub fn tools_in_toolset(name: &str) -> Option<Vec<ToolDef>> {
    let tools: Vec<ToolDef> = ALL_TOOLS
        .iter()
        .filter(|&&(_, ts, _)| ts == name)
        .map(|&(tool, _, is_read)| ToolDef {
            name: tool,
            is_read,
        })
        .collect();
    if tools.is_empty() {
        None
    } else {
        Some(tools)
    }
}

// ---------------------------------------------------------------------------
// ToolsetConfig implementation
// ---------------------------------------------------------------------------

impl ToolsetConfig {
    /// Parse a comma-separated toolset string.
    ///
    /// # Keywords
    /// - `default`  — enables the `default` toolset
    /// - `all`      — enables every toolset
    /// - `none`     — clears all enabled toolsets
    /// - `dynamic`  — enables dynamic toolsets mode (meta-tools in server.rs)
    ///
    /// # Mode suffixes
    /// - `:ro`  — read-only (only tools with `is_read = true`)
    /// - `:rw`  — read-write (default)
    ///
    /// Later entries override earlier ones.
    pub fn parse(input: &str) -> Self {
        let mut enabled: HashMap<&'static str, Mode> = HashMap::new();
        let mut dynamic = false;

        for token in input.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (name, mode) = if let Some(n) = token.strip_suffix(":ro") {
                (n, Mode::ReadOnly)
            } else if let Some(n) = token.strip_suffix(":rw") {
                (n, Mode::ReadWrite)
            } else {
                (token, Mode::ReadWrite)
            };

            match name {
                "none" => {
                    enabled.clear();
                }
                "dynamic" => {
                    dynamic = true;
                }
                "all" => {
                    for &ts in KNOWN_TOOLSETS {
                        enabled.insert(ts, mode);
                    }
                }
                "default" => {
                    enabled.insert("default", mode);
                }
                other => {
                    // Intern to &'static str if known; warn and skip if not.
                    if let Some(&known) = KNOWN_TOOLSETS.iter().find(|&&k| k == other) {
                        enabled.insert(known, mode);
                    } else {
                        eprintln!("sprout-mcp: unknown toolset {:?} — skipping", other);
                    }
                }
            }
        }

        Self { enabled, dynamic }
    }

    /// Parse from `SPROUT_TOOLSETS`, falling back to `"default"`.
    pub fn from_env() -> Self {
        let val = std::env::var("SPROUT_TOOLSETS").unwrap_or_else(|_| "default".into());
        Self::parse(&val)
    }

    /// Returns the set of tool names that should be **removed** from the router.
    ///
    /// Callers pass each name to `ToolRouter::remove_route()`. Aliases are
    /// never included — they are managed separately.
    pub fn tools_to_remove(&self) -> HashSet<&'static str> {
        ALL_TOOLS
            .iter()
            .filter(|&&(_tool, ts, is_read)| {
                match self.enabled.get(ts) {
                    None => true,                     // toolset not enabled → remove
                    Some(Mode::ReadWrite) => false,   // fully enabled → keep
                    Some(Mode::ReadOnly) => !is_read, // ro → remove write tools
                }
            })
            .map(|&(tool, _, _)| tool)
            .collect()
    }

    /// Whether dynamic toolsets mode is active.
    ///
    /// When `true`, `server.rs` should register the meta-tools
    /// (`list_toolsets`, `enable_toolset`, `disable_toolset`).
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_tools(input: &str) -> HashSet<&'static str> {
        let cfg = ToolsetConfig::parse(input);
        let remove = cfg.tools_to_remove();
        ALL_TOOLS
            .iter()
            .map(|&(t, _, _)| t)
            .filter(|t| !remove.contains(t))
            .collect()
    }

    #[test]
    fn default_includes_25_tools() {
        let tools = enabled_tools("default");
        assert_eq!(tools.len(), 25);
        assert!(tools.contains("send_message"));
        assert!(tools.contains("approve_step"));
        assert!(!tools.contains("create_channel"));
    }

    #[test]
    fn none_removes_all_tools() {
        assert!(enabled_tools("none").is_empty());
    }

    #[test]
    fn all_includes_all_44_tools() {
        assert_eq!(enabled_tools("all").len(), ALL_TOOLS.len());
    }

    #[test]
    fn ro_keeps_only_read_tools() {
        let tools = enabled_tools("default:ro");
        // Every enabled tool must be a read tool
        for t in &tools {
            let is_read = ALL_TOOLS.iter().find(|&&(n, _, _)| n == *t).unwrap().2;
            assert!(is_read, "{t} should not be present in :ro mode");
        }
        assert!(tools.contains("get_messages"));
        assert!(!tools.contains("send_message"));
    }

    #[test]
    fn later_entry_overrides_earlier() {
        // all:ro then default:rw → default tools are rw, rest are ro
        let cfg = ToolsetConfig::parse("all:ro,default:rw");
        let remove = cfg.tools_to_remove();
        // send_message is default+write → should be kept (rw)
        assert!(!remove.contains("send_message"));
        // create_channel is channel_admin+write → should be removed (ro)
        assert!(remove.contains("create_channel"));
        // list_channel_members is channel_admin+read → should be kept (ro allows reads)
        assert!(!remove.contains("list_channel_members"));
    }

    #[test]
    fn unknown_toolset_is_skipped_gracefully() {
        // Should not panic; unknown toolset is silently ignored
        let tools = enabled_tools("default,nonexistent_toolset");
        assert_eq!(tools.len(), 25); // only default
    }

    #[test]
    fn empty_input_enables_nothing() {
        assert!(enabled_tools("").is_empty());
    }

    #[test]
    fn dynamic_flag_detected() {
        assert!(ToolsetConfig::parse("default,dynamic").is_dynamic());
        assert!(!ToolsetConfig::parse("default").is_dynamic());
    }

    #[test]
    fn none_after_all_clears() {
        assert!(enabled_tools("all,none").is_empty());
    }

    #[test]
    fn rw_suffix_is_same_as_bare() {
        assert_eq!(enabled_tools("default:rw"), enabled_tools("default"));
    }

    #[test]
    fn all_tools_count_is_44() {
        assert_eq!(ALL_TOOLS.len(), 44);
    }

    #[test]
    fn aliases_count_is_7() {
        assert_eq!(ALIASES.len(), 7);
    }

    #[test]
    fn tools_in_toolset_returns_correct_tools() {
        let tools = tools_in_toolset("canvas").unwrap();
        assert_eq!(tools.len(), 2);
        let names: Vec<_> = tools.iter().map(|t| t.name).collect();
        assert!(names.contains(&"get_canvas"));
        assert!(names.contains(&"set_canvas"));
    }

    #[test]
    fn tools_in_toolset_unknown_returns_none() {
        assert!(tools_in_toolset("bogus").is_none());
    }
}
