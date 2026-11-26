const MAGIC = "VNCF";
const HEADER_LENGTH = 28;
const RECT_HEADER_LENGTH = 16;

export type CoreSurfaceRectangle = {
  x: number;
  y: number;
  width: number;
  height: number;
  pixels: Uint8ClampedArray;
};

export type CoreSurfaceFrame = {
  frameId: bigint;
  width: number;
  height: number;
  rectangles: CoreSurfaceRectangle[];
};

/** Decodes Canvas-native RGBA renderer packets only; RFB decoding remains in Rust. */
export function decodeCoreSurfaceFrame(packet: ArrayBuffer): CoreSurfaceFrame {
  const view = new DataView(packet);
  if (view.byteLength < HEADER_LENGTH) throw new Error("surface_packet_truncated");
  const magic = String.fromCharCode(...new Uint8Array(packet, 0, 4));
  if (magic !== MAGIC || view.getUint8(4) !== 1 || view.getUint16(6) !== HEADER_LENGTH) {
    throw new Error("surface_packet_invalid_header");
  }
  const frameId = view.getBigUint64(8);
  const width = view.getUint32(16);
  const height = view.getUint32(20);
  const count = view.getUint16(24);
  const headersEnd = HEADER_LENGTH + count * RECT_HEADER_LENGTH;
  if (headersEnd > view.byteLength) throw new Error("surface_packet_truncated");
  let payloadOffset = headersEnd;
  const rectangles: CoreSurfaceRectangle[] = [];
  for (let index = 0; index < count; index += 1) {
    const offset = HEADER_LENGTH + index * RECT_HEADER_LENGTH;
    const x = view.getUint32(offset);
    const y = view.getUint32(offset + 4);
    const rectWidth = view.getUint32(offset + 8);
    const rectHeight = view.getUint32(offset + 12);
    if (!rectWidth || !rectHeight || x + rectWidth > width || y + rectHeight > height) {
      throw new Error("surface_packet_rect_out_of_bounds");
    }
    const pixelLength = rectWidth * rectHeight * 4;
    if (!Number.isSafeInteger(pixelLength) || payloadOffset + pixelLength > view.byteLength) {
      throw new Error("surface_packet_truncated");
    }
    rectangles.push({
      x, y, width: rectWidth, height: rectHeight,
      pixels: new Uint8ClampedArray(packet, payloadOffset, pixelLength),
    });
    payloadOffset += pixelLength;
  }
  if (payloadOffset !== view.byteLength) throw new Error("surface_packet_trailing_bytes");
  return { frameId, width, height, rectangles };
}

export type CoreSurfaceCanvasRenderer = {
  present: (frame: CoreSurfaceFrame) => void;
  reset: () => void;
};

export function createCoreSurfaceCanvasRenderer(canvas: HTMLCanvasElement): CoreSurfaceCanvasRenderer {
  const context = canvas.getContext("2d", { alpha: false });
  if (!context) throw new Error("surface_canvas_context_unavailable");
  let lastFrameId = -1n;
  const requireContext = () => {
    // Feature-detect for WebViews without this method. The lost DOM event may
    // follow the rAF callback, and a discarded draw must never be ACKed.
    if (typeof context.isContextLost === "function" && context.isContextLost()) {
      throw new Error("surface_canvas_context_lost");
    }
  };
  return {
    present(frame) {
      requireContext();
      if (frame.frameId <= lastFrameId) return;
      if (canvas.width !== frame.width || canvas.height !== frame.height) {
        canvas.width = frame.width;
        canvas.height = frame.height;
      }
      for (const rectangle of frame.rectangles) {
        context.putImageData(
          new ImageData(rectangle.pixels, rectangle.width, rectangle.height),
          rectangle.x,
          rectangle.y,
        );
      }
      requireContext();
      lastFrameId = frame.frameId;
    },
    reset() { lastFrameId = -1n; },
  };
}
