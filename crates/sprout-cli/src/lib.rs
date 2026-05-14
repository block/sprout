mod client;
mod commands;
mod error;
mod validate;

use clap::{Parser, Subcommand};
use client::SproutClient;
use error::CliError;
use nostr::Keys;

/// Run the Sprout CLI from raw arguments (including `argv[0]`).
///
/// Returns a process exit code (0 = success).
///
/// # Example
///
/// ```ignore
/// let code = sprout_cli::run_from_args(std::env::args()).await;
/// std::process::exit(code);
/// ```
pub async fn run_from_args<I, S>(args: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            if e.use_stderr() {
                error::print_error(&CliError::Usage(e.to_string()));
                return 1;
            } else {
                // --help and --version: print normally (intentional human output)
                let _ = e.print();
                return 0;
            }
        }
    };
    match run(cli).await {
        Ok(()) => 0,
        Err(e) => {
            error::print_error(&e);
            error::exit_code(&e)
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "sprout", about = "Sprout CLI — interact with a Sprout relay")]
struct Cli {
    /// Relay URL (http:// or https://). Overrides SPROUT_RELAY_URL env var.
    #[arg(
        long,
        env = "SPROUT_RELAY_URL",
        default_value = "http://localhost:3000"
    )]
    relay: String,

    /// Nostr private key (hex or nsec). This is the CLI's identity.
    #[arg(long, env = "SPROUT_PRIVATE_KEY")]
    private_key: Option<String>,

    /// NIP-OA auth tag JSON (owner attestation). Injected into every signed event.
    #[arg(long, env = "SPROUT_AUTH_TAG")]
    auth_tag: Option<String>,

    #[command(subcommand)]
    command: Cmd,
}

// ---------------------------------------------------------------------------
// Subcommand groups
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
enum Cmd {
    /// Send, read, search, and manage messages
    #[command(subcommand)]
    Messages(MessagesCmd),
    /// Create, configure, and manage channels
    #[command(subcommand)]
    Channels(ChannelsCmd),
    /// Get and set channel canvas documents
    #[command(subcommand)]
    Canvas(CanvasCmd),
    /// Add, remove, and list emoji reactions
    #[command(subcommand)]
    Reactions(ReactionsCmd),
    /// List, open, and manage direct messages
    #[command(subcommand)]
    Dms(DmsCmd),
    /// Look up users and manage profiles and presence
    #[command(subcommand)]
    Users(UsersCmd),
    /// Create, trigger, and manage workflows
    #[command(subcommand)]
    Workflows(WorkflowsCmd),
    /// Read the activity feed
    #[command(subcommand)]
    Feed(FeedCmd),
    /// Publish notes and manage the social graph (NIP-01/02)
    #[command(subcommand)]
    Social(SocialCmd),
    /// Announce and discover git repositories (NIP-34)
    #[command(subcommand)]
    Repos(ReposCmd),
    /// Upload files to the relay's Blossom store
    #[command(subcommand)]
    Upload(UploadCmd),
    /// Persona pack operations (local, no relay connection needed)
    #[command(subcommand)]
    Pack(PackCmd),
}

