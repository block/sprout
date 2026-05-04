export type Workspace = {
  id: string;
  name: string;
  relayUrl: string;
  token?: string;
  /**
   * The pubkey associated with the active identity at the time the workspace
   * was created. Display-only — auth always uses the persisted `identity.key`
   * file resolved at startup, never this field.
   */
  pubkey?: string;
  addedAt: string;
  /**
   * @deprecated Never read. Kept on the type so old localStorage entries
   * deserialise without errors. New entries never set this field, and
   * `loadWorkspaces()` strips it on read so it cannot leak forward. The
   * authoritative private key is the on-disk `identity.key` file.
   */
  nsec?: never;
};
