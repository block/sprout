# MCP threading (`parent_event_id`) contract

This document describes how Sprout interprets `parent_event_id` for the MCP tools:

- `send_message`
- `send_diff_message`

## Overview

`parent_event_id` is a *threading hint*.

Sprout will attempt to post the new event as a reply in the same thread as the parent. If Sprout
cannot establish a valid NIP-10 thread context from the parent, the hint is ignored and the message
is posted as a top-level channel message.

## Stream messages and diffs (channel timeline)

For `send_message` / `send_diff_message` messages posted to a channel timeline, `parent_event_id` is
only honored if the **parent event already contains NIP-10 `e` tags with a `root` or `reply` marker**.

If the parent event does **not** include NIP-10 `e` tags marked `root`/`reply`, Sprout ignores
`parent_event_id` and posts the message as a new top-level event.

## Forums (kind `45003` comments)

Forum comment behavior is unchanged.

When posting a forum comment (kind `45003`), `parent_event_id` continues to behave as the required
identifier of the post/comment being replied to.
