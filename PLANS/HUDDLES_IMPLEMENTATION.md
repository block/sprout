---
title: "Sprout Huddles Implementation Plan"
tags: [huddles, voice, stt, tts, livekit, agents]
status: draft
created: 2026-04-11
---

# Sprout Huddles — Implementation Plan (v4)

## One-Sentence Summary

Humans talk via LiveKit WebRTC; each desktop GUI locally transcribes speech and posts it to an ephemeral Nostr text channel where agents read, respond, and get read aloud — agents never touch audio.

---

## Mental Model

```
┌──────────────────────────────────────────────────────────────────┐
│                   Human Desktop GUI (Tauri)                       │
│                                                                   │
│  ┌───────────────────────────────────────────────────────────┐   │
│  │ WebView                                                    │   │
│  │  LiveKit JS SDK ──── WebRTC audio to/from other humans     │   │
│  │  AudioWorklet ─────── taps mic PCM ──→ invoke(raw binary)  │   │
│  │  Huddle UI ────────── join / leave / mute / participants   │   │
│  └───────────────────────────────────────────────────────────┘   │
│                    ↕ Tauri invoke (raw binary)                    │
│  ┌───────────────────────────────────────────────────────────┐   │
│  │ Rust Backend                                               │   │
│  │                                                            │   │
│  │  STT: PCM → earshot VAD → sherpa-onnx Moonshine → text    │   │
│  │       └─→ POST kind:9 to ephemeral channel                │   │
│  │                                                            │   │
│  │  TTS: agent kind:9 → Supertonic (ort) → rodio → spkr       │   │
│  │       └─→ barge-in: VAD speech → cancel + stop + gate STT │   │
│  │                                                            │   │
│  │  HuddleManager: lifecycle, tokens, channel, agent invites │   │
│  └───────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
          │ WebSocket + REST                    │ LiveKit SFU
          ▼                                     ▼
┌────────────────────────┐        ┌────────────────────────┐
│     Sprout Relay       │        │   LiveKit Server       │
│                        │        │   (audio routing)      │
│  Ephemeral channel     │        └────────────────────────┘
│  (private, TTL, text)  │
│                        │
│  Token endpoint        │
│  POST /api/huddles/tkn │
│                        │
│  Lifecycle events      │
│  (advisory, 48100-105) │
└────────────────────────┘
          │
          │ membership notification → auto-subscribe
          ▼
┌────────────────────────────────────────────────┐
│              Agent (sprout-acp)                  │
│                                                  │
│  Sees ONLY a text channel. No audio. No WebRTC. │
│  Receives ALL human speech (p-tagged, interrupt) │
│  Posts text → read aloud on every human's GUI.   │
│  Also added to parent channel for full context.  │
└────────────────────────────────────────────────┘
```

**Component sentences:**

| Component | One sentence |
|-----------|-------------|
| LiveKit JS SDK | Handles human-to-human audio via WebRTC in the Tauri webview — zero binary overhead. |
| AudioWorklet | Taps the local mic's PCM stream and sends it to Rust via `invoke()` with raw binary body. |
| earshot VAD | Pure-Rust voice activity detector (8 KB, 1270× realtime) that gates the STT pipeline. |
| sherpa-onnx | Moonshine STT (34 ms), bundled ONNX runtime. |
| Supertonic | TTS engine (4 ONNX sessions via `ort`), 10 voices, ~0.5–1.0 s TTFA, 44.1 kHz output. |
| rodio | Audio playback for TTS output, with instant `sink.stop()` for barge-in. |
| Ephemeral channel | A normal private Sprout text channel with a TTL — the huddle's written record. |
| HuddleManager | Tauri-side state machine that creates the channel, mints tokens, and wires the pipelines. |
| sprout-acp | Existing agent harness in interrupt mode — every human utterance cancels and re-prompts. |

---

## Key Design Decisions

### 1. Agents see only text

The core insight. An agent that can chat in Sprout can participate in a huddle with zero changes. The desktop GUI handles all voice ↔ text translation locally. This means:

- Any LLM-backed agent works — no special audio capabilities needed.
- The ephemeral channel is a permanent written record of every huddle.
- Agents can be added or removed mid-huddle and it just works.

### 2. LiveKit JS SDK in the webview, not the Rust SDK

The LiveKit Rust SDK statically links Google's `libwebrtc` — a **253 MB** download per platform. The JS SDK uses the browser's native WebRTC (WebKit on macOS) at zero binary cost. The Tauri webview has full WebRTC support. LiveKit handles the hard multi-party SFU routing.

*Source: `.scratch/research-livekit-rust.md` — seismicgear/annex migrated away from LiveKit Rust SDK for the same reason.*

### 3. Supertonic for TTS, sherpa-onnx for STT

**TTS**: Supertonic (`supertone-inc/supertonic`) is an MIT-licensed TTS engine using 4 ONNX sessions (duration predictor, text encoder, vector estimator, vocoder) via the `ort` crate. It produces 44.1 kHz audio at ~167× real-time. Chosen over sherpa-onnx's Kokoro TTS for:
- MIT license (sherpa-onnx is Apache 2.0 but Kokoro model licensing was unclear)
- Superior voice quality with 10 built-in voices (F1-F5, M1-M5)
- Flow-matching architecture with configurable denoising steps

