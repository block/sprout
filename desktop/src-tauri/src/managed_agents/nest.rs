//! Sprout Nest — persistent agent workspace at `~/.sprout`.
//!
//! Creates a shared knowledge directory on first launch so every
//! Sprout-spawned agent starts with orientation (AGENTS.md) and a
//! place to accumulate research, plans, and logs across sessions.
//!
//! Idempotent: existing files and directories are never overwritten.

use super::{load_managed_agents, load_personas, ManagedAgentRecord, PersonaRecord};
#[cfg(test)]
use super::{BackendKind, RespondTo};
use crate::app_state::AppState;
use crate::relay::relay_ws_url_with_override;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Subdirectories created inside the nest.
const NEST_DIRS: &[&str] = &[
    "GUIDES",
    "RESEARCH",
    "PLANS",
    "WORK_LOGS",
    "REPOS",
    "OUTBOX",
    ".scratch",
];

/// Default AGENTS.md content written on first init.
/// Fully static — no runtime interpolation, no secrets, no user paths.
const AGENTS_MD: &str = include_str!("nest_agents.md");

/// Returns the nest root path (`~/.sprout`), or `None` if the home
/// directory cannot be resolved.
pub fn nest_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".sprout"))
}

/// Creates the Sprout nest at `~/.sprout` if it doesn't already exist.
///
/// Delegates to [`ensure_nest_at`] with the resolved nest directory.
/// Returns an error string if the home directory cannot be resolved.
pub fn ensure_nest() -> Result<(), String> {
    let root = nest_dir().ok_or("cannot resolve home directory for nest")?;
    ensure_nest_at(&root)
}

/// Creates a Sprout nest at the given `root` path.
///
/// - Creates the root directory and all subdirectories.
/// - Writes `AGENTS.md` only if it doesn't already exist.
/// - Sets 700 permissions on the root and all subdirectories (Unix).
///
/// Idempotent: safe to call on every launch. Existing files are never
/// overwritten — users can freely edit AGENTS.md and it will persist.
///
/// Rejects symlinks at the root path to prevent redirect attacks.
///
/// Errors are returned as strings for Tauri compatibility; callers
/// should log and continue rather than aborting app startup.
pub fn ensure_nest_at(root: &Path) -> Result<(), String> {
    // Reject symlinks — we want a real directory, not a redirect.
    // Platform-independent: symlink_metadata works on all OS.
    if root
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(format!(
            "{} is a symlink; refusing to use as nest root",
            root.display()
        ));
    }

    // Create root and all subdirectories. create_dir_all is idempotent —
    // it succeeds silently if the directory already exists.
    fs::create_dir_all(root).map_err(|e| format!("create {}: {e}", root.display()))?;

    for dir in NEST_DIRS {
        let path = root.join(dir);
        fs::create_dir_all(&path).map_err(|e| format!("create {}: {e}", path.display()))?;
    }

    // Write AGENTS.md only if it doesn't already exist.
    // Uses create_new (O_CREAT|O_EXCL) to atomically check-and-create,
    // closing the TOCTOU gap that exists() + write() would leave open.
    // Also guarantees we never clobber a user-edited file.
    let agents_md = root.join("AGENTS.md");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&agents_md)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(AGENTS_MD.as_bytes())
                .map_err(|e| format!("write {}: {e}", agents_md.display()))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // File already exists — leave it alone (idempotent).
        }
        Err(e) => {
            return Err(format!("create {}: {e}", agents_md.display()));
        }
    }

    // Set owner-only permissions on root and all subdirectories.
    // Skip any path that is a symlink — chmod would affect the target.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(root, perms.clone())
            .map_err(|e| format!("set permissions on {}: {e}", root.display()))?;
        for dir in NEST_DIRS {
            let path = root.join(dir);
            let is_symlink = path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);
            if !is_symlink {
                fs::set_permissions(&path, perms.clone())
                    .map_err(|e| format!("set permissions on {}: {e}", path.display()))?;
            }
        }
    }

    Ok(())
}

const CLI_QUICK_REFERENCE: &str = "\
## CLI Quick Reference
`sprout messages send --channel <id> --content <text>` — send a message
`sprout messages get --channel <id>` — read recent messages
`sprout channels list` — list available channels
`sprout workflows trigger --workflow <id>` — trigger a workflow
Run `sprout --help` for the full command reference.";

