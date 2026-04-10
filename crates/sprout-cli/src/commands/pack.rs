//! `sprout pack` subcommands — local persona pack operations.
//!
//! These commands operate on local pack directories. No relay connection needed.

use std::path::Path;
use std::process;

use crate::error::CliError;

/// Run `sprout pack validate <path>`.
///
/// Calls `validate_pack()` from the persona crate, prints diagnostics,
/// and exits with the appropriate code:
/// - 0: valid (may have warnings)
/// - 1: errors found
pub fn cmd_validate(path: &str) -> Result<(), CliError> {
    let pack_dir = Path::new(path);
    if !pack_dir.exists() {
        eprintln!("error: path does not exist: {path}");
        process::exit(1);
    }
    if !pack_dir.is_dir() {
        eprintln!("error: not a directory: {path}");
        process::exit(1);
    }

    let report = sprout_persona::validate::validate_pack(pack_dir);

    for diag in &report.diagnostics {
        match diag {
            sprout_persona::validate::ValidationDiagnostic::Error(msg) => {
                eprintln!("  ERROR: {msg}");
            }
            sprout_persona::validate::ValidationDiagnostic::Warning(msg) => {
                eprintln!("  WARN:  {msg}");
            }
        }
    }

    if report.has_errors() {
        eprintln!("\nValidation failed.");
        process::exit(1);
    } else if report.has_warnings() {
        println!("Valid (with warnings).");
    } else {
        println!("Valid.");
    }

    Ok(())
}

/// Run `sprout pack inspect <path>`.
///
/// Loads and resolves a pack, then pretty-prints a summary of each persona's
/// effective configuration.
pub fn cmd_inspect(path: &str) -> Result<(), CliError> {
    let pack_dir = Path::new(path);
    if !pack_dir.exists() {
        eprintln!("error: path does not exist: {path}");
        process::exit(1);
    }
    if !pack_dir.is_dir() {
        eprintln!("error: not a directory: {path}");
        process::exit(1);
    }

    // Load the pack (validate + load in one step).
    let pack = sprout_persona::pack::load_pack(pack_dir)
        .map_err(|e| CliError::Other(format!("failed to load pack: {e}")))?;

    // Header
    println!("Pack: {} ({})", pack.manifest.name, pack.manifest.id);
    println!("Version: {}", pack.manifest.version);
    println!("Personas: {}", pack.personas.len());
    if pack.pack_instructions.is_some() {
        println!("Pack instructions: yes");
    }
    if pack.shared_mcp_config.is_some() {
        println!("Shared MCP config: yes");
    }
    if pack.skills_dir.is_some() {
        println!("Skills directory: yes");
    }
    println!();

    // Per-persona summary
    for persona in &pack.personas {
        println!("  {}", persona.name);
        println!("    Display: {}", persona.display_name);
        println!("    Description: {}", persona.description);

        if let Some(ref model) = persona.model {
            println!("    Model: {model}");
        }
        if let Some(temp) = persona.temperature {
            println!("    Temperature: {temp}");
        }
        if let Some(ctx) = persona.max_context_tokens {
            println!("    Max context tokens: {ctx}");
        }

        if !persona.subscribe.is_empty() {
            println!("    Subscribe: {}", persona.subscribe.join(", "));
        }

        if let Some(ref rt) = persona.respond_to {
            let mut parts = Vec::new();
            if rt.mentions {
                parts.push("mentions".to_string());
            }
            if !rt.keywords.is_empty() {
                parts.push(format!("keywords {:?}", rt.keywords));
            }
            if rt.all_messages {
                parts.push("all_messages".to_string());
            }
            if !parts.is_empty() {
                println!("    Respond to: {}", parts.join(" + "));
            }
        }

        println!("    Thread replies: {}", persona.thread_replies);
        println!("    Broadcast replies: {}", persona.broadcast_replies);

        if !persona.mcp_servers.is_empty() {
            println!("    MCP servers: {}", persona.mcp_servers.len());
        }

        if !persona.skills.is_empty() {
            println!("    Skills: {}", persona.skills.join(", "));
        }

        if let Some(ref avatar) = persona.avatar {
            println!("    Avatar: {avatar}");
        }

        let prompt_preview = if persona.prompt.len() > 80 {
            format!("{}...", &persona.prompt[..77])
        } else {
            persona.prompt.clone()
        };
        println!(
            "    Prompt: {} chars ({})",
            persona.prompt.len(),
            prompt_preview.replace('\n', " ")
        );
        println!();
    }

    Ok(())
}
