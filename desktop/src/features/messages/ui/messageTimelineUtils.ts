const BOTTOM_THRESHOLD_PX = 72;

export function isNearBottom(container: HTMLDivElement) {
  return (
    container.scrollHeight - container.clientHeight - container.scrollTop <=
    BOTTOM_THRESHOLD_PX
  );
}
