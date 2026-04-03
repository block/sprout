import {
  useSetChannelPurposeMutation,
  useSetChannelTopicMutation,
  useUpdateChannelMutation,
} from "@/features/channels/hooks";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Section } from "@/shared/ui/Section";
import { Separator } from "@/shared/ui/separator";
import { Textarea } from "@/shared/ui/textarea";
import { ChannelCanvas } from "./ChannelCanvas";

type ChannelDetailsSectionProps = {
  channelId: string | null;
  canManageChannel: boolean;
  canEditNarrative: boolean;
  isArchived: boolean;
  nameDraft: string;
  descriptionDraft: string;
  topicDraft: string;
  purposeDraft: string;
  onNameChange: (value: string) => void;
  onDescriptionChange: (value: string) => void;
  onTopicChange: (value: string) => void;
  onPurposeChange: (value: string) => void;
};

export function ChannelDetailsSection({
  channelId,
  canManageChannel,
  canEditNarrative,
  isArchived,
  nameDraft,
  descriptionDraft,
  topicDraft,
  purposeDraft,
  onNameChange,
  onDescriptionChange,
  onTopicChange,
  onPurposeChange,
}: ChannelDetailsSectionProps) {
  const updateChannelMutation = useUpdateChannelMutation(channelId);
  const setTopicMutation = useSetChannelTopicMutation(channelId);
  const setPurposeMutation = useSetChannelPurposeMutation(channelId);

  return (
    <>
      <Section
        description="Name and description are owner/admin actions."
        title="Details"
      >
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void updateChannelMutation.mutateAsync({
              description: descriptionDraft.trim() || undefined,
              name: nameDraft.trim() || undefined,
            });
          }}
        >
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="channel-name">
              Name
            </label>
            <Input
              data-testid="channel-management-name"
              disabled={!canManageChannel || updateChannelMutation.isPending}
              id="channel-name"
              onChange={(event) => onNameChange(event.target.value)}
              value={nameDraft}
            />
          </div>
          <div className="space-y-1.5">
            <label
              className="text-sm font-medium"
              htmlFor="channel-description"
            >
              Description
            </label>
            <Textarea
              className="min-h-24"
              data-testid="channel-management-description"
              disabled={!canManageChannel || updateChannelMutation.isPending}
              id="channel-description"
              onChange={(event) => onDescriptionChange(event.target.value)}
              value={descriptionDraft}
            />
          </div>
          <Button
            data-testid="channel-management-save-details"
            disabled={!canManageChannel || updateChannelMutation.isPending}
            size="sm"
            type="submit"
          >
            {updateChannelMutation.isPending ? "Saving..." : "Save details"}
          </Button>
          {updateChannelMutation.error instanceof Error ? (
            <p className="text-sm text-destructive">
              {updateChannelMutation.error.message}
            </p>
          ) : null}
        </form>
      </Section>

      <Separator />

      <Section
        description="Topic and purpose show the current context for the channel."
        title="Context"
      >
        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void setTopicMutation.mutateAsync({ topic: topicDraft.trim() });
          }}
        >
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="channel-topic">
              Topic
            </label>
            <Input
              data-testid="channel-management-topic"
              disabled={!canEditNarrative || setTopicMutation.isPending}
              id="channel-topic"
              onChange={(event) => onTopicChange(event.target.value)}
              value={topicDraft}
            />
          </div>
          <Button
            data-testid="channel-management-save-topic"
            disabled={!canEditNarrative || setTopicMutation.isPending}
            size="sm"
            type="submit"
            variant="outline"
          >
            {setTopicMutation.isPending ? "Saving..." : "Save topic"}
          </Button>
          {setTopicMutation.error instanceof Error ? (
            <p className="text-sm text-destructive">
              {setTopicMutation.error.message}
            </p>
          ) : null}
        </form>

        <form
          className="space-y-3"
          onSubmit={(event) => {
            event.preventDefault();
            void setPurposeMutation.mutateAsync({
              purpose: purposeDraft.trim(),
            });
          }}
        >
          <div className="space-y-1.5">
            <label className="text-sm font-medium" htmlFor="channel-purpose">
              Purpose
            </label>
            <Textarea
              className="min-h-24"
              data-testid="channel-management-purpose"
              disabled={!canEditNarrative || setPurposeMutation.isPending}
              id="channel-purpose"
              onChange={(event) => onPurposeChange(event.target.value)}
              value={purposeDraft}
            />
          </div>
          <Button
            data-testid="channel-management-save-purpose"
            disabled={!canEditNarrative || setPurposeMutation.isPending}
            size="sm"
            type="submit"
            variant="outline"
          >
            {setPurposeMutation.isPending ? "Saving..." : "Save purpose"}
          </Button>
          {setPurposeMutation.error instanceof Error ? (
            <p className="text-sm text-destructive">
              {setPurposeMutation.error.message}
            </p>
          ) : null}
        </form>
      </Section>

      <Separator />

      <Section
        description="A shared Markdown document for the channel."
        title="Canvas"
      >
        <ChannelCanvas
          canEdit={canEditNarrative}
          channelId={channelId}
          isArchived={isArchived}
        />
      </Section>
    </>
  );
}
