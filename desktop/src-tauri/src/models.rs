use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

use sprout_core::PresenceStatus;

#[derive(Serialize)]
pub struct IdentityInfo {
    pub pubkey: String,
    pub display_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct ProfileInfo {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub about: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UserProfileSummaryInfo {
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UsersBatchResponse {
    pub profiles: HashMap<String, UserProfileSummaryInfo>,
    pub missing: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct UserSearchResultInfo {
    pub pubkey: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub nip05_handle: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SearchUsersResponse {
    pub users: Vec<UserSearchResultInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct SetPresenceResponse {
    pub status: PresenceStatus,
    pub ttl_seconds: u64,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub purpose: Option<String>,
    pub member_count: i64,
    pub last_message_at: Option<String>,
    pub archived_at: Option<String>,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub participant_pubkeys: Vec<String>,
    #[serde(default = "default_true")]
    pub is_member: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelDetailInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub visibility: String,
    #[serde(deserialize_with = "deserialize_null_string_as_empty")]
    pub description: String,
    pub topic: Option<String>,
    pub topic_set_by: Option<String>,
    pub topic_set_at: Option<String>,
    pub purpose: Option<String>,
    pub purpose_set_by: Option<String>,
    pub purpose_set_at: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub member_count: i64,
    pub topic_required: bool,
    pub max_members: Option<i32>,
    pub nip29_group_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMemberInfo {
    pub pubkey: String,
    pub role: String,
    pub joined_at: String,
    pub display_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMembersResponse {
    pub members: Vec<ChannelMemberInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct AddMembersResponse {
    pub added: Vec<String>,
    pub errors: Vec<serde_json::Value>,
}

#[derive(Serialize)]
pub struct CreateChannelBody<'a> {
    pub name: &'a str,
    pub channel_type: &'a str,
    pub visibility: &'a str,
    pub description: Option<&'a str>,
}

#[derive(Serialize)]
pub struct OpenDmBody<'a> {
    pub pubkeys: &'a [String],
}

#[derive(Deserialize)]
pub struct OpenDmResponse {
    pub channel_id: String,
}

#[derive(Serialize)]
pub struct UpdateChannelBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
}

#[derive(Serialize)]
pub struct SetTopicBody<'a> {
    pub topic: &'a str,
}

#[derive(Serialize)]
pub struct SetPurposeBody<'a> {
    pub purpose: &'a str,
}

#[derive(Serialize)]
pub struct AddMembersBody<'a> {
    pub pubkeys: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'a str>,
}

#[derive(Serialize)]
pub struct UpdateProfileBody<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub about: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nip05_handle: Option<&'a str>,
}

#[derive(Serialize)]
pub struct SetPresenceBody {
    pub status: PresenceStatus,
}

#[derive(Serialize)]
pub struct GetFeedQuery<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<&'a str>,
}

#[derive(Serialize)]
pub struct SearchQueryParams<'a> {
    pub q: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Serialize)]
pub struct SendChannelMessageBody<'a> {
    pub content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_event_id: Option<&'a str>,
    pub broadcast_to_channel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_tags: Option<Vec<Vec<String>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mention_pubkeys: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<u32>,
}

#[derive(Serialize)]
pub struct AddReactionBody<'a> {
    pub emoji: &'a str,
}

#[derive(Serialize)]
pub struct MintTokenBody<'a> {
    pub name: &'a str,
    pub scopes: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_ids: Option<&'a [String]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_days: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct MintTokenResponse {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub channel_ids: Vec<String>,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ListTokensResponse {
    pub tokens: Vec<TokenInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct RevokeAllTokensResponse {
    pub revoked_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedItemInfo {
    pub id: String,
    pub kind: u32,
    pub pubkey: String,
    pub content: String,
    pub created_at: u64,
    pub channel_id: Option<String>,
    pub channel_name: String,
    pub tags: Vec<Vec<String>>,
    pub category: String,
}

#[derive(Serialize, Deserialize)]
pub struct FeedSections {
    pub mentions: Vec<FeedItemInfo>,
    pub needs_action: Vec<FeedItemInfo>,
    pub activity: Vec<FeedItemInfo>,
    pub agent_activity: Vec<FeedItemInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct FeedMeta {
    pub since: i64,
    pub total: u64,
    pub generated_at: i64,
}

#[derive(Serialize, Deserialize)]
pub struct FeedResponse {
    pub feed: FeedSections,
    pub meta: FeedMeta,
}

#[derive(Serialize, Deserialize)]
pub struct SearchHitInfo {
    pub event_id: String,
    pub content: String,
    pub kind: u32,
    pub pubkey: String,
    pub channel_id: String,
    pub channel_name: String,
    pub created_at: u64,
    pub score: f64,
}

#[derive(Serialize, Deserialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHitInfo>,
    pub found: u64,
}

#[derive(Serialize, Deserialize)]
pub struct SendChannelMessageResponse {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub root_event_id: Option<String>,
    pub depth: u32,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct GetUsersBatchBody<'a> {
    pub pubkeys: &'a [String],
}

#[derive(Serialize, Deserialize)]
pub struct ThreadSummary {
    pub reply_count: u32,
    pub descendant_count: u32,
    pub last_reply_at: Option<i64>,
    pub participants: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ForumMessageInfo {
    pub event_id: String,
    pub pubkey: String,
    pub content: String,
    pub kind: u32,
    pub created_at: i64,
    pub channel_id: String,
    pub tags: Vec<Vec<String>>,
    #[serde(default)]
    pub thread_summary: Option<ThreadSummary>,
    #[serde(default)]
    pub reactions: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct ForumPostsResponse {
    pub messages: Vec<ForumMessageInfo>,
    pub next_cursor: Option<i64>,
}

#[derive(Serialize, Deserialize)]
pub struct ForumThreadReplyInfo {
    pub event_id: String,
    pub pubkey: String,
    pub content: String,
    pub kind: u32,
    pub created_at: i64,
    pub channel_id: String,
    pub tags: Vec<Vec<String>>,
    pub parent_event_id: Option<String>,
    pub root_event_id: Option<String>,
    pub depth: u32,
    pub broadcast: bool,
    #[serde(default)]
    pub reactions: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
pub struct ForumThreadResponse {
    pub root: ForumMessageInfo,
    pub replies: Vec<ForumThreadReplyInfo>,
    pub total_replies: u32,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct GetForumPostsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<i64>,
    pub with_threads: bool,
}

#[derive(Serialize)]
pub struct GetForumThreadQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

fn deserialize_null_string_as_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn default_true() -> bool {
    true
}
