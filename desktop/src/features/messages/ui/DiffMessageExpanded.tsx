import { DiffEditor } from '@monaco-editor/react';
import { useMemo } from 'react';

import { parseDiffToOldNew } from '@/features/messages/lib/parseDiff';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/shared/ui/dialog';

type DiffMessageExpandedProps = {
  content: string;
  language?: string;
  filePath?: string;
  onClose: () => void;
};

export default function DiffMessageExpanded({
  content,
  language,
  filePath,
  onClose,
}: DiffMessageExpandedProps) {
  const { original, modified } = useMemo(
    () => parseDiffToOldNew(content),
    [content],
  );

  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      open
    >
      <DialogContent className="max-w-5xl w-full h-[80vh] flex flex-col p-0 gap-0">
        <DialogHeader className="px-4 py-3 border-b border-border/50 shrink-0">
          <DialogTitle className="text-sm font-mono font-medium truncate">
            {filePath ?? 'Diff Viewer'}{' '}
            <span className="text-muted-foreground text-xs font-normal">
              (hunks-only preview — shows changed regions, not full file)
            </span>
          </DialogTitle>
        </DialogHeader>
        <div className="flex-1 min-h-0">
          <DiffEditor
            height="100%"
            language={language ?? 'plaintext'}
            modified={modified}
            options={{
              readOnly: true,
              renderSideBySide: true,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              fontSize: 13,
            }}
            original={original}
            theme="vs-dark"
          />
        </div>
      </DialogContent>
    </Dialog>
  );
}
