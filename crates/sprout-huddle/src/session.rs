use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of media track published by a participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    /// An audio track.
    Audio,
    /// A video track.
    Video,
    /// A screen-share track.
    ScreenShare,
}

impl std::fmt::Display for TrackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackKind::Audio => write!(f, "audio"),
            TrackKind::Video => write!(f, "video"),
            TrackKind::ScreenShare => write!(f, "screenshare"),
        }
    }
}

/// Metadata about a media track published by a participant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackInfo {
    /// The kind of track (audio, video, or screen-share).
    pub kind: TrackKind,
    /// When the track was published.
    pub published_at: DateTime<Utc>,
}

/// A participant in a huddle session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuddleParticipant {
    /// The participant's Nostr public key (hex).
    pub pubkey: String,
    /// The participant's display name.
    pub display_name: String,
    /// When the participant joined the session.
    pub joined_at: DateTime<Utc>,
    /// When the participant left, or `None` if still active.
    pub left_at: Option<DateTime<Utc>>,
    /// Tracks published by this participant.
    pub tracks: Vec<TrackInfo>,
}

impl HuddleParticipant {
    /// Create a new participant with the given pubkey and display name.
    pub fn new(pubkey: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            pubkey: pubkey.into(),
            display_name: display_name.into(),
            joined_at: Utc::now(),
            left_at: None,
            tracks: Vec::new(),
        }
    }

    /// Mark the participant as having left at the current time.
    pub fn leave(&mut self) {
        self.left_at = Some(Utc::now());
    }

    /// Record a published track of the given kind.
    pub fn add_track(&mut self, kind: TrackKind) {
        self.tracks.push(TrackInfo {
            kind,
            published_at: Utc::now(),
        });
    }
}

/// An in-progress or completed huddle session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuddleSession {
    /// Unique session identifier.
    pub id: Uuid,
    /// The Sprout channel this session belongs to.
    pub channel_id: Uuid,
    /// The LiveKit room name for this session.
    pub room_name: String,
    /// When the session started.
    pub started_at: DateTime<Utc>,
    /// When the session ended, or `None` if still active.
    pub ended_at: Option<DateTime<Utc>>,
    /// All participants who have joined (including those who have left).
    pub participants: Vec<HuddleParticipant>,
    /// Whether recording is enabled for this session.
    pub recording_enabled: bool,
}

impl HuddleSession {
    /// Create a new active session for `channel_id` in `room_name`.
    pub fn new(channel_id: Uuid, room_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            room_name: room_name.into(),
            started_at: Utc::now(),
            ended_at: None,
            participants: Vec::new(),
            recording_enabled: false,
        }
    }

    /// Add a participant to the session.
    pub fn join(&mut self, participant: HuddleParticipant) {
        self.participants.push(participant);
    }

    /// Returns true if the participant was found.
    pub fn leave(&mut self, pubkey: &str) -> bool {
        if let Some(p) = self
            .participants
            .iter_mut()
            .find(|p| p.pubkey == pubkey && p.left_at.is_none())
        {
            p.leave();
            true
        } else {
            false
        }
    }

    /// Mark the session as ended at the current time.
    pub fn end(&mut self) {
        self.ended_at = Some(Utc::now());
    }

    /// Returns `true` if the session has not yet ended.
    pub fn is_active(&self) -> bool {
        self.ended_at.is_none()
    }

    /// Iterate over participants who have not yet left.
    pub fn active_participants(&self) -> impl Iterator<Item = &HuddleParticipant> {
        self.participants.iter().filter(|p| p.left_at.is_none())
    }
}
