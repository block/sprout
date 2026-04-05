// ABOUTME: FFI-safe types that cross the UniFFI boundary to Swift/Kotlin.
// ABOUTME: Plain records with String/i64/u32/bool/Vec — no nostr::* types leak through.

/// Configuration for creating a [`SproutClient`](crate::SproutClient).
#[derive(uniffi::Record)]
pub struct ClientConfig {
    /// WebSocket URL of the Sprout relay (e.g. `wss://relay.example.com`).
    pub relay_url: String,
    /// Platform-specific app data directory for the SQLite cache.
    pub db_path: String,
    /// Optional existing key material (nsec bech32 or 64-char hex).
    pub nsec_or_hex: Option<String>,
    /// Optional pre-existing API token (`sprout_*`).
    pub api_token: Option<String>,
    /// Log level: "debug", "info", "warn", "error". Defaults to "info".
    pub log_level: Option<String>,
}

/// WebSocket connection state.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ConnectionState {
    /// Not connected to any relay.
    Disconnected,
    /// TCP/TLS handshake in progress.
    Connecting,
    /// NIP-42 authentication in progress.
    Authenticating,
    /// Fully connected and authenticated.
    Connected,
    /// Reconnecting after a connection drop.
    Reconnecting {
        /// Number of reconnect attempts so far.
        attempt: u32,
    },
}

/// The authenticated user's identity.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UserIdentity {
    /// 64-character hex public key.
    pub pubkey: String,
    /// Bech32-encoded npub.
    pub npub: String,
    /// Display name from the user's profile, if known.
    pub display_name: Option<String>,
}

/// A Sprout channel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Channel {
    /// Channel UUID.
    pub id: String,
    /// Channel name.
    pub name: String,
    /// Channel description/about.
    pub about: Option<String>,
    /// Current topic.
    pub topic: Option<String>,
    /// Channel type.
    pub channel_type: ChannelType,
    /// Visibility setting.
    pub visibility: ChannelVisibility,
    /// Number of members.
    pub member_count: u32,
    /// Timestamp of the last message (Unix seconds), if any.
    pub last_message_at: Option<i64>,
    /// Whether the current user is a member.
    pub is_member: bool,
}

/// Channel type enum.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ChannelType {
    /// Linear message stream (Slack-like).
    Stream,
    /// Threaded forum-style.
    Forum,
    /// Direct message conversation.
    Dm,
}

/// Channel visibility enum.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum ChannelVisibility {
    /// Searchable; anyone can join.
    Open,
    /// Hidden; requires invite.
    Private,
}

/// A message in a channel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Message {
    /// 64-character hex event ID.
    pub event_id: String,
    /// Channel UUID.
    pub channel_id: String,
    /// Author's 64-char hex pubkey.
    pub author_pubkey: String,
    /// Message content.
    pub content: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Nostr event kind.
    pub kind: u32,
    /// Parent event ID if this is a reply.
    pub reply_to: Option<String>,
    /// Thread root event ID.
    pub thread_root: Option<String>,
    /// Reaction summaries for this message.
    pub reactions: Vec<ReactionSummary>,
    /// Media attachments.
    pub media: Vec<MediaAttachment>,
    /// Number of direct replies.
    pub reply_count: u32,
    /// Author's profile, if available.
    pub author_profile: Option<UserProfile>,
}

/// A page of messages with pagination info.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MessagePage {
    /// The messages in this page.
    pub messages: Vec<Message>,
    /// Whether more messages exist before this page.
    pub has_more: bool,
}

/// Summary of reactions on a message, grouped by emoji.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReactionSummary {
    /// The emoji string.
    pub emoji: String,
    /// Total count.
    pub count: u32,
    /// Whether the current user reacted with this emoji.
    pub reacted_by_me: bool,
    /// Event ID of the current user's reaction (for removal).
    pub my_reaction_event_id: Option<String>,
}

/// A media attachment embedded in a message.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MediaAttachment {
    /// Public media URL.
    pub url: String,
    /// MIME type (e.g. "image/jpeg").
    pub mime_type: String,
    /// File size in bytes.
    pub size_bytes: Option<u64>,
    /// Pixel dimensions ("WxH").
    pub dimensions: Option<String>,
    /// Blurhash string for progressive loading.
    pub blurhash: Option<String>,
    /// Thumbnail URL.
    pub thumbnail_url: Option<String>,
}

