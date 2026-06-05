import { useId } from "react";

import { cn } from "@/shared/lib/cn";

type ArcadeSegmentedControlOption<T extends string> = {
  label: string;
  value: T;
};

type ArcadeSegmentedControlProps<T extends string> = {
  "aria-label": string;
  className?: string;
  onChange: (value: T) => void;
  onOptionPress?: (value: T) => void;
  options: ArcadeSegmentedControlOption<T>[];
  value: T;
};

export function ArcadeSegmentedControl<T extends string>({
  "aria-label": ariaLabel,
  className,
  onChange,
  onOptionPress,
  options,
  value,
}: ArcadeSegmentedControlProps<T>) {
  const radioGroupName = useId();
  const activeIndex = Math.max(
    options.findIndex((option) => option.value === value),
    0,
  );

  return (
    <div
      aria-label={ariaLabel}
      className={cn(
        "relative grid h-14 rounded-full bg-[var(--arcade-segmented-control-background)] p-1",
        className,
      )}
      role="radiogroup"
      style={{
        gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))`,
      }}
    >
      <div
        aria-hidden="true"
        className="absolute bottom-1 left-1 top-1 rounded-full bg-[var(--arcade-segmented-control-button-background-selected)] shadow-[0_1px_4px_rgba(0,0,0,0.08)] transition-transform duration-300 ease-[cubic-bezier(0.22,1,0.36,1)]"
        style={{
          transform: `translateX(calc(${activeIndex} * 100%))`,
          width: `calc((100% - 8px) / ${options.length})`,
        }}
      />
      {options.map((option) => {
        const isChecked = option.value === value;
        return (
          <label
            className="arcade-type-label-medium relative z-10 flex h-full cursor-default items-center justify-center rounded-full text-[var(--arcade-segmented-control-button-text)]"
            key={option.value}
            onPointerDown={() => {
              if (!isChecked) {
                onOptionPress?.(option.value);
              }
            }}
          >
            <input
              checked={isChecked}
              className="absolute inset-0 z-20 cursor-default opacity-0"
              name={radioGroupName}
              onChange={() => onChange(option.value)}
              onKeyDown={(event) => {
                if (
                  !event.repeat &&
                  !isChecked &&
                  (event.key === "Enter" || event.key === " ")
                ) {
                  onOptionPress?.(option.value);
                }
              }}
              type="radio"
              value={option.value}
            />
            <span className="relative z-10">{option.label}</span>
          </label>
        );
      })}
    </div>
  );
}
