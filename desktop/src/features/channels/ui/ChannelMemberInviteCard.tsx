import { Search, UserPlus, X } from "lucide-react";
import * as React from "react";

import { useUserSearchQuery } from "@/features/profile/hooks";
import type {
  AddChannelMembersResult,
  ChannelMember,
  UserSearchResult,
} from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { truncatePubkey as formatPubkey } from "@/shared/lib/pubkey";

function formatSearchUserName(user: UserSearchResult) {
  return (
    user.displayName?.trim() ||
    user.nip05Handle?.trim() ||
    formatPubkey(user.pubkey)
  );
}

function formatSearchUserSecondary(user: UserSearchResult) {
  const displayName = user.displayName?.trim();
  const nip05Handle = user.nip05Handle?.trim();

  if (displayName && nip05Handle) {
    return nip05Handle;
  }

  return formatPubkey(user.pubkey);
}

export function ChannelMemberInviteCard({
  existingMembers,
  isPending,
  onSubmit,
  open,
  requestErrorMessage,
}: {
  existingMembers: ChannelMember[];
  isPending: boolean;
  onSubmit: (input: {
    pubkeys: string[];
    role: Exclude<ChannelMember["role"], "owner">;
  }) => Promise<AddChannelMembersResult>;
  open: boolean;
  requestErrorMessage?: string | null;
}) {
  const [invitePubkeys, setInvitePubkeys] = React.useState("");
  const [inviteQuery, setInviteQuery] = React.useState("");
  const [selectedInvitees, setSelectedInvitees] = React.useState<
    UserSearchResult[]
  >([]);
  const [inviteRole, setInviteRole] =
    React.useState<Exclude<ChannelMember["role"], "owner">>("member");
  const [submissionErrors, setSubmissionErrors] = React.useState<
    AddChannelMembersResult["errors"]
  >([]);

  const deferredInviteQuery = React.useDeferredValue(inviteQuery.trim());
  const selectedInviteePubkeys = React.useMemo(
    () =>
      new Set(selectedInvitees.map((invitee) => invitee.pubkey.toLowerCase())),
    [selectedInvitees],
  );
  const memberPubkeys = React.useMemo(
    () => new Set(existingMembers.map((member) => member.pubkey.toLowerCase())),
    [existingMembers],
  );
  const userSearchQuery = useUserSearchQuery(deferredInviteQuery, {
    enabled: open && deferredInviteQuery.length > 0,
    limit: 8,
  });
  const inviteSearchResults = React.useMemo(
    () =>
      (userSearchQuery.data ?? []).filter(
        (user) =>
          !memberPubkeys.has(user.pubkey.toLowerCase()) &&
          !selectedInviteePubkeys.has(user.pubkey.toLowerCase()),
      ),
    [memberPubkeys, selectedInviteePubkeys, userSearchQuery.data],
  );

  React.useEffect(() => {
    if (!open) {
      setInvitePubkeys("");
      setInviteQuery("");
      setSelectedInvitees([]);
      setSubmissionErrors([]);
    }
  }, [open]);

  const parsedInvitePubkeys = invitePubkeys
    .split(/[\s,]+/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  const inviteTargets = [
    ...new Set([
      ...selectedInvitees.map((invitee) => invitee.pubkey),
      ...parsedInvitePubkeys,
    ]),
  ];

  return (
    <form
      className="space-y-3 rounded-2xl border border-border/80 bg-muted/20 p-4"
      onSubmit={(event) => {
        event.preventDefault();
        void onSubmit({
          pubkeys: inviteTargets,
          role: inviteRole,
        }).then((result) => {
          const addedPubkeys = new Set(
            result.added.map((pubkey) => pubkey.toLowerCase()),
          );
          setSelectedInvitees((current) =>
            current.filter(
              (invitee) => !addedPubkeys.has(invitee.pubkey.toLowerCase()),
            ),
          );
          setInvitePubkeys(
            parsedInvitePubkeys
              .filter((pubkey) => !addedPubkeys.has(pubkey.toLowerCase()))
              .join("\n"),
          );
          setInviteQuery("");
          setSubmissionErrors(result.errors);
        });
      }}
    >
      <div className="flex items-center gap-2 text-sm font-medium">
        <UserPlus className="h-4 w-4" />
        Add members
      </div>
      <div className="space-y-2">
        <label
          className="text-sm font-medium"
          htmlFor="channel-management-search-users"
        >
          Search people
        </label>
        <div className="rounded-xl border border-border/80 bg-background">
          <div className="flex items-center gap-2 px-3 py-2">
            <Search className="h-4 w-4 text-muted-foreground" />
            <Input
              className="h-auto border-0 px-0 py-0 shadow-none focus-visible:ring-0"
              data-testid="channel-management-search-users"
              disabled={isPending}
              id="channel-management-search-users"
              onChange={(event) => setInviteQuery(event.target.value)}
              placeholder="Search by name, NIP-05, or pubkey."
              value={inviteQuery}
            />
          </div>
          {selectedInvitees.length > 0 ? (
            <div className="flex flex-wrap gap-2 border-t border-border/70 px-3 py-2">
              {selectedInvitees.map((invitee) => (
                <div
                  className="inline-flex items-center gap-2 rounded-full border border-border/80 bg-muted/60 px-3 py-1 text-xs"
                  data-testid={`selected-invitee-${invitee.pubkey}`}
                  key={invitee.pubkey}
                >
                  <span className="font-medium">
                    {formatSearchUserName(invitee)}
                  </span>
                  <button
                    aria-label={`Remove ${formatSearchUserName(invitee)}`}
                    className="text-muted-foreground transition-colors hover:text-foreground"
                    onClick={() => {
                      setSelectedInvitees((current) =>
                        current.filter(
                          (candidate) => candidate.pubkey !== invitee.pubkey,
                        ),
                      );
                    }}
                    type="button"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                </div>
              ))}
            </div>
          ) : null}
          {deferredInviteQuery.length > 0 ? (
            <div className="border-t border-border/70 px-2 py-2">
              {userSearchQuery.isLoading ? (
                <p className="px-2 py-1 text-sm text-muted-foreground">
                  Searching…
                </p>
              ) : inviteSearchResults.length > 0 ? (
                <div className="space-y-1">
                  {inviteSearchResults.map((result) => (
                    <button
                      className="flex w-full items-center justify-between rounded-lg px-3 py-2 text-left transition-colors hover:bg-accent hover:text-accent-foreground"
                      data-testid={`channel-user-search-result-${result.pubkey}`}
                      key={result.pubkey}
                      onClick={() => {
                        setSelectedInvitees((current) => [...current, result]);
                        setInviteQuery("");
                      }}
                      type="button"
                    >
                      <div className="min-w-0">
                        <p className="truncate text-sm font-medium">
                          {formatSearchUserName(result)}
                        </p>
                        <p className="truncate text-xs text-muted-foreground">
                          {formatSearchUserSecondary(result)}
                        </p>
                      </div>
                      <span className="text-xs text-muted-foreground">Add</span>
                    </button>
                  ))}
                </div>
              ) : (
                <p className="px-2 py-1 text-sm text-muted-foreground">
                  No matching users.
                </p>
              )}
            </div>
          ) : null}
        </div>
        {userSearchQuery.error instanceof Error ? (
          <p className="text-sm text-destructive">
            {userSearchQuery.error.message}
          </p>
        ) : null}
      </div>
      <div className="space-y-1.5">
        <label
          className="text-sm font-medium"
          htmlFor="channel-management-add-pubkeys"
        >
          Paste pubkeys
        </label>
        <Textarea
          className="min-h-24"
          data-testid="channel-management-add-pubkeys"
          disabled={isPending}
          id="channel-management-add-pubkeys"
          onChange={(event) => setInvitePubkeys(event.target.value)}
          placeholder="Optional: paste one or more pubkeys, separated by spaces, commas, or new lines."
          value={invitePubkeys}
        />
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <label
          className="flex items-center gap-2 text-sm text-muted-foreground"
          htmlFor="channel-member-role"
        >
          Role
        </label>
        <select
          className="h-9 rounded-md border border-input bg-background px-3 text-sm"
          data-testid="channel-management-add-role"
          disabled={isPending}
          id="channel-member-role"
          onChange={(event) =>
            setInviteRole(
              event.target.value as Exclude<ChannelMember["role"], "owner">,
            )
          }
          value={inviteRole}
        >
          {["member", "admin", "guest", "bot"].map((role) => (
            <option key={role} value={role}>
              {role}
            </option>
          ))}
        </select>
        <Button
          data-testid="channel-management-add-members"
          disabled={isPending || inviteTargets.length === 0}
          size="sm"
          type="submit"
        >
          {isPending ? "Adding..." : "Add members"}
        </Button>
      </div>
      {requestErrorMessage ? (
        <p className="text-sm text-destructive">{requestErrorMessage}</p>
      ) : null}
      {submissionErrors.length > 0 ? (
        <div className="space-y-1 text-sm text-destructive">
          {submissionErrors.map((error) => (
            <p key={`${error.pubkey}-${error.error}`}>
              {formatPubkey(error.pubkey)}: {error.error}
            </p>
          ))}
        </div>
      ) : null}
    </form>
  );
}