// ---------------------------------------------------------------------------
// Messages subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum MessagesCmd {
    /// Send a message to a channel
    #[command(
        after_help = "Examples:\n  sprout messages send --channel <UUID> --content \"hello\"\n  sprout messages send --channel <UUID> --content \"@alice check this\" --mention alice"
    )]
    Send {
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
        /// Attach file(s) — uploads and includes as imeta tags
        #[arg(long = "file")]
        files: Vec<String>,
    },
    /// Send a code diff / patch to a channel
    SendDiff {
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
    /// Edit a previously sent message
    Edit {
        /// Event ID of the message to edit (64-char hex)
        #[arg(long)]
        event: String,
        /// New message content
        #[arg(long)]
        content: String,
    },
    /// Delete a message by event ID
    Delete {
        #[arg(long)]
        event: String,
    },
    /// Retrieve messages from a channel
    #[command(
        after_help = "Examples:\n  sprout messages get --channel <UUID>\n  sprout messages get --channel <UUID> --limit 50 --kinds 1,1984"
    )]
    Get {
        #[arg(long)]
        channel: String,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        before: Option<i64>,
        #[arg(long)]
        since: Option<i64>,
        /// Comma-separated event kinds to filter (e.g. 1,1984)
        #[arg(long)]
        kinds: Option<String>,
    },
    /// Get a message thread (replies to a root message)
    Thread {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        event: String,
        #[arg(long)]
        depth_limit: Option<u32>,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Full-text search across messages
    Search {
        #[arg(long)]
        query: String,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Upvote or downvote a forum post
    Vote {
        /// Event ID of the post to vote on (64-char hex)
        #[arg(long)]
        event: String,
        /// Vote direction: "up" or "down"
        #[arg(long)]
        direction: String,
    },
}

// ---------------------------------------------------------------------------
// Channels subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum ChannelsCmd {
    /// List channels visible to the current identity
    #[command(
        after_help = "Examples:\n  sprout channels list\n  sprout channels list --visibility open"
    )]
    List {
        /// Filter by visibility (e.g. open, closed)
        #[arg(long)]
        visibility: Option<String>,
        /// Only show channels where the current identity is a member
        #[arg(long, default_value_t = false)]
        member: bool,
    },
    /// Get details for a single channel
    Get {
        #[arg(long)]
        channel: String,
    },
    /// Create a new channel
    #[command(
        after_help = "Examples:\n  sprout channels create --name general --type stream --visibility open\n  sprout channels create --name design --type forum --visibility open --description \"Design discussions\""
    )]
    Create {
        #[arg(long)]
        name: String,
        #[arg(long = "type")]
        channel_type: String,
        #[arg(long)]
        visibility: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// Update channel name or description
    Update {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// Set the channel topic
    Topic {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        topic: String,
    },
    /// Set the channel purpose
    Purpose {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        purpose: String,
    },
    /// Join a channel
    Join {
        #[arg(long)]
        channel: String,
    },
    /// Leave a channel
    Leave {
        #[arg(long)]
        channel: String,
    },
    /// Archive a channel
    Archive {
        #[arg(long)]
        channel: String,
    },
    /// Unarchive a channel
    Unarchive {
        #[arg(long)]
        channel: String,
    },
    /// Delete a channel permanently
    Delete {
        #[arg(long)]
        channel: String,
    },
    /// List members of a channel
    Members {
        #[arg(long)]
        channel: String,
    },
    /// Add a member to a channel
    #[command(name = "add-member")]
    AddMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
        #[arg(long)]
        role: Option<String>,
    },
    /// Remove a member from a channel
    #[command(name = "remove-member")]
    RemoveMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
    },
}

// ---------------------------------------------------------------------------
// Canvas subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum CanvasCmd {
    /// Get the canvas document for a channel
    Get {
        #[arg(long)]
        channel: String,
    },
    /// Set (replace) the canvas document for a channel
    Set {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        content: String,
    },
}

// ---------------------------------------------------------------------------
// Reactions subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum ReactionsCmd {
    /// Add an emoji reaction to a message
    Add {
        #[arg(long)]
        event: String,
        #[arg(long)]
        emoji: String,
    },
    /// Remove an emoji reaction from a message
    Remove {
        #[arg(long)]
        event: String,
        #[arg(long)]
        emoji: String,
    },
    /// List reactions on a message
    Get {
        #[arg(long)]
        event: String,
    },
}

// ---------------------------------------------------------------------------
// DMs subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum DmsCmd {
    /// List direct message conversations
    List {
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Open a new direct message with one or more users
    Open {
        #[arg(long = "pubkey")]
        pubkeys: Vec<String>,
    },
    /// Add a member to an existing DM conversation
    AddMember {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        pubkey: String,
    },
}

// ---------------------------------------------------------------------------
// Users subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum UsersCmd {
    /// Look up user profiles by pubkey or name
    Get {
        #[arg(long = "pubkey")]
        pubkeys: Vec<String>,
        /// Search by display name (case-insensitive substring match)
        #[arg(long = "name")]
        name: Option<String>,
    },
    /// Update the current identity's profile
    #[command(name = "set-profile")]
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
    /// Get presence status for users
    Presence {
        #[arg(long)]
        pubkeys: String,
    },
    /// Set your presence status (online/away/offline)
    #[command(name = "set-presence")]
    SetPresence {
        #[arg(long)]
        status: String,
    },
}

