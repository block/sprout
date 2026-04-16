import { Check, ChevronUp, Mic, MicOff } from "lucide-react";

import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";

type MicControlsProps = {
  isMuted: boolean;
  onToggleMute: () => void;
  isPttMode: boolean;
  pttActive: boolean;
  micConnected: boolean;
  audioDevices: MediaDeviceInfo[];
  selectedDeviceId: string;
  onSelectDevice: (id: string) => void;
  micGain: number;
  onGainChange: (value: number) => void;
};

export function MicControls({
  isMuted,
  onToggleMute,
  isPttMode,
  pttActive,
  micConnected,
  audioDevices,
  selectedDeviceId,
  onSelectDevice,
  micGain,
  onGainChange,
}: MicControlsProps) {
  return (
    <Popover>
      <div className="flex items-center">
        <Button
          aria-label={
            isMuted
              ? "Unmute microphone"
              : isPttMode
                ? "Force mute (overrides PTT)"
                : "Mute microphone"
          }
          aria-pressed={isMuted}
          className={cn(
            "h-8 w-8 rounded-r-none",
            isPttMode &&
              pttActive &&
              !isMuted &&
              "ring-2 ring-green-500 ring-offset-1 ring-offset-background",
          )}
          onClick={onToggleMute}
          size="icon"
          variant={isMuted ? "destructive" : "secondary"}
        >
          {isMuted ? (
            <MicOff className="h-4 w-4" />
          ) : (
            <Mic className="h-4 w-4" />
          )}
        </Button>
        <PopoverTrigger asChild>
          <Button
            aria-label="Mic settings"
            className="h-8 w-5 rounded-l-none border-l px-0"
            size="icon"
            variant="secondary"
          >
            <ChevronUp className="h-3 w-3" />
          </Button>
        </PopoverTrigger>
      </div>
      <PopoverContent side="top" className="w-64">
        <div className="flex flex-col gap-3">
          <div>
            <span className="mb-1 block text-xs font-medium">Microphone</span>
            <ul className="flex flex-col">
              <li>
                <button
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent"
                  onClick={() => onSelectDevice("")}
                  type="button"
                >
                  <Check
                    className={cn(
                      "h-3 w-3 shrink-0",
                      selectedDeviceId && "invisible",
                    )}
                  />
                  System default
                </button>
              </li>
              {audioDevices.map((d) => {
                const isSelected = selectedDeviceId === d.deviceId;
                return (
                  <li key={d.deviceId}>
                    <button
                      className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs transition-colors hover:bg-accent"
                      onClick={() => onSelectDevice(d.deviceId)}
                      type="button"
                    >
                      <Check
                        className={cn(
                          "h-3 w-3 shrink-0",
                          !isSelected && "invisible",
                        )}
                      />
                      <span className="truncate">
                        {d.label || `Mic ${d.deviceId.slice(0, 8)}`}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
            {selectedDeviceId && micConnected && (
              <p className="mt-1 text-[10px] text-muted-foreground">
                Change takes effect on next huddle
              </p>
            )}
          </div>
          <div>
            <label
              htmlFor="mic-volume"
              className="mb-1 block text-xs font-medium"
            >
              Volume
            </label>
            <div className="flex items-center gap-2">
              <input
                id="mic-volume"
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={micGain}
                onChange={(e) => onGainChange(Number(e.target.value))}
                className="h-1.5 w-full cursor-pointer appearance-none rounded-full bg-muted accent-foreground"
              />
              <span className="w-8 text-right text-xs text-muted-foreground">
                {Math.round(micGain * 100)}%
              </span>
            </div>
          </div>
        </div>
      </PopoverContent>
    </Popover>
  );
}
