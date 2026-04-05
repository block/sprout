// ABOUTME: UniFFI-exposed Rust core for Sprout iOS/Android mobile apps.
// ABOUTME: Single SproutClient entry point wrapping auth, WebSocket, HTTP, and local storage.

#![deny(unsafe_code)]
#![warn(missing_docs)]

//! `sprout-mobile` — shared Rust core for Sprout iOS and Android apps.
//!
//! Exposes a single [`SproutClient`] object via UniFFI that handles all
//! networking, authentication, event signing, and local caching. Native shells
//! (SwiftUI / Jetpack Compose) call into this library and receive push events
//! through the [`SproutEventListener`] callback interface.

uniffi::setup_scaffolding!();

mod converters;
/// Error type used across the mobile core.
pub mod error;
mod runtime;
/// Data transfer types exposed over FFI.
pub mod types;

pub mod auth;
/// REST API client surface.
pub mod http;
/// Local SQLite cache surface.
pub mod store;
/// WebSocket connection and subscriptions.
pub mod ws;

use std::sync::Arc;

use nostr::Keys;
use tokio::sync::RwLock;

use auth::keys::KeyManager;
use auth::token::{self, TokenCache};
use error::SproutError;
use http::HttpClient;
use store::Store;
use types::*;
use ws::WsManager;

/// The main entry point for all Sprout mobile operations.
///
/// Owns the Tokio runtime, WebSocket connection, HTTP client, key material,
/// and local SQLite cache. Native code interacts exclusively through this object.
#[derive(uniffi::Object)]
pub struct SproutClient {
    _runtime: runtime::Runtime,
    http: HttpClient,
    ws: WsManager,
    store: Arc<Store>,
    key_manager: RwLock<Option<KeyManager>>,
    api_token: RwLock<Option<String>>,
}

#[uniffi::export]
impl SproutClient {
    /// Create a new client. Call [`connect`] to establish a relay connection.
    #[uniffi::constructor]
    pub fn new(config: ClientConfig) -> Result<Arc<Self>, SproutError> {
        let _runtime = runtime::Runtime::new()?;

        let http = HttpClient::new(&config.relay_url)?;
        let ws = WsManager::new(&config.relay_url);
        let store = Arc::new(Store::open(&config.db_path)?);

        // Import key material if provided.
        let key_manager = if let Some(ref nsec) = config.nsec_or_hex {
            Some(KeyManager::from_nsec_or_hex(nsec)?)
        } else {
            None
        };

        // Load or use provided API token.
        let api_token = if let Some(ref token) = config.api_token {
            Some(token.clone())
        } else {
            TokenCache::load(&store)?
        };

        Ok(Arc::new(Self {
            _runtime,
            http,
            ws,
            store,
            key_manager: RwLock::new(key_manager),
            api_token: RwLock::new(api_token),
        }))
    }

    // ── Connection lifecycle ─────────────────────────────────────────────────

    /// Connect to the relay and authenticate via WebSocket.
    pub async fn connect(&self) -> Result<(), SproutError> {
        let km = self.key_manager.read().await;
        let keys = km.as_ref().ok_or(SproutError::AuthRequired)?.keys().clone();
        drop(km);

        // Ensure we have an API token (auto-mint if needed).
        self.ensure_token(&keys).await?;

        let token = self.api_token.read().await.clone();
        self.ws.set_keys(keys).await;
        if let Some(ref t) = token {
            self.ws.set_api_token(t.clone()).await;
        }
        self.ws.connect().await
    }

    /// Disconnect from the relay gracefully.
    pub async fn disconnect(&self) -> Result<(), SproutError> {
        self.ws.disconnect().await;
        Ok(())
    }

    /// Current WebSocket connection state.
    pub fn connection_state(&self) -> ConnectionState {
        self.ws.state()
    }

    // ── Auth ─────────────────────────────────────────────────────────────────

    /// Authenticate with a Nostr private key (nsec bech32 or 64-char hex).
    pub async fn login_with_key(&self, nsec_or_hex: String) -> Result<UserIdentity, SproutError> {
        let km = KeyManager::from_nsec_or_hex(&nsec_or_hex)?;
        let identity = km.identity();
        *self.key_manager.write().await = Some(km);
        Ok(identity)
    }

    /// Authenticate with a pre-existing API token.
    pub async fn login_with_token(&self, api_token: String) -> Result<UserIdentity, SproutError> {
        TokenCache::save(&self.store, &api_token, None)?;
        *self.api_token.write().await = Some(api_token);
        // Without a key, we can't derive identity. Return a placeholder.
        let km = self.key_manager.read().await;
        match km.as_ref() {
            Some(km) => Ok(km.identity()),
            None => Err(SproutError::AuthRequired),
        }
    }