**STT**: sherpa-onnx with Moonshine tiny (unchanged from original plan).

**Dual ONNX runtime tradeoff**: Supertonic uses the `ort` crate while sherpa-onnx bundles its own ONNX runtime. This means two ONNX runtimes in the binary. We accept this tradeoff because:
- No symbol conflicts observed (ort and sherpa-onnx's bundled runtime coexist)
- Memory overhead is acceptable (~50 MB additional)
- Supertonic's voice quality and MIT licensing justify the cost
- The alternative (writing a sherpa-onnx Kokoro wrapper) would be more code with worse quality

*This supersedes the original plan to use sherpa-onnx for both STT and TTS.*

### 4. AudioWorklet fan-out with Tauri `invoke(raw)` IPC

A single mic stream in the webview feeds both LiveKit (for WebRTC) and the Rust backend (for STT). The AudioWorklet taps PCM before LiveKit encodes it, and sends frames to Rust via Tauri 2's binary `Channel` API — **not** `invoke()` with JSON serialization.

Why not dual mic capture (cpal + getUserMedia)? macOS CoreAudio supports it, but the AudioWorklet approach is strictly better: single mic permission dialog, single CoreAudio session, no VoiceProcessingIO interference risk, portable across platforms.

Why not `invoke()` with `Vec<f32>`? JSON serialization of 480 f32 samples = ~3.8 KB per 10 ms frame = ~380 KB/s overhead. Tauri's binary `Channel` API transfers raw bytes at ~60 KB/s — 6× less overhead.

*Source: `.scratch/research-dual-mic.md`*

### 5. earshot for VAD (not Silero)

Pure Rust, no ONNX dependency, 8 KB RAM, 1270× realtime. Silero VAD is higher quality but requires ONNX runtime — unnecessary when earshot is more than adequate for gating STT and detecting barge-in.

### 6. Agents are always listening — every message interrupts

Every transcribed human utterance in the ephemeral channel **p-tags all agents** in the huddle. Agents are spawned with `SPROUT_ACP_MULTIPLE_EVENT_HANDLING=interrupt` and `SPROUT_ACP_DEDUP=queue` — each new human message cancels the agent's in-flight turn and re-prompts with the new context.

This is the simplest possible design. No name detection, no routing, no "who was that for?" ambiguity. Agents hear everything and decide for themselves whether to respond. The voice-mode system prompt tells them: "Only respond if the message is relevant to you or directed at you. If it's not for you, respond with an empty message or stay brief."

In practice, well-prompted agents self-select effectively. A deployment agent won't respond to "what's for lunch?" and a research agent won't respond to "roll back the deploy." The interrupt mode ensures agents are always responsive to the latest human speech — no stale context.

**One agent is the default.** The UI defaults to one agent per huddle. Users can add more — they're asking for it, and they'll need to make the prompting work. No enforced limit.

**Why `dedup=queue` + `interrupt`:** The `queue` mode holds new events while the agent is mid-turn. The `interrupt` mode cancels the in-flight turn and re-prompts with the cancelled context merged in as `[Previous request — interrupted before completion]`. The agent always sees what it was working on plus the new message. `drop` mode would lose the interrupting message entirely — unacceptable for voice.

### 7. STT gating during TTS (echo cancellation)

When TTS is playing agent speech through the speakers, the **STT transcription** pipeline is gated — audio is not sent to sherpa-onnx. However, the **VAD continues running** so it can detect human speech for barge-in. The distinction:

- VAD (earshot): always active — detects speech onset for barge-in even during TTS
- STT (sherpa-onnx): gated while TTS is playing + 200 ms cooldown after TTS stops

This prevents the feedback loop (TTS → mic → STT → posts TTS output as human speech) while still allowing instant barge-in detection.

For MVP, this simple gating is sufficient (headset/earbuds assumed). Full AEC via `webrtc-audio-processing` is a future enhancement for laptop speakers. The UI should prominently recommend headphones when joining a huddle.

---

## What Exists Today vs. What Needs to Be Built

### Exists Today (verified in codebase)

| Component | Location | Status |
|-----------|----------|--------|
| Ephemeral channels (TTL) | `schema.sql` (`ttl_seconds`, `ttl_deadline`), `sprout-db/channel.rs` (bump/reaper) | ✅ Working |
| Desktop `create_channel` with TTL | `desktop/src-tauri/src/events.rs` (`build_create_channel` accepts `ttl_seconds`) | ✅ Working |
| TTL reaper task | `sprout-relay/src/main.rs` (`reap_expired_ephemeral_channels`) | ✅ Working |
| Huddle kind constants | `sprout-core/src/kind.rs` (48100–48106 in `ALL_KINDS`) | ✅ Defined |
| LiveKit token generation | `sprout-huddle/src/token.rs` (`generate_token`) | ✅ Working |
| LiveKit webhook parsing | `sprout-huddle/src/webhook.rs` (`parse_webhook`) | ✅ Working |
| In-memory session tracking | `sprout-huddle/src/session.rs` (`HuddleSession`) | ✅ Working |
| ACP interrupt mode | `sprout-acp/src/config.rs` (`MultipleEventHandling::Interrupt`) | ✅ Working |
| ACP dynamic channel subscription | `sprout-acp/src/main.rs` (membership notification → auto-subscribe) | ✅ Working |
| ACP `respond_to=anyone` mode | `sprout-acp/src/config.rs` (`RespondTo::Anyone`) | ✅ Working |
| Ephemeral channel UI (badge, TTL display) | `desktop/src/features/channels/lib/ephemeralChannel.ts` | ✅ Working |
| Channel add policy enforcement | `sprout-relay/src/handlers/side_effects.rs` (`channel_add_policy`) | ✅ Working |

### Needs to Be Built

| Component | Layer | Effort |
|-----------|-------|--------|
| **Relay: wire `HuddleService` into `AppState`** | Server | Small — add config, instantiate service |
| **Relay: `POST /api/huddles/{channel_id}/token` endpoint** | Server | Small — new route, auth check, call existing `generate_token` |
| **Relay: verify huddle kinds are stored/fanned-out** | Server | Verify only — kinds are in `ALL_KINDS`, should work |
| **Desktop Rust: `huddle/` module** | Desktop | Large — HuddleManager, STT, TTS, model management |
| **Desktop Rust: Tauri commands** | Desktop | Medium — `start_huddle`, `join_huddle`, `leave_huddle`, etc. |
| **Desktop WebView: LiveKit JS integration** | Desktop | Medium — room connection, AudioWorklet, invoke(raw) IPC |
| **Desktop WebView: Huddle UI** | Desktop | Medium — HuddleBar, participant list, mute, TTS toggle |
| **Desktop: `NSMicrophoneUsageDescription`** | Desktop | Trivial — add to `Info.plist` |
| **SDK: huddle event builders** | SDK | Small — `build_huddle_started`, `build_huddle_ended`, etc. |
| **Model download infrastructure** | Desktop | Medium — download manager, checksums, progress UI |

---

## Component Details

### A. Ephemeral Huddle Channel

**What:** A private Sprout stream channel with a TTL, created when a huddle starts.

**Creation** (by the initiating human's GUI):
- Kind: 9007 (NIP-29 create group)
- Name: `huddle-{parent_channel_name}-{short_id}`
- Type: `stream`, Visibility: `private`
- TTL: 3600 s (1 hour of inactivity — each message resets the clock)
- Tags: `["h", "<uuid>"]`, `["name", "..."]`, `["visibility", "private"]`, `["channel_type", "stream"]`, `["ttl", "3600"]`

**Lifecycle:**
1. Human clicks "Start Huddle" → GUI creates ephemeral channel
2. GUI adds human participants as members (kind:9000)
3. GUI adds selected agents as members (kind:9000, respecting `channel_add_policy`)
4. GUI emits KIND_HUDDLE_STARTED to **parent** channel (advisory)
5. GUI fetches LiveKit token from relay → connects to LiveKit room
6. Every message bumps `ttl_deadline` (existing relay behavior)
7. Human clicks "End Huddle" → GUI emits KIND_HUDDLE_ENDED to parent channel
8. Channel naturally expires via TTL reaper after 1 hour of silence

**Rollback:** If LiveKit token fetch fails after channel creation, the GUI archives the orphaned ephemeral channel (kind:9002) and shows an error.

### B. Huddle Lifecycle Events

Posted to the **parent** channel. These are **advisory UI hints**, not source of truth. The actual huddle state is determined by channel membership + LiveKit room state.

| Kind | Constant | Content |
|------|----------|---------|
| 48100 | `KIND_HUDDLE_STARTED` | `{ "ephemeral_channel_id": "<uuid>", "livekit_room": "sprout-<uuid>" }` |
| 48101 | `KIND_HUDDLE_PARTICIPANT_JOINED` | `{ "ephemeral_channel_id": "<uuid>" }` |
| 48102 | `KIND_HUDDLE_PARTICIPANT_LEFT` | `{ "ephemeral_channel_id": "<uuid>" }` |
| 48103 | `KIND_HUDDLE_ENDED` | `{ "ephemeral_channel_id": "<uuid>" }` |
| 48106 | `KIND_HUDDLE_GUIDELINES` | Voice-mode guidelines text for agents |

Tags include `["h", "<parent_channel_uuid>"]`. The parent channel's UI shows "Huddle in progress" with participant avatars based on these events.

### C. Security & Authorization

| Action | Who can do it | Enforcement |
|--------|---------------|-------------|
| Start a huddle | Any member of the parent channel | GUI-side (creates ephemeral channel as the user) |
| Join a huddle | Any member of the parent channel | Token endpoint requires channel membership |
| Add an agent | Any owner/admin of the ephemeral channel | Relay enforces role check + `channel_add_policy` on kind:9000 |
| End a huddle | The huddle creator | GUI-side (emits KIND_HUDDLE_ENDED) |
| Spoof lifecycle events | Mitigated | Events are advisory; actual state = membership + LiveKit |

**Token endpoint authorization:**
- `POST /api/huddles/{channel_id}/token` requires authentication (existing auth middleware)
- `{channel_id}` is the **ephemeral huddle channel** UUID
- Verifies the requesting user is a member of the **ephemeral channel** (not the parent)
- This ensures only invited participants can join the LiveKit room
- Returns a LiveKit JWT scoped to the room `sprout-{channel_id}`

**Agent enrollment:**
- Agents are added to huddles using the **same in-channel "+" button** used for adding agents to regular channels. Familiar UX, no new UI patterns.
- When an agent is added to a huddle, the desktop:
  1. Adds the agent to the **ephemeral huddle channel** (kind:9000) — this is where STT/TTS happens
  2. Adds the agent to the **parent channel** (kind:9000) — so the agent has full context of the channel the huddle is about
  3. Spawns an ACP process for the agent with interrupt mode + expanded toolsets
- Each agent add goes through the relay's existing validation in `side_effects.rs`:
  - Private channel: requires the actor to be owner or admin
  - `channel_add_policy` on the target agent: respects `owner_only` / `nobody` / `anyone`
- If an agent's policy rejects the add, the GUI shows a warning and skips that agent
- Agents can be added mid-huddle — the ACP harness auto-subscribes via membership notification

### D. Audio Routing

Single mic stream, two consumers:

```
Mic → getUserMedia → MediaStream
                        ├──→ LiveKit JS SDK (WebRTC to SFU)
                        └──→ AudioWorklet (tap PCM)
                                └──→ Tauri invoke(raw binary)
                                        └──→ Rust STT pipeline
```

**AudioWorklet implementation:**
```javascript
// In the webview:
class SttTapProcessor extends AudioWorkletProcessor {
  process(inputs) {
    const samples = inputs[0][0]; // Float32Array, 128 samples at 48kHz
    if (samples) {
      this.port.postMessage(samples.buffer, [samples.buffer]);
    }
    return true;
  }
}
```

The webview receives `postMessage` from the AudioWorklet, converts to `Uint8Array`, and sends to Rust via `invoke()` with a raw binary body (Tauri 2's `Channel` is Rust→JS only; JS→Rust binary streaming uses `invoke` with `InvokeBody::Raw`):

```typescript
// In AudioWorklet message handler — batch ~100ms of frames to reduce IPC calls:
const buffer = accumulateFrames(float32Frames, 4800); // 100ms at 48kHz
await invoke("push_audio_pcm", buffer, { headers: { "Content-Type": "application/octet-stream" } });
```

On the Rust side, the Tauri command receives raw bytes via `tauri::ipc::Request`:

```rust
#[tauri::command]
fn push_audio_pcm(request: tauri::ipc::Request) -> Result<(), String> {
    let body = request.body().as_bytes().ok_or("expected raw body")?;
    // body is &[u8] — reinterpret as &[f32] (4800 samples = 19.2 KB per 100ms)
    stt_tx.send(body.to_vec()).map_err(|e| e.to_string())
}
```

At 100 ms batching: ~10 IPC calls/sec, ~19 KB/call. Negligible overhead.

**⚠️ Phase 0 spike required:** Validate this AudioWorklet → `invoke(raw)` → Rust pipeline before committing to the architecture. The spike should confirm: (a) raw body IPC works from AudioWorklet context, (b) latency is <20 ms per batch, (c) no audio glitches under load.

**TTS playback** goes through `rodio` directly to speakers. It does **not** go through LiveKit — agent speech is local-only. Each human hears the agent independently.

### E. STT Pipeline

Runs in the Tauri Rust backend on a dedicated `spawn_blocking` thread.

```
PCM f32 48 kHz (from Tauri IPC, raw body)
  → rubato resample to 16 kHz mono
  → earshot VAD (256-sample frames)
      ├─ speech start → begin accumulating in ring buffer
      └─ speech end → flush buffer to sherpa-onnx
  → sherpa-onnx OfflineRecognizer (Moonshine tiny, 26 MB)
  → transcribed text
  → GUI signs kind:9 event with p-tags for ALL agents in huddle
  → POST to relay (normal Nostr event — no server-side magic)
```

**STT gating:** When TTS is playing, the STT transcription is muted — audio is not sent to sherpa-onnx. A shared `AtomicBool` (`tts_active`) is checked after VAD detects speech end. While true, the accumulated audio buffer is discarded instead of sent to STT. The VAD itself continues running so it can detect barge-in. Set `tts_active` to false 200 ms after TTS playback stops.

**Model management:**
- Moonshine tiny: ~26 MB, stored in `~/.sprout/models/moonshine-tiny/`
- Downloaded in background on app launch (not on huddle start)
- Checksum verified after download
- If models aren't ready when huddle starts: huddle works as voice-only (no transcription), with a banner "Downloading voice models…"

### F. TTS Pipeline

Runs in the Tauri Rust backend on a dedicated thread.

```
Agent kind:9 message arrives on ephemeral channel subscription
  → filter: pubkey NOT in human_participants set
  → text preprocessing (strip markdown, numbers → words)
  → Supertonic TTS (4 ONNX sessions, int8 quantized)
  → rodio Player → speakers
```

**Barge-in:**
```rust
let tts_active = Arc::new(AtomicBool::new(false));

// TTS thread:
tts_active.store(true, Ordering::Release);
// sherpa-onnx generates audio, rodio plays it
// On completion or barge-in:
tts_active.store(false, Ordering::Release);

// VAD thread, on speech detected:
if tts_active.load(Ordering::Acquire) {
    // Barge-in: stop TTS, mute speakers
    tts_cancel.store(true, Ordering::Release);
    sink.stop();
    // STT re-enables after 200ms cooldown
}
```

**Text preprocessing pipeline**: Agent text passes through two preprocessing stages:
1. `preprocessing.rs::preprocess_for_tts()` — strips markdown, code blocks, URLs, expands numbers to words
2. `supertonic.rs::preprocess_text()` — strips emoji, normalizes Unicode (NFKD), fixes punctuation spacing, adds language tags

Stage 1 is Sprout-specific (handles agent output patterns). Stage 2 is Supertonic-specific (prepares text for the ONNX model). Both are necessary — stage 1 handles content the TTS model shouldn't see, stage 2 handles Unicode normalization the TTS model requires.

**Stage 1 details** (`preprocessing.rs`):
- Strip markdown formatting (`**bold**` → `bold`)
- Code blocks → "code block omitted"
- Numbers → words: `"11:30"` → `"eleven thirty"`, `"42"` → `"forty two"`
- URLs → "link omitted"
- Emoji → skip or use name ("thumbs up")

**Latency budget:**

| Stage | Typical | Notes |
|-------|---------|-------|
| STT (VAD + Moonshine) | ~1–2 s | From speech end to text |
| Agent LLM response | ~1–3 s | Depends on model/provider |
| TTS (Supertonic TTFA) | ~0.5–1.0 s | Time to first audio on M2 |
| **Total** | **~3.5–6.5 s** | From speech end to agent starts talking |

**Latency perception management:**
- Agent emits a typing indicator (kind:20002) when processing → UI shows "Agent is thinking…"
- A subtle audio chime plays 200 ms before TTS starts → primes the listener
- Short responses are prioritized — the voice-mode guidelines encourage brevity

**Multiple agent responses:** Queue them. First response plays; subsequent responses wait. If a new human speech event arrives, cancel all queued TTS.

### G. Agent Experience

**Voice-mode guidelines:**

The voice-mode guidelines tell agents how to behave in a huddle:

```
You are in a live voice huddle. Your text is read aloud via TTS.
You will be interrupted by new messages whenever a human speaks — this is normal.

Rules:
- Only respond if the message is relevant to you or directed at you.
  If it's not for you, respond with just "." or stay silent.
- Keep responses under 2 sentences. This is a conversation, not an essay.
- Spell out numbers: "eleven thirty" not "11:30".
- No markdown, code blocks, or bullet lists — they sound terrible as speech.
- To share code or data, say "I'll post that in the main channel" and use it.
- You have access to Sprout tools — you can join channels, search messages,
  and take actions. Use them proactively when asked.
```

**Why system prompt injection, not a visible message?** Agents routinely ignore visible system messages (kind:40099). The ACP system prompt is prepended to every agent turn and cannot be ignored — it's part of the LLM's context window.

**ACP configuration strategy:**

Agents added to huddles are **spawned by the desktop** with huddle-specific ACP configuration:

```
SPROUT_ACP_MULTIPLE_EVENT_HANDLING=interrupt
SPROUT_ACP_DEDUP=queue
SPROUT_ACP_SYSTEM_PROMPT="<voice-mode guidelines above>"
SPROUT_ACP_RESPOND_TO=anyone
```

The desktop already spawns ACP processes per agent (see `managed_agents/runtime.rs`). For huddle agents, the desktop sets these env vars at spawn time. The agent's existing subscription rules apply — the ACP harness auto-subscribes to the ephemeral channel via membership notification, and every p-tagged message triggers an interrupt of the in-flight turn.

**Expanded MCP toolsets:** Huddle agents are spawned with additional MCP toolsets beyond the default. The `SPROUT_TOOLSETS` env var controls which tools are available. Huddle agents get tools that let them:
- Add themselves to other channels (`join_channel`)
- Search messages across channels
- Read canvases and channel history
- Take proactive actions (create channels, post to other channels)

This makes huddle agents more capable — they can act on voice requests like "join the incident channel and check the latest messages."

**Guidelines delivery:** Voice-mode guidelines delivered as kind:48106 (`KIND_HUDDLE_GUIDELINES`) to the ephemeral channel. This dedicated kind allows the TTS pipeline to filter guidelines without fragile content-prefix matching. Agents see the guidelines via EOSE replay when they subscribe to the channel. Also passed as `SPROUT_ACP_SYSTEM_PROMPT` env var at ACP spawn time — belt and suspenders.

**Post-MVP:** Add `system_prompt: Option<String>` to `SubscriptionRule` (~50 LOC in `filter.rs` + `queue.rs` + `pool.rs`) so agents can have different system prompts per channel without needing a separate ACP process.

### H. Relay Changes

Three additions, all small:

**1. `HuddleService` in `AppState`:**
```rust
// In sprout-relay/src/state.rs:
pub huddle_service: Option<HuddleService>,

// In sprout-relay/src/main.rs, during startup:
let huddle_service = match (
    std::env::var("LIVEKIT_URL"),
    std::env::var("LIVEKIT_API_KEY"),
    std::env::var("LIVEKIT_API_SECRET"),
) {
    (Ok(url), Ok(key), Ok(secret)) => Some(HuddleService::new(HuddleConfig {
        livekit_url: url, livekit_api_key: key, livekit_api_secret: secret,
    })),
    _ => { info!("LiveKit not configured — huddles disabled"); None }
};
```

**2. Token endpoint:**
- Route: `POST /api/huddles/{channel_id}/token`
- Auth: existing middleware (Bearer JWT or NIP-42)
- Validation: verify requester is a member of the **ephemeral huddle channel** (not parent)
- Response: `{ "token": "<jwt>", "url": "<livekit_url>", "room": "sprout-<channel_id>" }`
- If `huddle_service` is `None`: return 501 Not Implemented

**3. Add `sprout-huddle` to relay `Cargo.toml`:**
```toml
sprout-huddle = { workspace = true }
```

**Verify:** Huddle kinds (48100–48106) are in `ALL_KINDS` and the relay stores/fans-out any registered kind. No code change expected — verify with a test.

### I. Desktop Changes

**New module:** `desktop/src-tauri/src/huddle/`

```
huddle/
  mod.rs            — HuddleState, helpers, Tauri commands, transcription task
  agents.rs         — Agent enrollment and voice-mode guidelines
  stt.rs            — earshot VAD + sherpa-onnx Moonshine STT pipeline
  tts.rs            — Supertonic TTS + rodio playback, barge-in
  supertonic.rs     — Supertonic ONNX engine wrapper (4 sessions)
  models.rs         — Model download manager (Moonshine + Supertonic)
  preprocessing.rs  — Text preprocessing for TTS output
```

**Tauri commands:**

| Command | Returns | Description |
|---------|---------|-------------|
| `start_huddle(parent_channel_id, agent_pubkeys)` | `{ ephemeral_channel_id, livekit_token, livekit_url }` | Create channel, add members, mint token |
| `join_huddle(ephemeral_channel_id)` | `{ livekit_token, livekit_url }` | Mint token for existing huddle |
| `leave_huddle()` | `()` | Stop pipelines, emit left event |
| `end_huddle()` | `()` | Emit ended event, archive channel, stop everything |
| `start_stt_pipeline()` | `()` | Initialize STT pipeline; PCM arrives via `push_audio_pcm` |
| `push_audio_pcm` (raw body) | `()` | Receive PCM bytes from AudioWorklet; uses `tauri::ipc::Request` not normal command signature |
| `set_tts_enabled(enabled: bool)` | `()` | Mute/unmute agent TTS |
| `get_huddle_state()` | `HuddleState` | Current huddle status |
| `download_voice_models()` | progress events | Download STT + TTS models |
| `get_model_status()` | `ModelStatus` | Are models downloaded and ready? |

**New `Cargo.toml` dependencies:**

| Crate | Version | Purpose | Notes |
|-------|---------|---------|-------|
| `sherpa-onnx` | 1.12 | STT (Moonshine) | Bundles ONNX runtime |
| `ort` | 2.0 | TTS (Supertonic ONNX sessions) | Separate ONNX runtime |
| `ndarray` | latest | TTS tensor operations | Used by Supertonic wrapper |
| `earshot` | 1.0 | Pure Rust VAD | 8 KB, no deps |
| `rodio` | 0.22 | Audio playback | Wraps cpal |
| `rubato` | 2.0 | Audio resampling | 48 kHz ↔ 16 kHz ↔ 24 kHz |

**Removed from plan:** `kokoros` (replaced by Supertonic TTS), sherpa-onnx TTS (replaced by Supertonic).
**Added:** `ort`, `ndarray`, `rand_distr`, `unicode-normalization` (Supertonic dependencies).

**New npm dependency:** `livekit-client`

**Info.plist addition:** `NSMicrophoneUsageDescription` — "Sprout needs microphone access for voice huddles."

**Runtime model downloads** (~120 MB total, background on app launch):

| Model | Size | Purpose |
|-------|------|---------|
| Moonshine tiny | ~26 MB | STT |
| Supertonic ONNX models | ~90 MB | TTS (4 sessions) |

### J. UI Visibility

**The ephemeral text channel does not appear in the sidebar.** It's plumbing, not a destination. Humans almost never need to look at it.

**What shows in the UI:**
- **Parent channel header:** "Huddle in progress" banner with participant avatars, join/leave button
- **HuddleBar** (floating or docked): mute, TTS toggle, participant list, end huddle
- **Small text bubble icon** on the HuddleBar: opens the ephemeral text channel if the user wants to peek at the raw transcript. Discoverable but not promoted.
- **"+" button** on the HuddleBar: adds agents to the huddle (same UX as adding agents to channels). Adds the agent to both the ephemeral channel and the parent channel.

### K. Message Format

**Human speech** (posted by each human's GUI after STT):

```json
{
  "kind": 9,
  "content": "Hey, can someone check the deployment status?",
  "tags": [
    ["h", "<ephemeral_channel_uuid>"],
    ["p", "<agent1_pubkey>"],
    ["p", "<agent2_pubkey>"],
    ["p", "<agent3_pubkey>"]
  ]
}
```

Signed by the speaking human's key. **All agents** in the huddle are p-tagged on every message. Agents are in interrupt mode — each new message cancels their in-flight turn and re-prompts. Agents self-select whether to respond based on their persona and the message content.

**Agent response:**

```json
{
  "kind": 9,
  "content": "The deployment is at eighty five percent. Two pods are still rolling.",
  "tags": [
    ["h", "<ephemeral_channel_uuid>"]
  ]
}
```

Signed by the agent's key. Human GUIs filter: if pubkey ∉ human_participants → run TTS.

---

## Edge Cases

| Scenario | Handling |
|----------|----------|
| Multiple agents respond simultaneously | Queue TTS. First plays, others wait. New human speech cancels queue. |
| Human speaks while TTS is playing | Barge-in: cancel TTS, stop playback, gate STT for 200 ms. |
| Agent posts code/markdown | TTS preprocessor strips it. Code blocks → "code block omitted". Agent told to use parent channel for code. |
| Huddle with no agents | Works fine — voice call with auto-transcription. Free meeting notes. |
| Agent added mid-huddle | ACP auto-subscribes via membership notification. Sees history via EOSE. Also added to parent channel. |
| Network disconnect | LiveKit handles WebRTC reconnect. Relay handles WS reconnect. Channel persists. |
| Multiple humans say the same thing | Each GUI posts independently. Different pubkeys = clear attribution. |
| STT hallucination on silence | earshot VAD prevents feeding silence to STT. |
| Models not downloaded | Huddle starts as voice-only. Banner: "Downloading voice models…". STT/TTS enable when ready. |
| Ephemeral channel expires during huddle | Won't happen — every message bumps TTL deadline. |
| Agent sends very long response | TTS plays sentence-by-sentence. Human can barge-in at any point. Interrupt mode cancels agent's turn on next speech. |
| LiveKit token fetch fails after channel creation | GUI archives orphaned ephemeral channel, shows error. |
| Agent's `channel_add_policy` rejects add | GUI shows warning, skips that agent. Other agents still added. |
| TTS → mic → STT feedback loop | STT gated while TTS active + 200 ms cooldown. |
| Agent ignores voice-friendly guidelines | Interrupt mode ensures agent is always re-prompted with latest speech. Verbose agents get interrupted naturally. |
| All agents respond to every message | Expected behavior. Agents self-select relevance. TTS queues responses. Interrupt mode keeps it conversational. |
| Agent wants to take action (join channel, search) | Expanded MCP toolsets give huddle agents proactive capabilities. |
| Overlapping human speech (cocktail party) | Each GUI transcribes own mic only. Agent sees sequential messages. |

---

## Implementation Phases

### Phase 0 — Proof-of-Concept Spikes

**Goal:** Validate high-risk technical assumptions before committing to the architecture.

- [ ] **Spike A: AudioWorklet → Rust binary IPC** — Build a minimal Tauri app that captures mic via AudioWorklet, sends PCM to Rust via `invoke()` with raw body, and logs received samples. Validate: latency <20 ms, no audio glitches, works on macOS.
- [ ] **Spike B: sherpa-onnx + Supertonic in Tauri** — Add `sherpa-onnx` and `ort`/Supertonic to a minimal Tauri Cargo.toml. Verify: compiles on macOS (arm64 + x64), links cleanly, no ONNX runtime symbol conflicts, Moonshine STT works, Supertonic TTS produces audio.
- [ ] **Spike C: LiveKit JS SDK in Tauri webview** — Connect to a LiveKit room from a Tauri webview. Verify: mic permission works, audio publishes/subscribes, AudioWorklet can tap the stream.

**Deliverable:** Three green spikes → proceed to Phase 1. Any red spike → redesign that component.

### Phase 1 — Voice Call Foundation

**Goal:** Humans can voice chat. Ephemeral text channel is created. No STT/TTS.

- [ ] Relay: add `sprout-huddle` to `Cargo.toml`, wire `HuddleService` into `AppState`
- [ ] Relay: implement `POST /api/huddles/{channel_id}/token` endpoint
- [ ] Relay: verify huddle kinds (48100–48106) are stored and fanned out
- [ ] Desktop: add `NSMicrophoneUsageDescription` to `Info.plist`
- [ ] Desktop Rust: `HuddleManager` — create ephemeral channel, add members, emit lifecycle events
- [ ] Desktop WebView: LiveKit JS SDK integration, room connection
- [ ] Desktop WebView: `HuddleBar` UI — join/leave, participant avatars, mute, "+" for agents
- [ ] Desktop: ephemeral channel hidden from sidebar (no UI affordance)
- [ ] SDK: `build_huddle_started`, `build_huddle_ended` builders

**Deliverable:** Click "Start Huddle" → LiveKit room opens → humans talk → ephemeral channel exists (empty).

### Phase 2 — Speech-to-Text

**Goal:** Human speech is transcribed and posted to the ephemeral channel.

- [ ] Desktop Rust: `models.rs` — background download manager for Moonshine model
- [ ] Desktop Rust: `stt.rs` — earshot VAD + sherpa-onnx Moonshine pipeline
- [ ] Desktop WebView: AudioWorklet mic tap → Tauri invoke(raw binary) IPC
- [ ] Desktop Rust: post transcribed text as kind:9 to ephemeral channel
- [ ] Desktop: progressive enhancement — huddle works voice-only while models download

**Deliverable:** Speak in huddle → text appears in ephemeral channel → agents can see it.

### Phase 3 — Agent Integration

**Goal:** Agents participate in huddles via text.

- [ ] Desktop: explicit agent selection UI when starting a huddle
- [ ] Desktop: add selected agents to ephemeral channel (respecting `channel_add_policy`)
- [ ] Desktop: reuse in-channel "+" button UX for adding agents to huddles
- [ ] Desktop: when agent added to huddle, also add to parent channel (kind:9000)
- [ ] Desktop: spawn ACP process with interrupt mode + expanded MCP toolsets
- [ ] Desktop STT: p-tag all huddle agents on every transcribed message (client-side, no server magic)
- [ ] Desktop: post voice-mode guidelines (kind:48106 `KIND_HUDDLE_GUIDELINES`) to ephemeral channel on huddle start
- [ ] Verify: ACP harness auto-subscribes, interrupt mode cancels in-flight turns on new speech
- [ ] Verify: agents can use expanded toolsets (join channels, search, etc.)

**Deliverable:** Agents hear all human speech, respond when relevant, get interrupted by new speech, can take actions.

### Phase 4 — Text-to-Speech

**Goal:** Agent responses are read aloud. Full end-to-end huddle.

- [ ] Desktop Rust: `models.rs` — add Supertonic model download
- [ ] Desktop Rust: `tts.rs` — Supertonic + rodio playback pipeline
- [ ] Desktop Rust: `preprocessing.rs` — strip markdown, numbers → words
- [ ] Desktop Rust: barge-in handling (VAD → cancel TTS → gate STT)
- [ ] Desktop Rust: STT gating during TTS (echo prevention)
- [ ] Desktop WebView: TTS toggle in huddle UI
- [ ] Desktop: audio chime before agent speech (latency perception)

**Deliverable:** Agent responds → text is read aloud → human can interrupt → full conversation loop.

### Phase 5 — Polish

- [ ] Different TTS voices per agent (map agent pubkey → Supertonic voice, F1-F5/M1-M5)
- [ ] ACP: per-channel system prompt in `SubscriptionRule` (~50 LOC) — replaces visible system message
- [ ] Per-channel interrupt mode in ACP subscription rules
- [ ] Transcript UI: visual distinction for spoken vs. typed messages
- [ ] "Save transcript" button (convert ephemeral → permanent before TTL expiry)
- [ ] Speaking indicators in HuddleBar (LiveKit `participant.isSpeaking`)
- [ ] Sub-sentence TTS chunking for faster barge-in
- [ ] Full AEC via `webrtc-audio-processing` for laptop speakers

---

## What We Are NOT Building

- **No server-side STT/TTS** — everything runs locally on the desktop
- **No video** — audio only (video is a separate, larger effort)
- **No screen sharing** — future work
- **No mobile support** — desktop only
- **No custom wake words** — agents hear everything and self-select relevance
- **No recording to file** — the ephemeral channel IS the transcript
- **No auto-add all agents** — explicit selection by huddle creator

---

## Sources

| Source | Key finding |
|--------|-------------|
| `.scratch/research-whisper-rs.md` | whisper-rs not streaming, archived; use sherpa-onnx/Moonshine |
| `.scratch/research-kokoro-tts.md` | Kokoros Rust crate (superseded — Supertonic chosen instead) |
| `.scratch/research-livekit-rust.md` | Rust SDK = 253 MB; JS SDK = 0; annex migrated away |
| `.scratch/research-voice-agent-prior-art.md` | LiveKit Agents reference, barge-in patterns, latency budget |
| `.scratch/research-stt-tts-patterns.md` | earshot VAD, cpal/rodio, rubato resampling |
| `.scratch/research-sherpa-tts.md` | sherpa-onnx Moonshine STT; Kokoro TTS not used (Supertonic chosen) |
| `.scratch/research-dual-mic.md` | macOS supports dual mic; AudioWorklet fan-out is strictly better |
| `.scratch/review-codex.md` | Crossfire round 1: security model, IPC design, auth gaps |
| `.scratch/review-opus-arch.md` | Crossfire round 1: dual ONNX runtime, PCM IPC overhead |
| `.scratch/review-opus-ux.md` | Crossfire round 1: multi-agent chaos, latency perception |
