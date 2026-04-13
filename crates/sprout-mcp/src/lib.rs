#![deny(unsafe_code)]
#![warn(missing_docs)]
//! # sprout-mcp
//!
//! MCP (Model Context Protocol) server that exposes [Sprout] — a Nostr-based enterprise
//! communications platform — as a set of tools consumable by AI agents.
//!
//! ## Overview
//!
//! `sprout-mcp` runs as a stdio MCP server. An agent host (e.g. Claude Desktop, Goose)
//! launches it as a subprocess and communicates over JSON-RPC on stdin/stdout. The server
//! maintains a persistent, authenticated WebSocket connection to a Sprout relay and a shared
//! HTTP client for REST API calls.
//!
//! ```text
//!  ┌─────────────┐  JSON-RPC (stdio)  ┌──────────────┐  NIP-42 WebSocket  ┌───────────────┐
//!  │  Agent Host │ ◄─────────────────► │  sprout-mcp  │ ◄─────────────────► │ Sprout Relay  │
//!  └─────────────┘                     └──────────────┘  REST (reqwest)     └───────────────┘
//! ```
//!
//! ## Connecting to the Relay
//!
//! On startup `sprout-mcp` reads three environment variables:
//!
//! | Variable             | Default                  | Description                                      |
//! |----------------------|--------------------------|--------------------------------------------------|
//! | `SPROUT_RELAY_URL`   | `ws://localhost:3000`    | WebSocket URL of the Sprout relay                |
//! | `SPROUT_PRIVATE_KEY` | *(generated)*            | `nsec…` Nostr private key for the agent identity |
//! | `SPROUT_API_TOKEN`   | *(none)*                 | Bearer token for REST auth (production mode)     |
//!
//! If `SPROUT_PRIVATE_KEY` is absent a fresh ephemeral keypair is generated and its public key
//! is printed to stderr. In production you should supply a stable key so the agent has a
//! consistent Nostr identity.
//!
//! Authentication follows [NIP-42]: the relay sends an `AUTH` challenge immediately after the
//! WebSocket handshake; the client signs it and sends back an `AUTH` event. When
//! `SPROUT_API_TOKEN` is set the token is embedded in the auth event tags so the relay can
//! verify the agent's API permissions.
//!
//! ## WebSocket Connection Management
//!
//! [`relay_client::RelayClient`] uses a background tokio task that owns the WebSocket
//! connection. The background task:
//!
//! - Responds to Ping frames immediately — preventing relay disconnects during long LLM turns
//! - Handles mid-session NIP-42 AUTH challenges automatically
//! - Reconnects with exponential backoff (1 s → 2 s → 4 s → … → 30 s cap) on any
//!   connection loss, without any action required from the caller
//! - Re-authenticates via NIP-42 after each reconnect
//! - Replays all active subscriptions after reconnect
//!
//! ```text
//! RelayClient (Clone)
//!   ├── cmd_tx: mpsc::Sender<RelayCommand>   ← send_event / subscribe / close
//!   └── bg_handle: JoinHandle<()>
//!         └── run_background_task()
//!               ├── ws.next()  → handle_ws_message()   // Ping→Pong, AUTH→respond, OK→resolve
//!               ├── cmd_rx     → handle_command()       // SendEvent, Subscribe, Close
//!               └── tick       → expire_timed_out()     // 10s timeouts
//! ```
//!
//! ## Available Tools
//!
//! 50 tools total, organized into toolsets. Tools are organized into toolsets. Set
//! `SPROUT_TOOLSETS` to control which are active (default: 27 core tools).
//!
//! ### Messaging (default toolset)
//! - **`send_current_reply`** — Reply in the current bound conversation context.
//! - **`send_message`** — Post a message to a specific channel.
//! - **`send_current_diff_reply`** — Reply with a diff in the current bound conversation context.
//! - **`send_diff_message`** — Post a diff-formatted message to a specific channel.
//! - **`edit_message`** — Edit an existing message.
//! - **`delete_message`** — Delete a message.
//! - **`get_messages`** — Fetch recent messages from a channel (default 50, max 200).
//! - **`get_thread`** — Fetch replies in a message thread.
//! - **`search`** — Full-text search across channels.
//! - **`get_feed`** — Retrieve the agent's personalized home feed (mentions, needs-action
//!   items, channel activity). Replaces the former `get_feed_mentions` / `get_feed_actions`.
//! - **`add_reaction`** / **`remove_reaction`** / **`get_reactions`** — Emoji reactions.
//!
//! ### Channels (default toolset)
//! - **`list_channels`** / **`get_channel`** — List or inspect channels.
//! - **`join_channel`** / **`leave_channel`** — Membership management.
//! - **`update_channel`** / **`set_channel_topic`** / **`set_channel_purpose`** — Metadata.
//! - **`open_dm`** — Open a direct-message channel.
//!
//! ### Channel Admin (`channel_admin` toolset)
//! - **`create_channel`** / **`archive_channel`** / **`unarchive_channel`**
//! - **`add_channel_member`** / **`remove_channel_member`** / **`list_channel_members`**
//!
//! ### Canvas (`canvas` toolset)
//! - **`get_canvas`** — Retrieve the shared canvas document for a channel.
//! - **`set_canvas`** — Write or replace the canvas document for a channel.
//!
//! ### Workflows
//! - **`trigger_workflow`** / **`approve_step`** — Trigger and approve steps (default toolset).
//! - **`list_workflows`** / **`create_workflow`** / **`update_workflow`** /
//!   **`delete_workflow`** / **`get_workflow_runs`** — Workflow admin (`workflow_admin` toolset).
//!
//! ## Example Configuration (Claude Desktop)
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "sprout": {
//!       "command": "/usr/local/bin/sprout-mcp-server",
//!       "env": {
//!         "SPROUT_RELAY_URL": "wss://relay.example.com",
//!         "SPROUT_PRIVATE_KEY": "nsec1...",
//!         "SPROUT_API_TOKEN": "your-api-token"
//!       }
//!     }
//!   }
//! }
//! ```
//!
//! [Sprout]: https://github.com/sprout-rs/sprout
//! [NIP-42]: https://github.com/nostr-protocol/nips/blob/master/42.md

// NOTE: `parse_relay_message`, `OkResponse`, and `RelayMessage` from `relay_client`
// are re-exported by `sprout-test-client`. Changes to these types are a breaking
// change for the test harness.

/// WebSocket client for the Sprout relay (NIP-42 auth, subscriptions, reconnect).
pub mod relay_client;
/// MCP tool implementations backed by the relay client.
pub mod server;
/// Toolset definitions and configuration for organizing MCP tools.
pub mod toolsets;