    /// The currently authenticated identity, if any.
    pub fn current_identity(&self) -> Option<UserIdentity> {
        // Use try_read to avoid blocking in a sync context.
        self.key_manager
            .try_read()
            .ok()
            .and_then(|km| km.as_ref().map(|k| k.identity()))
    }

    /// Export the private key as bech32-encoded nsec.
    pub fn export_nsec(&self) -> Result<String, SproutError> {
        self.key_manager
            .try_read()
            .ok()
            .and_then(|km| km.as_ref().map(|k| k.export_nsec()))
            .unwrap_or(Err(SproutError::AuthRequired))
    }

    // ── Channels ─────────────────────────────────────────────────────────────

    /// List channels accessible to the current user.
    pub async fn list_channels(&self, _filter: ChannelFilter) -> Result<Vec<Channel>, SproutError> {
        let token = self.require_token().await?;
        let channels = self.http.list_channels(&token).await?;
        // Cache all channels.
        for ch in &channels {
            let _ = self.store.upsert_channel(ch);
        }
        Ok(channels)
    }

    /// Get details for a single channel.
    pub async fn get_channel(&self, channel_id: String) -> Result<Channel, SproutError> {
        let token = self.require_token().await?;
        let ch = self.http.get_channel_detail(&token, &channel_id).await?;
        let _ = self.store.upsert_channel(&ch);
        Ok(ch)
    }

    /// Create a new channel.
    pub async fn create_channel(
        &self,
        params: CreateChannelParams,
    ) -> Result<Channel, SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;

        let visibility = params.visibility.as_ref().map(|v| match v {
            ChannelVisibility::Open => sprout_sdk::Visibility::Open,
            ChannelVisibility::Private => sprout_sdk::Visibility::Private,
        });
        let channel_type = params.channel_type.as_ref().map(|ct| match ct {
            ChannelType::Stream => sprout_sdk::ChannelKind::Stream,
            ChannelType::Forum => sprout_sdk::ChannelKind::Forum,
            ChannelType::Dm => sprout_sdk::ChannelKind::Dm,
        });

        let channel_id = uuid::Uuid::new_v4();
        let builder = sprout_sdk::build_create_channel(
            channel_id,
            &params.name,
            visibility,
            channel_type,
            params.about.as_deref(),
        )
        .map_err(sdk_err)?;

        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;

