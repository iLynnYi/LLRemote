type ViewportScrollArea = Pick<HTMLElement,
  "clientLeft" | "clientTop" | "clientWidth" | "clientHeight"
  | "scrollWidth" | "scrollHeight" | "scrollLeft" | "scrollTop"
> & { getBoundingClientRect: () => { left: number; top: number } };

type ViewportPoint = Pick<PointerEvent, "clientX" | "clientY">;
type EdgeScrollPointer = ViewportPoint & Pick<PointerEvent, "pointerType" | "altKey" | "buttons">;

/** Exclude native scrollbar tracks so local navigation never becomes remote input. */
export function isViewportContentPoint(target: ViewportScrollArea | null, point: ViewportPoint): boolean {
  if (!target) return false;
  const rect = target.getBoundingClientRect();
  const x = point.clientX - rect.left - target.clientLeft;
  const y = point.clientY - rect.top - target.clientTop;
  return x >= 0 && x < target.clientWidth && y >= 0 && y < target.clientHeight;
}

function edgeSpeed(position: number, size: number): number {
  const edge = Math.min(32, size / 4);
  if (edge <= 0) return 0;
  if (position < edge) return -(80 + 640 * (1 - position / edge) ** 2);
  if (position > size - edge) return 80 + 640 * (1 - (size - position) / edge) ** 2;
  return 0;
}

/** Scroll the local viewport while a mouse hovers near its edges, including corners. */
export function createViewportEdgeScrollController(options: {
  getTarget: () => ViewportScrollArea | null;
  canScroll: (point: ViewportPoint) => boolean;
  requestFrame: (callback: FrameRequestCallback) => number;
  cancelFrame: (id: number) => void;
  onScroll?: (pointer: EdgeScrollPointer) => void;
}) {
  let pointer: EdgeScrollPointer | null = null;
  let frame: number | null = null;
  let previousTime: number | null = null;

  function stop() {
    if (frame !== null) options.cancelFrame(frame);
    frame = null;
    pointer = null;
    previousTime = null;
  }

  function velocity(target: ViewportScrollArea, point: ViewportPoint) {
    const rect = target.getBoundingClientRect();
    return {
      x: target.scrollWidth > target.clientWidth
        ? edgeSpeed(point.clientX - rect.left - target.clientLeft, target.clientWidth) : 0,
      y: target.scrollHeight > target.clientHeight
        ? edgeSpeed(point.clientY - rect.top - target.clientTop, target.clientHeight) : 0,
    };
  }

  function tick(time: number) {
    frame = null;
    const target = options.getTarget();
    if (!pointer || !target || !options.canScroll(pointer) || !isViewportContentPoint(target, pointer)) {
      stop();
      return;
    }
    const speed = velocity(target, pointer);
    const seconds = Math.min(previousTime === null ? 1000 / 60 : time - previousTime, 32) / 1000;
    previousTime = time;
    const oldX = target.scrollLeft;
    const oldY = target.scrollTop;
    target.scrollLeft = Math.max(0, Math.min(target.scrollWidth - target.clientWidth, oldX + speed.x * seconds));
    target.scrollTop = Math.max(0, Math.min(target.scrollHeight - target.clientHeight, oldY + speed.y * seconds));
    if (target.scrollLeft === oldX && target.scrollTop === oldY) {
      stop();
      return;
    }
    options.onScroll?.(pointer);
    frame = options.requestFrame(tick);
  }

  function move(event: EdgeScrollPointer) {
    const target = options.getTarget();
    if (event.pointerType !== "mouse" || event.altKey || (event.buttons & 4)
      || !target || !options.canScroll(event) || !isViewportContentPoint(target, event)) {
      stop();
      return;
    }
    const speed = velocity(target, event);
    if (!speed.x && !speed.y) {
      stop();
      return;
    }
    pointer = event;
    if (frame === null) frame = options.requestFrame(tick);
  }

  return { move, stop };
}
