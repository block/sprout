#![deny(unsafe_code)]

mod client;
mod commands;
mod error;
mod validate;

use clap::{Parser, Subcommand};
use client::{Auth, SproutClient};
use error::CliError;

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "sprout", about = "Sprout CLI — interact with a Sprout relay")]
struct Cli {
    #[arg(
        long,
        env = "SPROUT_RELAY_URL",
        default_value = "http://localhost:3000"
    )]
    relay: String,

    #[arg(long, env = "SPROUT_API_TOKEN")]
    token: Option<String>,

    #[arg(long, env = "SPROUT_PRIVATE_KEY", hide = true)]
    private_key: Option<String>,

    #[arg(long, env = "SPROUT_PUBKEY")]
    pubkey: Option<String>,

    #[command(subcommand)]
    command: Cmd,
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum Cmd {
    // ---- Messages ----------------------------------------------------------
    /// Send a message to a channel
    SendMessage {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        content: String,
        #[arg(long)]
        kind: Option<u16>,
        #[arg(long)]
        reply_to: Option<String>,
        #[arg(long, default_value_t = false)]
        broadcast: bool,
        #[arg(long = "mention")]
        mentions: Vec<String>,
    },
    /// Send a diff/code-review message
    SendDiffMessage {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        diff: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        commit: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        parent_commit: Option<String>,
        #[arg(long)]
        source_branch: Option<String>,
        #[arg(long)]
        target_branch: Option<String>,
        #[arg(long)]
        pr: Option<u32>,
        #[arg(long)]
        lang: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        reply_to: Option<String>,
    },
    /// Delete a message by event ID
    DeleteMessage {
        #[arg(long)]
        event: String,
    },
    /// Get messages from a channel
    GetMessages {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        before: Option<i64>,
        #[arg(long)]
        since: Option<i64>,
        #[arg(long)]
        kinds: Option<String>,
    },
    /// Get a message thread
    GetThread {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        event: String,
        #[arg(long)]
        depth_limit: Option<u32>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Search messages
    Search {
        #[arg(long)]
        query: String,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Edit a message you previously sent
    EditMessage {
        /// Event ID of the message to edit (64-char hex)
        #[arg(long)]
        event: String,
        /// New message content
        #[arg(long)]
        content: String,
    },
    /// Vote on a forum post or comment (up or down)
    VoteOnPost {
        /// Event ID of the post to vote on (64-char hex)
        #[arg(long)]
        event: String,
        /// Vote direction: "up" or "down"
        #[arg(long)]
        direction: String,
    },

    // ---- Channels ----------------------------------------------------------
    /// List channels
    ListChannels {
        #[arg(long)]
        visibility: Option<String>,
        #[arg(long, default_value_t = false)]
        member: bool,
    },
    /// Get a channel by ID
    GetChannel {
        #[arg(long)]
        channel: String,
    },
    /// Create a new channel
    CreateChannel {
        #[arg(long)]
        name: String,
        #[arg(long = "type")]
        channel_type: String,
        #[arg(long)]
        visibility: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Update a channel's name or description
    UpdateChannel {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// Set a channel's topic
    SetChannelTopic {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        topic: String,
    },
    /// Set a channel's purpose
    SetChannelPurpose {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        purpose: String,
    },
    /// Join a channel
    JoinChannel {
        #[arg(long)]
        channel: String,
    },
    /// Leave a channel
    LeaveChannel {
        #[arg(long)]
        channel: String,
    },
    /// Archive a channel
    ArchiveChannel {
        #[arg(long)]
        channel: String,
    },
    /// Unarchive a channel
    UnarchiveChannel {
        #[arg(long)]
        channel: String,
    },
    /// Delete a channel
    DeleteChannel {
        #[arg(long)]
        channel: String,
    },
    /// List channel members
    ListChannelMembers {
        #[arg(long)]
        channel: String,
    },
    /// Add a member to a channel
    AddChannelMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
        #[arg(long)]
        role: Option<String>,
    },
    /// Remove a member from a channel
    RemoveChannelMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
    },
    /// Get a channel's canvas
    GetCanvas {
        #[arg(long)]
        channel: String,
    },
    /// Set a channel's canvas content
    SetCanvas {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        content: String,
    },

    // ---- Reactions ---------------------------------------------------------
    /// Add a reaction to a message
    AddReaction {
        #[arg(long)]
        event: String,
        #[arg(long)]
        emoji: String,
    },
    /// Remove a reaction from a message
    RemoveReaction {
        #[arg(long)]
        event: String,
        #[arg(long)]
        emoji: String,
    },
    /// Get reactions on a message
    GetReactions {
        #[arg(long)]
        event: String,
    },

    // ---- DMs ---------------------------------------------------------------
    /// List DM conversations
    ListDms {
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Open a DM with one or more users (1–8 pubkeys)
    OpenDm {
        #[arg(long = "pubkey")]
        pubkeys: Vec<String>,
    },
    /// Add a member to a DM group
    AddDmMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
    },

    // ---- Users -------------------------------------------------------------
    /// Get user profiles (0 = self, 1 = single, 2+ = batch)
    GetUsers {
        #[arg(long = "pubkey")]
        pubkeys: Vec<String>,
    },
    /// Update your profile
    SetProfile {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        avatar: Option<String>,
        #[arg(long)]
        about: Option<String>,
        #[arg(long)]
        nip05: Option<String>,
    },
    /// Get presence status for users (comma-separated pubkeys)
    GetPresence {
        #[arg(long)]
        pubkeys: String,
    },
    /// Set your presence status
    SetPresence {
        #[arg(long)]
        status: String,
    },
    /// Set who can add you to channels
    SetChannelAddPolicy {
        #[arg(long)]
        policy: String,
    },

    // ---- Workflows ---------------------------------------------------------
    /// List workflows in a channel
    ListWorkflows {
        #[arg(long)]
        channel: String,
    },
    /// Create a workflow in a channel
    CreateWorkflow {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        yaml: String,
    },
    /// Update a workflow
    UpdateWorkflow {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        yaml: String,
    },
    /// Delete a workflow
    DeleteWorkflow {
        #[arg(long)]
        workflow: String,
    },
    /// Trigger a workflow manually
    TriggerWorkflow {
        #[arg(long)]
        workflow: String,
    },
    /// Get workflow run history
    GetWorkflowRuns {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Get a workflow definition
    GetWorkflow {
        #[arg(long)]
        workflow: String,
    },
    /// Approve or deny a workflow approval step
    ApproveStep {
        #[arg(long)]
        token: String,
        /// Whether to approve: "true" or "false"
        #[arg(long)]
        approved: String,
        #[arg(long)]
        note: Option<String>,
    },

    // ---- Feed --------------------------------------------------------------
    /// Get your activity feed
    GetFeed {
        #[arg(long)]
        since: Option<i64>,
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        types: Option<String>,
    },

    // ---- Auth & Tokens -----------------------------------------------------
    /// Mint a long-lived API token (prints token to stdout)
    Auth,
    /// List your API tokens
    ListTokens,
    /// Delete an API token by ID
    DeleteToken {
        #[arg(long)]
        id: String,
    },
    /// Delete all your API tokens
    DeleteAllTokens,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            if e.use_stderr() {
                error::print_error(&CliError::Usage(e.to_string()));
                std::process::exit(1);
            } else {
                // --help and --version: print normally (intentional human output)
                let _ = e.print();
                std::process::exit(0);
            }
        }
    };
    match run(cli).await {
        Ok(()) => {}
        Err(e) => {
            error::print_error(&e);
            std::process::exit(error::exit_code(&e));
        }
    }
}

/// Parse a string flag that must be "true" or "false".
fn parse_bool_flag(flag_name: &str, value: &str) -> Result<bool, CliError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(CliError::Usage(format!(
            "{flag_name} must be 'true' or 'false' (got: {other})"
        ))),
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let relay_url = client::normalize_relay_url(&cli.relay);

    // Auth command is special — runs before SproutClient creation.
    // Passes --private-key flag; cmd_auth falls back to SPROUT_PRIVATE_KEY env var.
    if let Cmd::Auth = &cli.command {
        return commands::auth::cmd_auth(&relay_url, cli.private_key.as_deref()).await;
    }

    // Auth resolution: token > private_key (auto-mint) > pubkey > error
    //
    // When SPROUT_PRIVATE_KEY is set, auto_mint_token returns (token, keys).
    // The keys are retained on the client for signing write operations.
    let (auth, retained_keys) = if let Some(token) = cli.token {
        (Auth::Bearer(token), None)
    } else if let Some(key) = cli.private_key {
        let (minted, keys) = client::auto_mint_token(&relay_url, &key).await?;
        (Auth::Bearer(minted), Some(keys))
    } else if let Some(pk) = cli.pubkey {
        (Auth::DevMode(pk), None)
    } else {
        return Err(CliError::Auth(
            "Set SPROUT_API_TOKEN, SPROUT_PRIVATE_KEY, or SPROUT_PUBKEY".into(),
        ));
    };

    let client = {
        let c = SproutClient::new(relay_url, auth)?;
        match retained_keys {
            Some(k) => c.with_keys(k),
            None => c,
        }
    };

    match cli.command {
        // ---- Messages ------------------------------------------------------
        Cmd::SendMessage {
            channel,
            content,
            kind,
            reply_to,
            broadcast,
            mentions,
        } => {
            commands::messages::cmd_send_message(
                &client,
                &channel,
                &content,
                kind,
                reply_to.as_deref(),
                broadcast,
                &mentions,
            )
            .await
        }
        Cmd::SendDiffMessage {
            channel,
            diff,
            repo,
            commit,
            file,
            parent_commit,
            source_branch,
            target_branch,
            pr,
            lang,
            description,
            reply_to,
        } => {
            commands::messages::cmd_send_diff_message(
                &client,
                commands::messages::SendDiffParams {
                    channel_id: channel,
                    diff,
                    repo_url: repo,
                    commit_sha: commit,
                    file_path: file,
                    parent_commit_sha: parent_commit,
                    source_branch,
                    target_branch,
                    pr_number: pr,
                    language: lang,
                    description,
                    reply_to,
                },
            )
            .await
        }
        Cmd::DeleteMessage { event } => {
            commands::messages::cmd_delete_message(&client, &event).await
        }
        Cmd::GetMessages {
            channel,
            limit,
            before,
            since,
            kinds,
        } => {
            commands::messages::cmd_get_messages(
                &client,
                &channel,
                limit,
                before,
                since,
                kinds.as_deref(),
            )
            .await
        }
        Cmd::GetThread {
            channel,
            event,
            depth_limit,
            limit,
            cursor,
        } => {
            commands::messages::cmd_get_thread(
                &client,
                &channel,
                &event,
                depth_limit,
                limit,
                cursor.as_deref(),
            )
            .await
        }
        Cmd::Search { query, limit } => {
            commands::messages::cmd_search(&client, &query, limit).await
        }
        Cmd::EditMessage { event, content } => {
            commands::messages::cmd_edit_message(&client, &event, &content).await
        }
        Cmd::VoteOnPost { event, direction } => {
            commands::messages::cmd_vote_on_post(&client, &event, &direction).await
        }

        // ---- Channels ------------------------------------------------------
        Cmd::ListChannels { visibility, member } => {
            commands::channels::cmd_list_channels(&client, visibility.as_deref(), Some(member))
                .await
        }
        Cmd::GetChannel { channel } => commands::channels::cmd_get_channel(&client, &channel).await,
        Cmd::CreateChannel {
            name,
            channel_type,
            visibility,
            description,
        } => {
            commands::channels::cmd_create_channel(
                &client,
                &name,
                &channel_type,
                &visibility,
                description.as_deref(),
            )
            .await
        }
        Cmd::UpdateChannel {
            channel,
            name,
            description,
        } => {
            commands::channels::cmd_update_channel(
                &client,
                &channel,
                name.as_deref(),
                description.as_deref(),
            )
            .await
        }
        Cmd::SetChannelTopic { channel, topic } => {
            commands::channels::cmd_set_channel_topic(&client, &channel, &topic).await
        }
        Cmd::SetChannelPurpose { channel, purpose } => {
            commands::channels::cmd_set_channel_purpose(&client, &channel, &purpose).await
        }
        Cmd::JoinChannel { channel } => {
            commands::channels::cmd_join_channel(&client, &channel).await
        }
        Cmd::LeaveChannel { channel } => {
            commands::channels::cmd_leave_channel(&client, &channel).await
        }
        Cmd::ArchiveChannel { channel } => {
            commands::channels::cmd_archive_channel(&client, &channel).await
        }
        Cmd::UnarchiveChannel { channel } => {
            commands::channels::cmd_unarchive_channel(&client, &channel).await
        }
        Cmd::DeleteChannel { channel } => {
            commands::channels::cmd_delete_channel(&client, &channel).await
        }
        Cmd::ListChannelMembers { channel } => {
            commands::channels::cmd_list_channel_members(&client, &channel).await
        }
        Cmd::AddChannelMember {
            channel,
            pubkey,
            role,
        } => {
            commands::channels::cmd_add_channel_member(&client, &channel, &pubkey, role.as_deref())
                .await
        }
        Cmd::RemoveChannelMember { channel, pubkey } => {
            commands::channels::cmd_remove_channel_member(&client, &channel, &pubkey).await
        }
        Cmd::GetCanvas { channel } => commands::channels::cmd_get_canvas(&client, &channel).await,
        Cmd::SetCanvas { channel, content } => {
            commands::channels::cmd_set_canvas(&client, &channel, &content).await
        }

        // ---- Reactions -----------------------------------------------------
        Cmd::AddReaction { event, emoji } => {
            commands::reactions::cmd_add_reaction(&client, &event, &emoji).await
        }
        Cmd::RemoveReaction { event, emoji } => {
            commands::reactions::cmd_remove_reaction(&client, &event, &emoji).await
        }
        Cmd::GetReactions { event } => {
            commands::reactions::cmd_get_reactions(&client, &event).await
        }

        // ---- DMs -----------------------------------------------------------
        Cmd::ListDms { cursor, limit } => {
            commands::dms::cmd_list_dms(&client, cursor.as_deref(), limit).await
        }
        Cmd::OpenDm { pubkeys } => commands::dms::cmd_open_dm(&client, &pubkeys).await,
        Cmd::AddDmMember { channel, pubkey } => {
            commands::dms::cmd_add_dm_member(&client, &channel, &pubkey).await
        }

        // ---- Users ---------------------------------------------------------
        Cmd::GetUsers { pubkeys } => commands::users::cmd_get_users(&client, &pubkeys).await,
        Cmd::SetProfile {
            name,
            avatar,
            about,
            nip05,
        } => {
            commands::users::cmd_set_profile(
                &client,
                name.as_deref(),
                avatar.as_deref(),
                about.as_deref(),
                nip05.as_deref(),
            )
            .await
        }
        Cmd::GetPresence { pubkeys } => commands::users::cmd_get_presence(&client, &pubkeys).await,
        Cmd::SetPresence { status } => commands::users::cmd_set_presence(&client, &status).await,
        Cmd::SetChannelAddPolicy { policy } => {
            commands::users::cmd_set_channel_add_policy(&client, &policy).await
        }

        // ---- Workflows -----------------------------------------------------
        Cmd::ListWorkflows { channel } => {
            commands::workflows::cmd_list_workflows(&client, &channel).await
        }
        Cmd::CreateWorkflow { channel, yaml } => {
            commands::workflows::cmd_create_workflow(&client, &channel, &yaml).await
        }
        Cmd::UpdateWorkflow { workflow, yaml } => {
            commands::workflows::cmd_update_workflow(&client, &workflow, &yaml).await
        }
        Cmd::DeleteWorkflow { workflow } => {
            commands::workflows::cmd_delete_workflow(&client, &workflow).await
        }
        Cmd::TriggerWorkflow { workflow } => {
            commands::workflows::cmd_trigger_workflow(&client, &workflow).await
        }
        Cmd::GetWorkflowRuns { workflow, limit } => {
            commands::workflows::cmd_get_workflow_runs(&client, &workflow, limit).await
        }
        Cmd::GetWorkflow { workflow } => {
            commands::workflows::cmd_get_workflow(&client, &workflow).await
        }
        Cmd::ApproveStep {
            token,
            approved,
            note,
        } => {
            let approved = parse_bool_flag("--approved", &approved)?;
            commands::workflows::cmd_approve_step(&client, &token, approved, note.as_deref()).await
        }

        // ---- Feed ----------------------------------------------------------
        Cmd::GetFeed {
            since,
            limit,
            types,
        } => commands::feed::cmd_get_feed(&client, since, limit, types.as_deref()).await,

        // ---- Auth & Tokens -------------------------------------------------
        Cmd::Auth => unreachable!("handled above"),
        Cmd::ListTokens => commands::auth::cmd_list_tokens(&client).await,
        Cmd::DeleteToken { id } => commands::auth::cmd_delete_token(&client, &id).await,
        Cmd::DeleteAllTokens => commands::auth::cmd_delete_all_tokens(&client).await,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// Smoke test: CLI definition is valid and parseable.
    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// Regression: parse_bool_flag rejects values other than "true"/"false".
    #[test]
    fn parse_bool_flag_accepts_true() {
        assert!(super::parse_bool_flag("--approved", "true").unwrap());
    }

    #[test]
    fn parse_bool_flag_accepts_false() {
        assert!(!super::parse_bool_flag("--approved", "false").unwrap());
    }

    #[test]
    fn parse_bool_flag_rejects_invalid() {
        let err = super::parse_bool_flag("--approved", "maybe").unwrap_err();
        match err {
            super::CliError::Usage(msg) => {
                assert!(msg.contains("must be 'true' or 'false'"), "got: {msg}");
                assert!(msg.contains("maybe"), "got: {msg}");
            }
            other => panic!("expected Usage error, got: {other:?}"),
        }
    }

    #[test]
    fn parse_bool_flag_rejects_empty() {
        assert!(super::parse_bool_flag("--approved", "").is_err());
    }

    /// Parity: the CLI exposes exactly the expected 48 commands.
    /// If a command is added or removed, this test forces a conscious update.
    #[test]
    fn command_inventory_is_48() {
        let expected: Vec<&str> = vec![
            "add-channel-member",
            "add-dm-member",
            "add-reaction",
            "approve-step",
            "archive-channel",
            "auth",
            "create-channel",
            "create-workflow",
            "delete-all-tokens",
            "delete-channel",
            "delete-message",
            "delete-token",
            "delete-workflow",
            "edit-message",
            "get-canvas",
            "get-channel",
            "get-feed",
            "get-messages",
            "get-presence",
            "get-reactions",
            "get-thread",
            "get-users",
            "get-workflow",
            "get-workflow-runs",
            "join-channel",
            "leave-channel",
            "list-channel-members",
            "list-channels",
            "list-dms",
            "list-tokens",
            "list-workflows",
            "open-dm",
            "remove-channel-member",
            "remove-reaction",
            "search",
            "send-diff-message",
            "send-message",
            "set-canvas",
            "set-channel-add-policy",
            "set-channel-purpose",
            "set-channel-topic",
            "set-presence",
            "set-profile",
            "trigger-workflow",
            "unarchive-channel",
            "update-channel",
            "update-workflow",
            "vote-on-post",
        ];

        let cmd = Cli::command();
        let mut actual: Vec<String> = cmd
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .filter(|n| n != "help") // clap auto-adds "help"
            .collect();
        actual.sort();

        assert_eq!(
            actual.len(),
            48,
            "Expected 48 commands, got {}. Actual: {:?}",
            actual.len(),
            actual
        );
        assert_eq!(actual, expected, "Command inventory drift detected");
    }
}