        // Fetch the created channel to return full details.
        let ch_id = channel_id.to_string();
        self.get_channel(ch_id).await
    }

    /// Join a channel.
    pub async fn join_channel(&self, channel_id: String) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let uuid = parse_uuid(&channel_id)?;

        let builder = sprout_sdk::build_join(uuid).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    /// Leave a channel.
    pub async fn leave_channel(&self, channel_id: String) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let uuid = parse_uuid(&channel_id)?;

        let builder = sprout_sdk::build_leave(uuid).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    /// List members of a channel.
    pub async fn list_members(
        &self,
        channel_id: String,
    ) -> Result<Vec<ChannelMember>, SproutError> {
        let token = self.require_token().await?;
        self.http.list_channel_members(&token, &channel_id).await
    }

    // ── Messages ─────────────────────────────────────────────────────────────

    /// List messages in a channel with optional cursor-based pagination.
    pub async fn list_messages(
        &self,
        channel_id: String,
        before: Option<i64>,
        limit: Option<u32>,
    ) -> Result<MessagePage, SproutError> {
        let token = self.require_token().await?;
        let cursor = before.map(|ts| ts.to_string());
        self.http
            .list_messages(&token, &channel_id, cursor.as_deref(), limit.unwrap_or(50))
            .await
    }

    /// Fetch a full thread by root event ID.
    pub async fn get_thread(
        &self,
        channel_id: String,
        root_event_id: String,
    ) -> Result<Vec<Message>, SproutError> {
        let token = self.require_token().await?;
        self.http
            .get_thread(&token, &channel_id, &root_event_id)
            .await
    }

    /// Send a message to a channel.
    pub async fn send_message(&self, params: SendMessageParams) -> Result<Message, SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let uuid = parse_uuid(&params.channel_id)?;

        let thread_ref = match (&params.thread_root_event_id, &params.reply_to_event_id) {
            (Some(root), Some(parent)) => Some(sprout_sdk::ThreadRef {
                root_event_id: parse_event_id(root)?,
                parent_event_id: parse_event_id(parent)?,
            }),
            (Some(root), None) => Some(sprout_sdk::ThreadRef {
                root_event_id: parse_event_id(root)?,
                parent_event_id: parse_event_id(root)?,
            }),
            _ => None,
        };

        let mention_refs: Vec<&str> = params.mentions.iter().map(|s| s.as_str()).collect();

        let builder = sprout_sdk::build_message(
            uuid,
            &params.content,
            thread_ref.as_ref(),
            &mention_refs,
            false,
            &[],
        )
        .map_err(sdk_err)?;

        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        let resp = self.http.submit_event(&token, &event).await?;

        // Build a Message from what we know.
        let event_id = resp
            .get("event_id")
            .and_then(|e| e.as_str())
            .unwrap_or(&event.id.to_hex())
            .to_string();

        Ok(Message {
            event_id,
            channel_id: params.channel_id,
            author_pubkey: keys.public_key().to_hex(),
            content: params.content,
            created_at: event.created_at.as_u64() as i64,
            kind: 9,
            reply_to: params.reply_to_event_id,
            thread_root: params.thread_root_event_id,
            reactions: Vec::new(),
            media: Vec::new(),
            reply_count: 0,
            author_profile: None,
        })
    }

    /// Edit a message.
    pub async fn edit_message(
        &self,
        channel_id: String,
        event_id: String,
        content: String,
    ) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let uuid = parse_uuid(&channel_id)?;
        let target = parse_event_id(&event_id)?;

        let builder = sprout_sdk::build_edit(uuid, target, &content).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    /// Delete a message.
    pub async fn delete_message(
        &self,
        channel_id: String,
        event_id: String,
    ) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let uuid = parse_uuid(&channel_id)?;
        let target = parse_event_id(&event_id)?;

        let builder = sprout_sdk::build_delete_message(uuid, target).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    // ── Reactions ────────────────────────────────────────────────────────────

    /// Add an emoji reaction to a message.
    pub async fn add_reaction(&self, event_id: String, emoji: String) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let target = parse_event_id(&event_id)?;

        let builder = sprout_sdk::build_reaction(target, &emoji).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    /// Remove a reaction.
    pub async fn remove_reaction(&self, reaction_event_id: String) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        let target = parse_event_id(&reaction_event_id)?;

        let builder = sprout_sdk::build_remove_reaction(target).map_err(sdk_err)?;
        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    // ── Direct Messages ──────────────────────────────────────────────────────

    /// List DM conversations.
    pub async fn list_dms(&self) -> Result<Vec<DmConversation>, SproutError> {
        let token = self.require_token().await?;
        self.http.list_dms(&token).await
    }

    /// Open or create a DM conversation with the given participants.
    pub async fn open_dm(&self, pubkeys: Vec<String>) -> Result<DmConversation, SproutError> {
        let token = self.require_token().await?;
        self.http.open_dm(&token, &pubkeys).await
    }

    // ── Users & Profiles ─────────────────────────────────────────────────────

    /// Get a user's profile by pubkey.
    pub async fn get_profile(&self, pubkey: String) -> Result<UserProfile, SproutError> {
        let token = self.require_token().await?;
        let profile = self.http.get_user_profile(&token, &pubkey).await?;
        let _ = self.store.upsert_profile(&profile);
        Ok(profile)
    }

    /// Batch-fetch multiple user profiles.
    pub async fn get_profiles_batch(
        &self,
        pubkeys: Vec<String>,
    ) -> Result<Vec<UserProfile>, SproutError> {
        let token = self.require_token().await?;
        let profiles = self.http.get_users_batch(&token, &pubkeys).await?;
        for p in &profiles {
            let _ = self.store.upsert_profile(p);
        }
        Ok(profiles)
    }

    /// Update the current user's profile.
    pub async fn update_profile(&self, params: UpdateProfileParams) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;

        let builder = sprout_sdk::build_profile(
            params.display_name.as_deref(),
            None,
            params.picture.as_deref(),
            params.about.as_deref(),
            None,
        )
        .map_err(sdk_err)?;

        let event = builder.sign_with_keys(&keys).map_err(sign_err)?;
        self.http.submit_event(&token, &event).await?;
        Ok(())
    }

    /// Search users by name or pubkey prefix.
    pub async fn search_users(&self, query: String) -> Result<Vec<UserProfile>, SproutError> {
        let token = self.require_token().await?;
        self.http.search_users(&token, &query).await
    }

    // ── Feed & Search ────────────────────────────────────────────────────────

    /// Get the personalized home feed.
    pub async fn get_feed(&self) -> Result<HomeFeed, SproutError> {
        let token = self.require_token().await?;
        self.http.get_feed(&token).await
    }

    /// Full-text search across messages.
    pub async fn search(&self, query: String) -> Result<Vec<SearchResult>, SproutError> {
        let token = self.require_token().await?;
        self.http.search(&token, &query).await
    }

    // ── Presence ─────────────────────────────────────────────────────────────

    /// Set the current user's presence status.
    pub async fn set_presence(&self, status: PresenceStatus) -> Result<(), SproutError> {
        let token = self.require_token().await?;
        self.http.set_presence(&token, &status).await
    }

    // ── Media ────────────────────────────────────────────────────────────────

    /// Upload media bytes and return the Blossom blob descriptor.
    pub async fn upload_media(
        &self,
        file_bytes: Vec<u8>,
        _mime_type: String,
    ) -> Result<MediaUploadResult, SproutError> {
        let token = self.require_token().await?;
        let keys = self.require_keys().await?;
        self.http
            .upload_media_bytes(&token, &keys, file_bytes)
            .await
    }

    // ── Real-time subscriptions ──────────────────────────────────────────────

    /// Subscribe to real-time events for a channel. Returns a subscription ID.
    pub async fn subscribe_channel(&self, channel_id: String) -> Result<String, SproutError> {
        let sub_id = ws::subscription::generate_sub_id("ch");
        let filter = ws::subscription::channel_message_filter(&channel_id);
        self.ws.subscribe(sub_id.clone(), vec![filter]).await;
        Ok(sub_id)
    }

    /// Unsubscribe from a channel subscription.
    pub async fn unsubscribe(&self, subscription_id: String) -> Result<(), SproutError> {
        self.ws.unsubscribe(&subscription_id).await;
        Ok(())
    }

    // ── Event listener ───────────────────────────────────────────────────────

    /// Register a listener for push events from the relay.
    pub fn set_event_listener(&self, listener: Box<dyn SproutEventListener>) {
        // Use try_write to avoid blocking; this is called from native main thread.
        if let Ok(mut guard) = self.ws.listener.try_write() {
            *guard = Some(listener);
        }
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

impl SproutClient {
    /// Get the current API token, returning an error if not available.
    async fn require_token(&self) -> Result<String, SproutError> {
        self.api_token
            .read()
            .await
            .clone()
            .ok_or(SproutError::AuthRequired)
    }

    /// Get the current keys, returning an error if not available.
    async fn require_keys(&self) -> Result<Keys, SproutError> {
        self.key_manager
            .read()
            .await
            .as_ref()
            .map(|km| km.keys().clone())
            .ok_or(SproutError::AuthRequired)
    }

    /// Ensure we have an API token, auto-minting via NIP-98 if needed.
    async fn ensure_token(&self, keys: &Keys) -> Result<(), SproutError> {
        if self.api_token.read().await.is_some() {
            return Ok(());
        }

        // Auto-mint a token via NIP-98.
        let body = token::mint_token_body("sprout-mobile");
        let resp = self
            .http
            .post_with_nip98("/api/tokens", keys, &body)
            .await?;
        let (token_str, expires_at) = token::parse_mint_response(&resp)?;

        TokenCache::save(&self.store, &token_str, expires_at)?;
        *self.api_token.write().await = Some(token_str);

        Ok(())
    }
}

/// Callback interface for push events from the relay to native UI.
#[uniffi::export(callback_interface)]
pub trait SproutEventListener: Send + Sync {
    /// A new message arrived in a subscribed channel.
    fn on_message(&self, message: Message);
    /// A message was edited.
    fn on_message_edited(&self, channel_id: String, event_id: String, new_content: String);
    /// A message was deleted.
    fn on_message_deleted(&self, channel_id: String, event_id: String);
    /// A reaction was added to a message.
    fn on_reaction(
        &self,
        channel_id: String,
        event_id: String,
        emoji: String,
        author_pubkey: String,
    );
    /// A user started or stopped typing in a channel.
    fn on_typing(&self, channel_id: String, pubkey: String, is_typing: bool);
    /// A user's presence changed.
    fn on_presence_changed(&self, pubkey: String, status: PresenceStatus);
    /// WebSocket connection state changed.
    fn on_connection_state_changed(&self, state: ConnectionState);
    /// A channel's metadata was updated.
    fn on_channel_updated(&self, channel: Channel);
    /// The current user was added to a channel.
    fn on_added_to_channel(&self, channel_id: String);
    /// The current user was removed from a channel.
    fn on_removed_from_channel(&self, channel_id: String);
}

// ── Parsing helpers ──────────────────────────────────────────────────────────

fn parse_uuid(s: &str) -> Result<uuid::Uuid, SproutError> {
    uuid::Uuid::parse_str(s).map_err(|e| SproutError::ValidationError {
        message: format!("invalid UUID: {e}"),
    })
}

fn parse_event_id(s: &str) -> Result<nostr::EventId, SproutError> {
    nostr::EventId::from_hex(s).map_err(|e| SproutError::ValidationError {
        message: format!("invalid event ID: {e}"),
    })
}

fn sdk_err(e: sprout_sdk::SdkError) -> SproutError {
    SproutError::ValidationError {
        message: e.to_string(),
    }
}

fn sign_err(e: nostr::event::builder::Error) -> SproutError {
    SproutError::InvalidKey {
        message: format!("event signing failed: {e}"),
    }
}
