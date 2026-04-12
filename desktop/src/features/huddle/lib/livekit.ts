import { LocalAudioTrack, Room } from "livekit-client";

export interface HuddleConnection {
  room: Room;
  localAudioTrack: MediaStreamTrack;
  disconnect: () => Promise<void>;
}

/**
 * LiveKit connection lifecycle:
 *
 *   connectToHuddle(url, token)
 *     → getUserMedia({ audio: true })   [mic permission]
 *     → room.connect(url, token)        [WebRTC signaling]
 *     → room.localParticipant.publishTrack(audioTrack)
 *     → returns { room, localAudioTrack, disconnect }
 *
 *   disconnect()
 *     → room.disconnect()
 *     → stream.getTracks().forEach(t => t.stop())
 *
 *   Error handling: mic stream is always cleaned up, even on partial failure.
 */
export async function connectToHuddle(
  url: string,
  token: string,
): Promise<HuddleConnection> {
  const room = new Room();
  let stream: MediaStream | null = null;

  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const audioTrack = stream.getAudioTracks()[0];

    await room.connect(url, token);

    try {
      // false = don't let LiveKit manage the track lifecycle
      const localTrack = new LocalAudioTrack(audioTrack, undefined, false);
      await room.localParticipant.publishTrack(localTrack);
    } catch (publishErr) {
      // Publish failed after connect — disconnect room before propagating
      room.disconnect();
      throw publishErr;
    }

    return {
      room,
      localAudioTrack: audioTrack,
      disconnect: async () => {
        room.disconnect();
        stream?.getTracks().forEach((t) => {
          t.stop();
        });
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
