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
    /// Owner pubkey hex for NIP-AA virtual members. `None` for direct members.
    /// Retained for the audio session lifetime per NIP-AA §6 for owner-scoped
    /// enumeration, termination, and quota aggregation.
    pub owner_pubkey: Option<String>,
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

/// Peer index allocator + room lifecycle gate.
///
/// The `ended` flag and peer admission are synchronized under the same mutex.
/// `add_peer` holds this lock across the ended check, index allocation, and
/// peer insert — so `mark_ended` (which also acquires this lock) is mutually
/// exclusive with peer admission. This closes the race between the last
/// peer's cleanup path and a concurrent joiner.
struct AdmissionGuard {
    next_fresh: u8,
    free: Vec<u8>,
    ended: bool,
}

impl AdmissionGuard {
    fn new() -> Self {
        Self {
            next_fresh: 0,
            free: Vec::new(),
            ended: false,
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

/// A single audio room for one channel.
pub struct Room {
    /// Channel UUID this room belongs to.
    pub channel_id: Uuid,
    /// Connected peers keyed by peer UUID.
    pub peers: DashMap<Uuid, AudioPeer>,
    /// Admission gate: index allocator + ended flag under one lock.
    guard: std::sync::Mutex<AdmissionGuard>,
}

impl Room {
    /// Create an empty room for the given channel.
    pub fn new(channel_id: Uuid) -> Self {
        Self {
            channel_id,
            peers: DashMap::new(),
            guard: std::sync::Mutex::new(AdmissionGuard::new()),
        }
    }

    /// Mark the room as ended. After this returns, no new `add_peer` can
    /// succeed — they'll see `ended == true` under the same lock.
    /// Returns `true` if the room is empty (safe to archive + emit 48103).
    /// Returns `false` if a peer snuck in before we acquired the lock.
    pub fn mark_ended(&self) -> bool {
        if let Ok(mut g) = self.guard.lock() {
            g.ended = true;
            self.peers.is_empty()
        } else {
            false
        }
    }

    /// Undo `mark_ended` — used when archive needs to be rolled back.
    pub fn clear_ended(&self) {
        if let Ok(mut g) = self.guard.lock() {
            g.ended = false;
        }
    }

    /// Add a peer. Returns `(peer_id, peer_index, audio_rx, ctrl_rx)`, or
    /// `None` if the room has ended, hit the peer cap, or exhausted the index space.
    ///
    /// The ended check, index allocation, and peer insert all happen under
    /// the admission guard lock — mutually exclusive with `mark_ended`.
    pub fn add_peer(
        &self,
        pubkey: String,
        owner_pubkey: Option<String>,
    ) -> Option<(Uuid, u8, mpsc::Receiver<Bytes>, mpsc::Receiver<PeerCtrl>)> {
        if self.peers.len() >= MAX_PEERS_PER_ROOM {
            return None;
        }
        // Hold the guard across ended check + index alloc + peer insert.
        // This makes add_peer mutually exclusive with mark_ended — the lock
        // is the single synchronization point that closes the race.
        let mut g = self.guard.lock().ok()?;
        if g.ended {
            return None;
        }
        let peer_index = g.alloc()?;
        let peer_id = Uuid::new_v4();
        let (audio_tx, audio_rx) = mpsc::channel(AUDIO_CHANNEL_CAPACITY);
        let (ctrl_tx, ctrl_rx) = mpsc::channel(CTRL_CHANNEL_CAPACITY);
        self.peers.insert(
            peer_id,
            AudioPeer {
                pubkey,
                owner_pubkey,
                audio_tx,
                ctrl_tx,
                peer_index,
            },
        );
        drop(g); // Release lock after insert.
        Some((peer_id, peer_index, audio_rx, ctrl_rx))
    }

    /// Remove a peer and recycle its index.
    pub fn remove_peer(&self, peer_id: Uuid) {
        if let Some((_, peer)) = self.peers.remove(&peer_id) {
            if let Ok(mut g) = self.guard.lock() {
                g.release(peer.peer_index);
            }
        }
    }

    /// Remove a peer AND atomically check if the room should end.
    /// If the room is now empty, sets `ended = true` under the same lock
    /// acquisition that recycles the index — no window for a concurrent
    /// `add_peer` to sneak in between removal and the ended flag.
    /// Returns `(peer_index, should_auto_end)`.
    pub fn remove_peer_and_check_ended(&self, peer_id: Uuid) -> Option<(u8, bool)> {
        let (_, peer) = self.peers.remove(&peer_id)?;
        let peer_index = peer.peer_index;
        let should_end = if let Ok(mut g) = self.guard.lock() {
            g.release(peer_index);
            // Only the first task to see empty + !ended wins the auto-end.
            // This prevents duplicate archive/48103 when two peers disconnect
            // simultaneously and both see is_empty() == true.
            if !g.ended && self.peers.is_empty() {
                g.ended = true;
                true
            } else {
                false
            }
        } else {
            false
        };
        Some((peer_index, should_end))
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

    /// Remove the room if it has no peers. Returns `true` if the room was removed.
    pub fn cleanup_if_empty(&self, channel_id: Uuid) -> bool {
        self.rooms
            .remove_if(&channel_id, |_, room| room.is_empty())
            .is_some()
    }
}

impl Default for AudioRoomManager {
    fn default() -> Self {
        Self::new()
    }
}
