import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";

type ArchiveIdentityConfirmDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  isPending: boolean;
  displayName: string;
  /**
   * Which authority is publishing the archive request.
   * - "owner": NIP-OA owner archiving an agent they manage (consent=owner).
   * - "admin": relay admin/owner archiving any identity on this relay.
   */
  consentPath: "owner" | "admin";
};

export function ArchiveIdentityConfirmDialog({
  open,
  onOpenChange,
  onConfirm,
  isPending,
  displayName,
  consentPath,
}: ArchiveIdentityConfirmDialogProps) {
  const title =
    consentPath === "owner"
      ? `Archive ${displayName}?`
      : `Archive ${displayName} on this relay?`;

  // Copy mirrors NIP-IA semantics implemented in identity-archive/hooks.ts:
  // - publishes a public kind:9035 archive request,
  // - relay emits a kind:13535 snapshot delta adding the pubkey,
  // - archived identity is folded out of forward-looking discovery
  //   (mention autocomplete, DM picker, member-add, search) for everyone
  //   except the agent itself (self-exempt anti-shadowban),
  // - reversible via Unarchive on this same panel.
  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            This publishes a public archive request on this relay. {displayName}{" "}
            will be hidden from mention autocomplete, DM pickers, member lists,
            and search for other users on this relay. The agent will still see
            itself (anti-shadowban). You can reverse this from the same profile
            panel.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button type="button" variant="outline">
              Cancel
            </Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button
              data-testid="archive-identity-confirm"
              disabled={isPending}
              onClick={onConfirm}
              type="button"
              variant="destructive"
            >
              {isPending ? "Archiving…" : "Archive identity"}
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
