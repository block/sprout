import { LocalAudioTrack, Room } from "livekit-client";

export interface HuddleConnection {
  room: Room;
  localAudioTrack: MediaStreamTrack;
  disconnect: () => Promise<void>;
}

export async function connectToHuddle(
  url: string,
  token: string,
): Promise<HuddleConnection> {
  const room = new Room();
  let stream: MediaStream | null = null;

  try {
    console.log("[huddle] requesting mic access…");
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    const audioTrack = stream.getAudioTracks()[0];
    console.log("[huddle] mic acquired, connecting to LiveKit", url);

    await room.connect(url, token);
    console.log("[huddle] LiveKit connected, publishing track…");

    try {
      // false = don't let LiveKit manage the track lifecycle
      const localTrack = new LocalAudioTrack(audioTrack, undefined, false);
      await room.localParticipant.publishTrack(localTrack);
      console.log("[huddle] track published");
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
        stream?.getTracks().forEach((t) => t.stop());
      },
    };
  } catch (err) {
    // Clean up mic stream on any failure (getUserMedia, connect, or publish)
    stream?.getTracks().forEach((t) => t.stop());
    throw err;
  }
}