// ---------------------------------------------------------------------------
// Workflows subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum WorkflowsCmd {
    /// List workflows in a channel
    List {
        #[arg(long)]
        channel: String,
    },
    /// Get details for a single workflow
    Get {
        #[arg(long)]
        workflow: String,
    },
    /// Create a workflow from a YAML definition
    Create {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        yaml: String,
    },
    /// Update a workflow's YAML definition
    Update {
        #[arg(long)]
        channel: String,
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        yaml: String,
    },
    /// Delete a workflow
    Delete {
        #[arg(long)]
        workflow: String,
    },
    /// Trigger a workflow run
    #[command(after_help = "Examples:\n  sprout workflows trigger --workflow <UUID>")]
    Trigger {
        #[arg(long)]
        workflow: String,
    },
    /// List runs for a workflow
    Runs {
        #[arg(long)]
        workflow: String,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Approve or deny a workflow step
    #[command(
        after_help = "Examples:\n  sprout workflows approve --token <UUID>\n  sprout workflows approve --token <UUID> --no-approved --note \"needs revision\""
    )]
    Approve {
        /// The approval token UUID (from the approval request)
        #[arg(long)]
        token: String,
        /// Approve the step (pass --no-approved to deny)
        #[arg(long, default_value_t = true)]
        approved: bool,
        /// Optional note to include with the approval/denial
        #[arg(long)]
        note: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Feed subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum FeedCmd {
    /// Get recent activity feed entries
    Get {
        #[arg(long)]
        since: Option<i64>,
        /// Maximum number of results to return
        #[arg(long)]
        limit: Option<u32>,
        #[arg(long)]
        types: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Social subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum SocialCmd {
    /// Publish a text note (NIP-01 kind:1)
    #[command(name = "publish-note")]
    PublishNote {
        /// Text content of the note.
        #[arg(long)]
        content: String,
        /// 64-char hex event ID to reply to.
        #[arg(long)]
        reply_to: Option<String>,
    },
    /// Set your contact list (NIP-02 kind:3)
    #[command(name = "set-contact-list")]
    SetContactList {
        /// JSON array of contacts: [{"pubkey":"hex","relay_url":"...","petname":"..."}]
        #[arg(long)]
        contacts: String,
    },
    /// Get a single event by ID
    #[command(name = "get-event")]
    GetEvent {
        /// 64-char hex event ID.
        #[arg(long)]
        event: String,
    },
    /// Get recent notes published by a user
    #[command(name = "get-user-notes")]
    GetUserNotes {
        /// 64-char hex pubkey of the author.
        #[arg(long)]
        pubkey: String,
        /// Maximum number of notes to return (default 50, max 100).
        #[arg(long)]
        limit: Option<u32>,
        /// Unix timestamp cursor — return notes created before this time.
        #[arg(long)]
        before: Option<i64>,
    },
    /// Get a user's contact list
    #[command(name = "get-contact-list")]
    GetContactList {
        /// 64-char hex pubkey.
        #[arg(long)]
        pubkey: String,
    },
}

// ---------------------------------------------------------------------------
// Repos subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum ReposCmd {
    /// Announce a git repository (NIP-34)
    Create {
        /// Repository identifier: [a-zA-Z0-9._-]{1,64}
        #[arg(long)]
        id: String,
        /// Human-readable display name
        #[arg(long)]
        name: Option<String>,
        /// Repository description
        #[arg(long)]
        description: Option<String>,
        /// Clone URL(s) — can be specified multiple times
        #[arg(long = "clone")]
        clone_urls: Vec<String>,
        /// Web browsing URL
        #[arg(long)]
        web: Option<String>,
        /// Preferred relay(s) — can be specified multiple times
        #[arg(long = "relay")]
        relays: Vec<String>,
    },
    /// Get a repository announcement
    Get {
        /// Repository identifier (d-tag)
        #[arg(long)]
        id: String,
        /// Owner pubkey (64-char hex). Omit to match any owner.
        #[arg(long)]
        owner: Option<String>,
    },
    /// List repository announcements
    List {
        /// Owner pubkey (64-char hex). Omit for your repos.
        #[arg(long)]
        owner: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<u32>,
    },
}

// ---------------------------------------------------------------------------
// Upload subcommands
// ---------------------------------------------------------------------------

#[derive(Subcommand)]
pub enum UploadCmd {
    /// Upload a file to the relay's Blossom store
    File {
        /// Path to the file to upload
        #[arg(long)]
        file: String,
    },
}

// ---------------------------------------------------------------------------
// Pack subcommands (local, no relay connection needed)
// ---------------------------------------------------------------------------

/// Subcommands for `sprout pack`.
#[derive(Subcommand)]
enum PackCmd {
    /// Validate a persona pack directory
    Validate {
        /// Path to the pack directory
        path: String,
    },
    /// Inspect a persona pack — show metadata and effective config
    Inspect {
        /// Path to the pack directory
        path: String,
    },
}

// ---------------------------------------------------------------------------
// Command dispatch
// ---------------------------------------------------------------------------

async fn run(cli: Cli) -> Result<(), CliError> {
    let relay_url = client::normalize_relay_url(&cli.relay);

    // Pack commands are local-only — no relay connection needed.
    if let Cmd::Pack(ref sub) = cli.command {
        return match sub {
            PackCmd::Validate { path } => commands::pack::cmd_validate(path),
            PackCmd::Inspect { path } => commands::pack::cmd_inspect(path),
        };
    }

    // Auth: private key is required for all relay operations.
    // The keypair IS the identity — no tokens, no other auth.
    let private_key_str = cli.private_key.ok_or_else(|| {
        CliError::Auth("SPROUT_PRIVATE_KEY is required (use --private-key or set env var)".into())
    })?;
    let keys = Keys::parse(&private_key_str)
        .map_err(|e| CliError::Key(format!("invalid SPROUT_PRIVATE_KEY: {e}")))?;

    // NIP-OA: parse and verify the auth tag if provided.
    let (auth_tag, auth_tag_json) = match cli.auth_tag {
        Some(ref json) if !json.is_empty() => {
            let tag = sprout_sdk::nip_oa::parse_auth_tag(json)
                .map_err(|e| CliError::Auth(format!("SPROUT_AUTH_TAG is malformed: {e}")))?;
            sprout_sdk::nip_oa::verify_auth_tag(json, &keys.public_key()).map_err(|e| {
                CliError::Auth(format!(
                    "SPROUT_AUTH_TAG verification failed for pubkey {}: {e}",
                    keys.public_key().to_hex()
                ))
            })?;
            (Some(tag), Some(json.clone()))
        }
        _ => (None, None),
    };

    let client = SproutClient::new(relay_url, keys, auth_tag, auth_tag_json)?;

    match cli.command {
        Cmd::Messages(sub) => match sub {
            MessagesCmd::Send {
                channel,
                content,
                kind,
                reply_to,
                broadcast,
                mentions,
                files,
            } => {
                commands::messages::cmd_send_message(
                    &client,
                    commands::messages::SendMessageParams {
                        channel_id: channel,
                        content,
                        kind,
                        reply_to,
                        broadcast,
                        mentions,
                        files,
                    },
                )
                .await
            }
            MessagesCmd::SendDiff {
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
            MessagesCmd::Edit { event, content } => {
                commands::messages::cmd_edit_message(&client, &event, &content).await
            }
            MessagesCmd::Delete { event } => {
                commands::messages::cmd_delete_message(&client, &event).await
            }
            MessagesCmd::Get {
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
            MessagesCmd::Thread {
                channel,
                event,
                depth_limit,
                limit,
            } => {
                commands::messages::cmd_get_thread(&client, &channel, &event, depth_limit, limit)
                    .await
            }
            MessagesCmd::Search { query, limit } => {
                commands::messages::cmd_search(&client, &query, limit).await
            }
            MessagesCmd::Vote { event, direction } => {
                commands::messages::cmd_vote_on_post(&client, &event, &direction).await
            }
        },

        Cmd::Channels(sub) => match sub {
            ChannelsCmd::List { visibility, member } => {
                commands::channels::cmd_list_channels(&client, visibility.as_deref(), Some(member))
                    .await
            }
            ChannelsCmd::Get { channel } => {
                commands::channels::cmd_get_channel(&client, &channel).await
            }
            ChannelsCmd::Create {
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
            ChannelsCmd::Update {
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
            ChannelsCmd::Topic { channel, topic } => {
                commands::channels::cmd_set_channel_topic(&client, &channel, &topic).await
            }
            ChannelsCmd::Purpose { channel, purpose } => {
                commands::channels::cmd_set_channel_purpose(&client, &channel, &purpose).await
            }
            ChannelsCmd::Join { channel } => {
                commands::channels::cmd_join_channel(&client, &channel).await
            }
            ChannelsCmd::Leave { channel } => {
                commands::channels::cmd_leave_channel(&client, &channel).await
            }
            ChannelsCmd::Archive { channel } => {
                commands::channels::cmd_archive_channel(&client, &channel).await
            }
            ChannelsCmd::Unarchive { channel } => {
                commands::channels::cmd_unarchive_channel(&client, &channel).await
            }
            ChannelsCmd::Delete { channel } => {
                commands::channels::cmd_delete_channel(&client, &channel).await
            }
            ChannelsCmd::Members { channel } => {
                commands::channels::cmd_list_channel_members(&client, &channel).await
            }
            ChannelsCmd::AddMember {
                channel,
                pubkey,
                role,
            } => {
                commands::channels::cmd_add_channel_member(
                    &client,
                    &channel,
                    &pubkey,
                    role.as_deref(),
                )
                .await
            }
            ChannelsCmd::RemoveMember { channel, pubkey } => {
                commands::channels::cmd_remove_channel_member(&client, &channel, &pubkey).await
            }
        },

        Cmd::Canvas(sub) => match sub {
            CanvasCmd::Get { channel } => {
                commands::channels::cmd_get_canvas(&client, &channel).await
            }
            CanvasCmd::Set { channel, content } => {
                commands::channels::cmd_set_canvas(&client, &channel, &content).await
            }
        },

        Cmd::Reactions(sub) => match sub {
            ReactionsCmd::Add { event, emoji } => {
                commands::reactions::cmd_add_reaction(&client, &event, &emoji).await
            }
            ReactionsCmd::Remove { event, emoji } => {
                commands::reactions::cmd_remove_reaction(&client, &event, &emoji).await
            }
            ReactionsCmd::Get { event } => {
                commands::reactions::cmd_get_reactions(&client, &event).await
            }
        },

        Cmd::Dms(sub) => match sub {
            DmsCmd::List { limit } => commands::dms::cmd_list_dms(&client, limit).await,
            DmsCmd::Open { pubkeys } => commands::dms::cmd_open_dm(&client, &pubkeys).await,
            DmsCmd::AddMember { channel, pubkey } => {
                commands::dms::cmd_add_dm_member(&client, &channel, &pubkey).await
            }
        },

        Cmd::Users(sub) => match sub {
            UsersCmd::Get { pubkeys, name } => {
                commands::users::cmd_get_users(&client, &pubkeys, name.as_deref()).await
            }
            UsersCmd::SetProfile {
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
            UsersCmd::Presence { pubkeys } => {
                commands::users::cmd_get_presence(&client, &pubkeys).await
            }
            UsersCmd::SetPresence { status } => {
                commands::users::cmd_set_presence(&client, &status).await
            }
        },

        Cmd::Workflows(sub) => match sub {
            WorkflowsCmd::List { channel } => {
                commands::workflows::cmd_list_workflows(&client, &channel).await
            }
            WorkflowsCmd::Get { workflow } => {
                commands::workflows::cmd_get_workflow(&client, &workflow).await
            }
            WorkflowsCmd::Create { channel, yaml } => {
                commands::workflows::cmd_create_workflow(&client, &channel, &yaml).await
            }
            WorkflowsCmd::Update {
                channel,
                workflow,
                yaml,
            } => {
                commands::workflows::cmd_update_workflow(&client, &channel, &workflow, &yaml).await
            }
            WorkflowsCmd::Delete { workflow } => {
                commands::workflows::cmd_delete_workflow(&client, &workflow).await
            }
            WorkflowsCmd::Trigger { workflow } => {
                commands::workflows::cmd_trigger_workflow(&client, &workflow).await
            }
            WorkflowsCmd::Runs { workflow, limit } => {
                commands::workflows::cmd_get_workflow_runs(&client, &workflow, limit).await
            }
            WorkflowsCmd::Approve {
                token,
                approved,
                note,
            } => {
                // approved is already a bool — no parse_bool_flag needed
                commands::workflows::cmd_approve_step(&client, &token, approved, note.as_deref())
                    .await
            }
        },

        Cmd::Feed(sub) => match sub {
            FeedCmd::Get {
                since,
                limit,
                types,
            } => commands::feed::cmd_get_feed(&client, since, limit, types.as_deref()).await,
        },

        Cmd::Social(sub) => match sub {
            SocialCmd::PublishNote { content, reply_to } => {
                commands::social::cmd_publish_note(&client, &content, reply_to.as_deref()).await
            }
            SocialCmd::SetContactList { contacts } => {
                commands::social::cmd_set_contact_list(&client, &contacts).await
            }
            SocialCmd::GetEvent { event } => commands::social::cmd_get_event(&client, &event).await,
            SocialCmd::GetUserNotes {
                pubkey,
                limit,
                before,
            } => commands::social::cmd_get_user_notes(&client, &pubkey, limit, before).await,
            SocialCmd::GetContactList { pubkey } => {
                commands::social::cmd_get_contact_list(&client, &pubkey).await
            }
        },

        Cmd::Repos(sub) => match sub {
            ReposCmd::Create {
                id,
                name,
                description,
                clone_urls,
                web,
                relays,
            } => {
                commands::repos::cmd_create_repo(
                    &client,
                    &id,
                    name.as_deref(),
                    description.as_deref(),
                    &clone_urls,
                    web.as_deref(),
                    &relays,
                )
                .await
            }
            ReposCmd::Get { id, owner } => {
                commands::repos::cmd_get_repo(&client, &id, owner.as_deref()).await
            }
            ReposCmd::List { owner, limit } => {
                commands::repos::cmd_list_repos(&client, owner.as_deref(), limit).await
            }
        },

        Cmd::Upload(sub) => match sub {
            UploadCmd::File { file } => {
                let desc = client.upload_file(&file).await?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&desc)
                        .map_err(|e| CliError::Other(e.to_string()))?
                );
                Ok(())
            }
        },

        Cmd::Pack(_) => unreachable!("handled above"),
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

    #[test]
    fn command_inventory_is_stable() {
        let expected_groups: Vec<&str> = vec![
            "canvas",
            "channels",
            "dms",
            "feed",
            "messages",
            "pack",
            "reactions",
            "repos",
            "social",
            "upload",
            "users",
            "workflows",
        ];

        let cmd = Cli::command();
        let mut actual: Vec<String> = cmd
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .filter(|n| n != "help")
            .collect();
        actual.sort();

        assert_eq!(
            actual.len(),
            expected_groups.len(),
            "Expected {} groups, got {}. Actual: {:?}",
            expected_groups.len(),
            actual.len(),
            actual
        );
        assert_eq!(
            actual, expected_groups,
            "Command group inventory drift detected"
        );
    }

    #[test]
    fn subcommand_counts_are_stable() {
        let expected: Vec<(&str, usize)> = vec![
            ("canvas", 2),
            ("channels", 14),
            ("dms", 3),
            ("feed", 1),
            ("messages", 8),
            ("pack", 2),
            ("reactions", 3),
            ("repos", 3),
            ("social", 5),
            ("upload", 1),
            ("users", 4),
            ("workflows", 8),
        ];

        let cmd = Cli::command();
        for (group_name, expected_count) in &expected {
            let group = cmd
                .get_subcommands()
                .find(|s| s.get_name() == *group_name)
                .unwrap_or_else(|| panic!("group '{}' not found", group_name));
            let actual_count = group
                .get_subcommands()
                .filter(|s| s.get_name() != "help")
                .count();
            assert_eq!(
                actual_count, *expected_count,
                "Group '{}': expected {} subcommands, got {}",
                group_name, expected_count, actual_count
            );
        }
    }
}