/// A user profile.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UserProfile {
    /// 64-character hex pubkey.
    pub pubkey: String,
    /// Display name.
    pub display_name: Option<String>,
    /// Avatar/picture URL.
    pub picture: Option<String>,
    /// About/bio text.
    pub about: Option<String>,
    /// NIP-05 handle (e.g. "alice@example.com").
    pub nip05: Option<String>,
}

/// Presence status.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum PresenceStatus {
    /// Actively online.
    Online,
    /// Idle / away.
    Away,
    /// Offline.
    Offline,
}

/// Member role in a channel.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum MemberRole {
    /// Full control.
    Owner,
    /// Manage members and settings.
    Admin,
    /// Standard participant.
    Member,
    /// Read-only.
    Guest,
    /// Automated agent.
    Bot,
}

/// A member of a channel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelMember {
    /// Member's 64-char hex pubkey.
    pub pubkey: String,
    /// Member's role.
    pub role: MemberRole,
    /// Member's profile, if available.
    pub profile: Option<UserProfile>,
}

/// Parameters for sending a message.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SendMessageParams {
    /// Target channel UUID.
    pub channel_id: String,
    /// Message content.
    pub content: String,
    /// Event ID of the message being replied to (direct parent).
    pub reply_to_event_id: Option<String>,
    /// Event ID of the thread root.
    pub thread_root_event_id: Option<String>,
    /// Pubkey hex strings of mentioned users.
    pub mentions: Vec<String>,
    /// Media attachment URLs from prior `upload_media` calls.
    pub media_attachments: Vec<String>,
}

/// Parameters for creating a channel.
#[derive(Debug, Clone, uniffi::Record)]
pub struct CreateChannelParams {
    /// Channel name.
    pub name: String,
    /// Visibility setting.
    pub visibility: Option<ChannelVisibility>,
    /// Channel type.
    pub channel_type: Option<ChannelType>,
    /// Description/about.
    pub about: Option<String>,
}

/// Filter for listing channels.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ChannelFilter {
    /// Filter by visibility.
    pub visibility: Option<ChannelVisibility>,
    /// Only show channels the user is a member of.
    pub member_only: bool,
}

/// Parameters for updating a profile.
#[derive(Debug, Clone, uniffi::Record)]
pub struct UpdateProfileParams {
    /// New display name.
    pub display_name: Option<String>,
    /// New avatar URL.
    pub picture: Option<String>,
    /// New about/bio text.
    pub about: Option<String>,
}

/// A DM conversation.
#[derive(Debug, Clone, uniffi::Record)]
pub struct DmConversation {
    /// Underlying channel UUID.
    pub channel_id: String,
    /// Participant profiles.
    pub participants: Vec<UserProfile>,
    /// Most recent message, if any.
    pub last_message: Option<Message>,
}

/// A search result.
#[derive(Debug, Clone, uniffi::Record)]
pub struct SearchResult {
    /// Event ID of the matching message.
    pub event_id: String,
    /// Channel UUID.
    pub channel_id: String,
    /// Channel name for display.
    pub channel_name: String,
    /// Message content.
    pub content: String,
    /// Author's pubkey.
    pub author_pubkey: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
}

/// A feed item from the home feed.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FeedItem {
    /// Event ID.
    pub event_id: String,
    /// Channel UUID.
    pub channel_id: String,
    /// Channel name for display.
    pub channel_name: String,
    /// Message content.
    pub content: String,
    /// Author's pubkey.
    pub author_pubkey: String,
    /// Unix timestamp (seconds).
    pub created_at: i64,
    /// Event kind.
    pub kind: u32,
    /// Feed category.
    pub category: FeedCategory,
}

/// Feed item category.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum FeedCategory {
    /// The user was mentioned.
    Mention,
    /// An action is required (approval, reminder).
    NeedsAction,
    /// General channel activity.
    Activity,
    /// Agent/bot activity.
    AgentActivity,
}

/// The personalized home feed.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HomeFeed {
    /// Items in the feed.
    pub items: Vec<FeedItem>,
    /// Total count across all categories.
    pub total: u32,
}

/// Result of a media upload.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MediaUploadResult {
    /// Public URL of the uploaded media.
    pub url: String,
    /// SHA-256 hash (64-char hex).
    pub sha256: String,
    /// File size in bytes.
    pub size: u64,
    /// MIME type.
    pub mime_type: String,
    /// Pixel dimensions ("WxH"), if applicable.
    pub dimensions: Option<String>,
    /// Blurhash string, if generated.
    pub blurhash: Option<String>,
    /// Thumbnail URL, if generated.
    pub thumbnail_url: Option<String>,
}
