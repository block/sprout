//! Audio room: peer registry and frame fan-out.
//!
//! ```text
//! Client A → WS binary frame → Room::broadcast_frame → Client B, C, ...
//!                                                        (1-byte peer_index prefix)
//! ```
//!
//! Frames are opaque Opus bytes — the relay never decodes audio.
//! `try_send` is used throughout: real-time audio tolerates drops, never queues.

use bytes::Bytes;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// A connected audio peer.
pub struct AudioPeer {
    /// Nostr pubkey hex.
    pub pubkey: String,
    /// Audio frames (binary Opus with peer_index prefix). Drops on full — real-time.
    pub audio_tx: mpsc::Sender<Bytes>,
    /// Control messages (joined/left/close JSON). Separate queue so control
    /// is never starved by audio backpressure.
    pub ctrl_tx: mpsc::Sender<PeerCtrl>,
    /// Stable 0-254 index assigned at join; prefixed onto relayed frames.
    pub peer_index: u8,
}

/// Control message for a single peer (separate from audio frames).
pub enum PeerCtrl {
    /// JSON control message (joined/left/speakers).
    Json(String),
    /// Graceful shutdown signal.
    Close,
}

/// Audio channel capacity per peer: 8 frames = 160ms at 20ms/frame.
const AUDIO_CHANNEL_CAPACITY: usize = 8;
/// Control channel capacity per peer: 32 slots — must never drop joined/left
/// messages, which are state-bearing (they maintain the client's peer_index →
/// pubkey map). Sized generously: even 30 simultaneous join/leave events fit.
const CTRL_CHANNEL_CAPACITY: usize = 32;

/// Defense-in-depth cap on peers per room. A room with N peers generates
/// N×(N−1) frame copies per 20ms tick — 25 peers = 600 copies/tick, which
/// is reasonable. The 255 index space is the hard limit; this is the soft one.
const MAX_PEERS_PER_ROOM: usize = 25;

/// A single audio room for one channel.
pub struct Room {
    /// Channel UUID this room belongs to.
    pub channel_id: Uuid,
    /// Connected peers keyed by peer UUID.
    pub peers: DashMap<Uuid, AudioPeer>,
    /// Recycled peer indices (returned on leave) + next fresh index.
    index_pool: std::sync::Mutex<IndexPool>,
}

/// Simple index allocator: hands out 0–254, recycles on remove.
struct IndexPool {
    next_fresh: u8,
    free: Vec<u8>,
}

impl IndexPool {
    fn new() -> Self {
        Self {
            next_fresh: 0,
            free: Vec::new(),
        }
    }

    fn alloc(&mut self) -> Option<u8> {
        if let Some(idx) = self.free.pop() {
            return Some(idx);
        }
        if self.next_fresh == 255 {
            return None;
        }
        let idx = self.next_fresh;
        self.next_fresh += 1;
        Some(idx)
    }

    fn release(&mut self, idx: u8) {
        self.free.push(idx);
    }
}

impl Room {
    /// Create an empty room for the given channel.
    pub fn new(channel_id: Uuid) -> Self {
        Self {
            channel_id,
            peers: DashMap::new(),
            index_pool: std::sync::Mutex::new(IndexPool::new()),
        }
    }

    /// Add a peer. Returns `(peer_id, peer_index, audio_rx, ctrl_rx)`, or
    /// `None` if the room has hit the peer cap or exhausted the 0–254 index space.
    pub fn add_peer(
        &self,
        pubkey: String,
    ) -> Option<(Uuid, u8, mpsc::Receiver<Bytes>, mpsc::Receiver<PeerCtrl>)> {
        if self.peers.len() >= MAX_PEERS_PER_ROOM {
            return None;
        }
        let peer_id = Uuid::new_v4();
        let peer_index = self.index_pool.lock().ok()?.alloc()?;
        let (audio_tx, audio_rx) = mpsc::channel(AUDIO_CHANNEL_CAPACITY);
        let (ctrl_tx, ctrl_rx) = mpsc::channel(CTRL_CHANNEL_CAPACITY);
        self.peers.insert(
            peer_id,
            AudioPeer {
                pubkey,
                audio_tx,
                ctrl_tx,
                peer_index,
            },
        );
        Some((peer_id, peer_index, audio_rx, ctrl_rx))
    }

    /// Remove a peer and recycle its index.
    pub fn remove_peer(&self, peer_id: Uuid) {
        if let Some((_, peer)) = self.peers.remove(&peer_id) {
            if let Ok(mut pool) = self.index_pool.lock() {
                pool.release(peer.peer_index);
            }
        }
    }

    /// Fan-out a binary frame to all peers except the sender.
    /// Prepends the sender's `peer_index` as a 1-byte prefix.
    /// Drops on full buffer — real-time audio never queues.
    pub fn broadcast_frame(&self, sender_id: Uuid, frame: Bytes) {
        let sender_index = match self.peers.get(&sender_id) {
            Some(p) => p.peer_index,
            None => return,
        };

        // Prepend peer_index as 1-byte header.
        let mut prefixed = bytes::BytesMut::with_capacity(1 + frame.len());
        prefixed.extend_from_slice(&[sender_index]);
        prefixed.extend_from_slice(&frame);
        let prefixed = prefixed.freeze();

        for entry in self.peers.iter() {
            if *entry.key() == sender_id {
                continue;
            }
            let _ = entry.audio_tx.try_send(prefixed.clone());
        }
    }

    /// Send a JSON control message to all peers via the control channel.
    /// Separate from audio so control is never starved by audio backpressure.
    /// Control messages (joined/left) are state-bearing — the client's
    /// peer_index→pubkey map depends on receiving every one. The channel is
    /// sized generously (32 slots) so drops should never happen in practice;
    /// if they do, we log a warning so the issue is visible.
    pub fn broadcast_control(&self, json: String) {
        for entry in self.peers.iter() {
            if entry
                .ctrl_tx
                .try_send(PeerCtrl::Json(json.clone()))
                .is_err()
            {
                tracing::warn!(
                    peer_id = %entry.key(),
                    "control channel full — dropped state-bearing message (peer map may desync)"
                );
            }
        }
    }

    /// All `(pubkey, peer_index)` pairs in the room.
    pub fn peer_pubkeys(&self) -> Vec<(String, u8)> {
        self.peers
            .iter()
            .map(|e| (e.pubkey.clone(), e.peer_index))
            .collect()
    }

    /// True if no peers remain in the room.
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

/// Global registry of active audio rooms.
pub struct AudioRoomManager {
    rooms: DashMap<Uuid, Arc<Room>>,
}

impl AudioRoomManager {
    /// Create an empty room manager.
    pub fn new() -> Self {
        Self {
            rooms: DashMap::new(),
        }
    }

    /// Get an existing room or create a new one.
    pub fn get_or_create(&self, channel_id: Uuid) -> Arc<Room> {
        self.rooms
            .entry(channel_id)
            .or_insert_with(|| Arc::new(Room::new(channel_id)))
            .clone()
    }

    /// Remove the room if it has no peers.
    pub fn cleanup_if_empty(&self, channel_id: Uuid) {
        self.rooms.remove_if(&channel_id, |_, room| room.is_empty());
    }
}

impl Default for AudioRoomManager {
    fn default() -> Self {
        Self::new()
    }
}