pub fn render_dynamic_section(
    personas: &[PersonaRecord],
    agents: &[ManagedAgentRecord],
    relay_url: &str,
) -> String {
    let active_agents = if agents.is_empty() {
        "## Active Agents\n\n*(No agents deployed yet. Add agents in the Sprout desktop app.)*"
            .to_string()
    } else {
        let mut table =
            "## Active Agents\n\n| Name | Role | How to address |\n|------|------|----------------|"
                .to_string();
        for agent in agents {
            let role = agent
                .persona_id
                .as_deref()
                .and_then(|pid| personas.iter().find(|p| p.id == pid))
                .map(|p| p.display_name.as_str())
                .unwrap_or("—");
            table.push_str(&format!(
                "\n| {} | {} | @{} |",
                agent.name, role, agent.name
            ));
        }
        table
    };

    format!("{active_agents}\n\n## Workspace\n- Relay: {relay_url}\n\n{CLI_QUICK_REFERENCE}")
}

pub fn upsert_managed_section(file_path: &Path, new_section_content: &str) -> io::Result<()> {
    let current = fs::read_to_string(file_path)?;

    const BEGIN: &str = "<!-- BEGIN SPROUT MANAGED";
    const END: &str = "<!-- END SPROUT MANAGED -->";

    let replacement = format!(
        "<!-- BEGIN SPROUT MANAGED — regenerated automatically, do not edit below -->\n{new_section_content}\n<!-- END SPROUT MANAGED -->\n"
    );

    let new_content =
        if let (Some(begin_pos), Some(end_pos)) = (current.find(BEGIN), current.find(END)) {
            // Find the start of the BEGIN marker's line.
            let line_start = current[..begin_pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
            // END marker spans to the end of its content + the newline after it.
            let end_of_end = end_pos + END.len();
            let after_end = if current[end_of_end..].starts_with('\n') {
                end_of_end + 1
            } else {
                end_of_end
            };
            format!(
                "{}{}{}",
                &current[..line_start],
                replacement,
                &current[after_end..]
            )
        } else {
            format!("{}\n\n{}", current.trim_end_matches('\n'), replacement)
        };

    let tmp_path = file_path.with_extension(
        file_path
            .extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );
    fs::write(&tmp_path, new_content)?;
    fs::rename(&tmp_path, file_path)?;

    Ok(())
}

pub fn regenerate_nest_context(app: &AppHandle) -> Result<(), String> {
    let nest = nest_dir().ok_or("cannot resolve home directory for nest")?;
    let agents_md = nest.join("AGENTS.md");

    if !agents_md.exists() {
        return Ok(());
    }

    let personas = load_personas(app)?;
    let agents = load_managed_agents(app)?;
    let state = app.state::<AppState>();
    let relay_url = relay_ws_url_with_override(&state);
    let content = render_dynamic_section(&personas, &agents, &relay_url);
    upsert_managed_section(&agents_md, &content)
        .map_err(|e| format!("regenerate nest context: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nest_dir_is_under_home() {
        if let Some(dir) = nest_dir() {
            assert!(dir.ends_with(".sprout"));
        }
    }

    #[test]
    fn ensure_nest_creates_all_dirs_and_agents_md() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".sprout");

        ensure_nest_at(&root).unwrap();

        // All subdirectories exist.
        for dir in NEST_DIRS {
            assert!(root.join(dir).is_dir(), "{dir}/ should exist");
        }

        // AGENTS.md was written with default content.
        let content = fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert_eq!(content, AGENTS_MD);

        // Permissions are 700 on Unix for root and all subdirs.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&root).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700, "root should be 700");
            for dir in NEST_DIRS {
                let mode = fs::metadata(root.join(dir)).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o700, "{dir}/ should be 700");
            }
        }
    }

    #[test]
    fn ensure_nest_is_idempotent_and_preserves_custom_content() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".sprout");

        // First call creates everything.
        ensure_nest_at(&root).unwrap();

        // User customizes AGENTS.md.
        let agents = root.join("AGENTS.md");
        fs::write(&agents, "my custom instructions").unwrap();

        // Second call succeeds and does not overwrite.
        ensure_nest_at(&root).unwrap();

        assert_eq!(
            fs::read_to_string(&agents).unwrap(),
            "my custom instructions"
        );

        // All dirs still exist.
        for dir in NEST_DIRS {
            assert!(root.join(dir).is_dir(), "{dir}/ should still exist");
        }
    }

    #[cfg(unix)]
    #[test]
    fn ensure_nest_rejects_symlink_root() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real_dir");
        fs::create_dir(&target).unwrap();
        let link = tmp.path().join(".sprout");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let result = ensure_nest_at(&link);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn ensure_nest_skips_permissions_on_symlinked_child() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".sprout");

        // First call creates the real nest.
        ensure_nest_at(&root).unwrap();

        // Replace REPOS/ with a symlink to an external directory.
        let external = tmp.path().join("external");
        fs::create_dir(&external).unwrap();
        fs::set_permissions(&external, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir(&root.join("REPOS")).unwrap();
        std::os::unix::fs::symlink(&external, &root.join("REPOS")).unwrap();

        // Second call should succeed — it skips chmod on the symlinked child.
        ensure_nest_at(&root).unwrap();

        // The external directory's permissions should be unchanged (755, not 700).
        let mode = fs::metadata(&external).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o755,
            "symlinked child's target should not be chmod'd"
        );
    }

    fn make_persona(id: &str, display_name: &str) -> PersonaRecord {
        PersonaRecord {
            id: id.to_string(),
            display_name: display_name.to_string(),
            avatar_url: None,
            system_prompt: String::new(),
            provider: None,
            model: None,
            name_pool: vec![],
            is_builtin: false,
            is_active: true,
            source_pack: None,
            source_pack_persona_slug: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn make_agent(name: &str, persona_id: Option<&str>) -> ManagedAgentRecord {
        ManagedAgentRecord {
            pubkey: String::new(),
            name: name.to_string(),
            persona_id: persona_id.map(|s| s.to_string()),
            private_key_nsec: String::new(),
            auth_tag: None,
            relay_url: String::new(),
            acp_command: String::new(),
            agent_command: String::new(),
            agent_args: vec![],
            mcp_command: String::new(),
            turn_timeout_seconds: 0,
            idle_timeout_seconds: None,
            max_turn_duration_seconds: None,
            parallelism: 1,
            system_prompt: None,
            model: None,
            mcp_toolsets: None,
            start_on_app_launch: false,
            runtime_pid: None,
            backend: BackendKind::default(),
            backend_agent_id: None,
            provider_binary_path: None,
            persona_pack_path: None,
            persona_name_in_pack: None,
            created_at: String::new(),
            updated_at: String::new(),
            last_started_at: None,
            last_stopped_at: None,
            last_exit_code: None,
            last_error: None,
            respond_to: RespondTo::default(),
            respond_to_allowlist: vec![],
        }
    }

    #[test]
    fn test_render_dynamic_section_with_agents() {
        let personas = vec![make_persona("p1", "Builder")];
        let agents = vec![make_agent("Kit", Some("p1"))];
        let output = render_dynamic_section(&personas, &agents, "ws://example.com:3000");
        assert!(output.contains("| Kit | Builder | @Kit |"));
        assert!(output.contains("## CLI Quick Reference"));
    }

    #[test]
    fn test_render_dynamic_section_empty() {
        let output = render_dynamic_section(&[], &[], "ws://example.com:3000");
        assert!(output.contains("No agents deployed yet"));
    }

    #[test]
    fn test_render_dynamic_section_agent_no_persona() {
        let personas = vec![make_persona("p1", "Builder")];
        let agents = vec![make_agent("Scout", Some("nonexistent"))];
        let output = render_dynamic_section(&personas, &agents, "ws://example.com:3000");
        assert!(output.contains("| Scout | — | @Scout |"));
    }

    #[test]
    fn test_upsert_managed_section_with_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("AGENTS.md");
        fs::write(
            &file,
            "# Header\n\nsome content\n\n<!-- BEGIN SPROUT MANAGED — regenerated automatically, do not edit below -->\nold section\n<!-- END SPROUT MANAGED -->\n\nafter\n",
        )
        .unwrap();

        upsert_managed_section(&file, "new section").unwrap();

        let result = fs::read_to_string(&file).unwrap();
        assert!(result.contains("<!-- BEGIN SPROUT MANAGED"));
        assert!(result.contains("<!-- END SPROUT MANAGED -->"));
        assert!(result.contains("new section"));
        assert!(!result.contains("old section"));
        assert!(result.contains("# Header"));
        assert!(result.contains("some content"));
        assert!(result.contains("after"));
    }

    #[test]
    fn test_upsert_managed_section_without_markers() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("AGENTS.md");
        fs::write(&file, "# Header\n\nexisting content\n").unwrap();

        upsert_managed_section(&file, "injected section").unwrap();

        let result = fs::read_to_string(&file).unwrap();
        assert!(result.contains("# Header"));
        assert!(result.contains("existing content"));
        assert!(result.contains("<!-- BEGIN SPROUT MANAGED"));
        assert!(result.contains("<!-- END SPROUT MANAGED -->"));
        assert!(result.contains("injected section"));
        let begin_pos = result.find("<!-- BEGIN SPROUT MANAGED").unwrap();
        let header_pos = result.find("# Header").unwrap();
        assert!(
            header_pos < begin_pos,
            "original content should precede the managed section"
        );
    }

    #[test]
    fn test_upsert_managed_section_no_tmp_leftover() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("AGENTS.md");
        fs::write(&file, "# Header\n").unwrap();

        upsert_managed_section(&file, "content").unwrap();

        let tmp_path = file.with_extension("md.tmp");
        assert!(
            !tmp_path.exists(),
            ".tmp file should not exist after successful upsert"
        );
    }
}
