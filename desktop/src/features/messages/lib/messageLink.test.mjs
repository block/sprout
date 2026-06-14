import assert from "node:assert/strict";
import test from "node:test";

import {
  buildMessageLink,
  isBuzzUrl,
  isMessageLink,
  parseMessageLink,
} from "./messageLink.ts";

const CHANNEL = "f570339f-8f8a-4e08-a779-8d954aa44109";
const MESSAGE =
  "b04819ffc1f7c8ffb49c6d30b5899f470198264680d02e78894a658e30a9059f";
const THREAD =
  "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

test("buildMessageLink → parseMessageLink round-trips without thread", () => {
  const url = buildMessageLink({ channelId: CHANNEL, messageId: MESSAGE });
  assert.equal(url, `buzz://message?channel=${CHANNEL}&id=${MESSAGE}`);

  const parsed = parseMessageLink(url);
  assert.equal(parsed.ok, true);
  assert.deepEqual(parsed.ok && parsed.value, {
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: null,
  });
});

test("buildMessageLink → parseMessageLink round-trips with thread", () => {
  const url = buildMessageLink({
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: THREAD,
  });
  const parsed = parseMessageLink(url);
  assert.equal(parsed.ok, true);
  assert.deepEqual(parsed.ok && parsed.value, {
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: THREAD,
  });
});

test("buildMessageLink treats null/empty thread as absent", () => {
  const a = buildMessageLink({
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: null,
  });
  const b = buildMessageLink({
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: "",
  });
  assert.equal(a, `buzz://message?channel=${CHANNEL}&id=${MESSAGE}`);
  assert.equal(b, `buzz://message?channel=${CHANNEL}&id=${MESSAGE}`);
});

test("buildMessageLink rejects missing required params", () => {
  assert.throws(() => buildMessageLink({ channelId: "", messageId: MESSAGE }));
  assert.throws(() => buildMessageLink({ channelId: CHANNEL, messageId: "" }));
});

test("parseMessageLink rejects unsupported schemes", () => {
  const r = parseMessageLink(
    `https://example.com/?channel=${CHANNEL}&id=${MESSAGE}`,
  );
  assert.equal(r.ok, false);
  assert.equal(r.ok === false && r.reason, "wrong-scheme");
});

test("parseMessageLink rejects buzz:// with wrong host", () => {
  const r = parseMessageLink(`buzz://connect?relay=wss://example.com`);
  assert.equal(r.ok, false);
  assert.equal(r.ok === false && r.reason, "wrong-host");
});

test("parseMessageLink rejects missing channel", () => {
  const r = parseMessageLink(`buzz://message?id=${MESSAGE}`);
  assert.equal(r.ok, false);
  assert.equal(r.ok === false && r.reason, "missing-channel");
});

test("parseMessageLink rejects missing id", () => {
  const r = parseMessageLink(`buzz://message?channel=${CHANNEL}`);
  assert.equal(r.ok, false);
  assert.equal(r.ok === false && r.reason, "missing-id");
});

test("parseMessageLink rejects malformed URL strings", () => {
  const r = parseMessageLink("not a url");
  assert.equal(r.ok, false);
  assert.equal(r.ok === false && r.reason, "invalid-url");
});

test("parseMessageLink accepts legacy buzz://message links", () => {
  const r = parseMessageLink(`buzz://message?channel=${CHANNEL}&id=${MESSAGE}`);
  assert.equal(r.ok, true);
  assert.deepEqual(r.ok && r.value, {
    channelId: CHANNEL,
    messageId: MESSAGE,
    threadRootId: null,
  });
});

test("isMessageLink matches buzz://message and legacy buzz://message", () => {
  assert.equal(
    isMessageLink(`buzz://message?channel=${CHANNEL}&id=${MESSAGE}`),
    true,
  );
  assert.equal(
    isMessageLink(`buzz://message?channel=${CHANNEL}&id=${MESSAGE}`),
    true,
  );
  assert.equal(isMessageLink("buzz://connect?relay=wss://x"), false);
  assert.equal(isMessageLink("buzz://connect?relay=wss://x"), false);
  assert.equal(isMessageLink("https://example.com"), false);
  assert.equal(isMessageLink(undefined), false);
  assert.equal(isMessageLink(""), false);
});

test("isBuzzUrl matches any well-formed buzz:// URL", () => {
  // Message deep-links.
  assert.equal(
    isBuzzUrl(`buzz://message?channel=${CHANNEL}&id=${MESSAGE}`),
    true,
  );
  // Non-message buzz:// URLs (tho wants ALL buzz:// links to wrap).
  assert.equal(isBuzzUrl("buzz://connect?relay=wss://x"), true);
  assert.equal(isBuzzUrl("buzz://channel?id=abc"), true);
  // Surrounding whitespace is tolerated.
  assert.equal(
    isBuzzUrl(`  buzz://message?channel=${CHANNEL}&id=${MESSAGE}  `),
    true,
  );
  // Standard URLs and non-buzz schemes are rejected.
  assert.equal(isBuzzUrl("https://example.com"), false);
  assert.equal(isBuzzUrl("mailto:x@example.com"), false);
  // Malformed / empty input is rejected.
  assert.equal(isBuzzUrl("buzz"), false);
  assert.equal(isBuzzUrl("not a url at all"), false);
  assert.equal(isBuzzUrl(undefined), false);
  assert.equal(isBuzzUrl(null), false);
  assert.equal(isBuzzUrl(""), false);
});
