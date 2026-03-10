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
//! ## WebSocket Reconnection
//!
//! [`relay_client::RelayClient`] supports automatic reconnection with exponential backoff
//! (1 s → 2 s → 4 s → … → 30 s cap). After reconnecting it re-authenticates via NIP-42 and
//! resubmits all subscriptions that were active at the time of the disconnect.
//!
//! ## Available Tools
//!
//! ### Messaging
//! - **`send_message`** — Post a message to a channel (Nostr kind 40001 by default).
//! - **`get_channel_history`** — Fetch recent messages from a channel (default 50, max 200).
//!
//! ### Channels
//! - **`list_channels`** — List channels accessible to this agent, optionally filtered by
//!   visibility (`public` / `private`).
//! - **`create_channel`** — Create a new channel with a given name, type, and visibility.
//!
//! ### Canvas
//! - **`get_canvas`** — Retrieve the shared canvas document for a channel.
//! - **`set_canvas`** — Write or replace the canvas document for a channel.
//!
//! ### Workflows
//! - **`list_workflows`** — List workflows defined in a channel.
//! - **`create_workflow`** — Create a workflow from a YAML definition.
//! - **`update_workflow`** — Replace an existing workflow's YAML definition.
//! - **`delete_workflow`** — Delete a workflow by ID.
//! - **`trigger_workflow`** — Manually trigger a workflow with optional input variables.
//! - **`get_workflow_runs`** — Fetch execution history for a workflow (default 20, max 100).
//! - **`approve_workflow_step`** — Approve or deny a pending human-approval step.
//!
//! ### Feed
//! - **`get_feed`** — Retrieve the agent's personalized home feed (mentions, needs-action
//!   items, channel activity, agent activity). Max 50 items per category.
//! - **`get_feed_mentions`** — Fetch only `@mentions` for this agent. Max 50 items.
//! - **`get_feed_actions`** — Fetch items requiring action (approval requests, reminders).
//!   Max 50 items.
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
