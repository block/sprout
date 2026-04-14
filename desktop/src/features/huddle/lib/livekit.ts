import {
  LocalAudioTrack,
  Room,
  RoomEvent,
  type Participant,
} from "livekit-client";

export interface HuddleConnection {
  room: Room;
  localAudioTrack: MediaStreamTrack;
  disconnect: () => Promise<void>;
}

export type HuddleRoomCallbacks = {
  onActiveSpeakersChanged?: (speakers: Participant[]) => void;
  onDisconnected?: () => void;
  onReconnecting?: () => void;
  onReconnected?: () => void;
};

/**
 * LiveKit connection lifecycle:
 *
 *   connectToHuddle(url, token, callbacks?)
 *     → getUserMedia({ audio: true })   [mic permission]
 *     → room.connect(url, token)        [WebRTC signaling]
 *     → register room event listeners  [active speakers, disconnect, reconnect]
 *     → room.localParticipant.publishTrack(audioTrack)
 *     → returns { room, localAudioTrack, disconnect }
 *
 *   disconnect()
 *     → room.removeAllListeners()
 *     → room.disconnect()
 *     → stream.getTracks().forEach(t => t.stop())
 *
 *   Error handling: mic stream is always cleaned up, even on partial failure.
 */
export async function connectToHuddle(
  url: string,
  token: string,
  callbacks?: HuddleRoomCallbacks,
): Promise<HuddleConnection> {
  const room = new Room();
  let stream: MediaStream | null = null;

  try {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true },
    });
    const audioTrack = stream.getAudioTracks()[0];

    await room.connect(url, token);

    // Register room event listeners before publishing track
    if (callbacks?.onActiveSpeakersChanged) {
      room.on(
        RoomEvent.ActiveSpeakersChanged,
        callbacks.onActiveSpeakersChanged,
      );
    }
    if (callbacks?.onDisconnected) {
      room.on(RoomEvent.Disconnected, callbacks.onDisconnected);
    }
    if (callbacks?.onReconnecting) {
      room.on(RoomEvent.Reconnecting, callbacks.onReconnecting);
    }
    if (callbacks?.onReconnected) {
      room.on(RoomEvent.Reconnected, callbacks.onReconnected);
    }

    try {
      // false = don't let LiveKit manage the track lifecycle
      const localTrack = new LocalAudioTrack(audioTrack, undefined, false);
      await room.localParticipant.publishTrack(localTrack);
    } catch (publishErr) {
      // Publish failed after connect — disconnect room before propagating
      room.removeAllListeners();
      room.disconnect();
      throw publishErr;
    }

    return {
      room,
      localAudioTrack: audioTrack,
      disconnect: async () => {
        try {
          room.removeAllListeners();
          room.disconnect();
        } finally {
          stream?.getTracks().forEach((t) => {
            t.stop();
          });
        }
      },
    };
  } catch (err) {
    // Clean up mic stream on any failure (getUserMedia, connect, or publish)
    stream?.getTracks().forEach((t) => {
      t.stop();
    });
    throw err;
  }
}
