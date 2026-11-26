export type CoreSurfaceCursor = {
  hotspotX: number;
  hotspotY: number;
  width: number;
  height: number;
  pixels: number[];
};

/** Converts the infrequent Core BGRA cursor update into the browser cursor surface. */
export function applyCoreSurfaceCursor(target: HTMLCanvasElement, cursor: CoreSurfaceCursor): void {
  if (!cursor.width || !cursor.height) {
    target.style.cursor = "none";
    return;
  }
  if (cursor.pixels.length !== cursor.width * cursor.height * 4) {
    throw new Error("surface_cursor_length_mismatch");
  }
  const canvas = document.createElement("canvas");
  canvas.width = cursor.width;
  canvas.height = cursor.height;
  const context = canvas.getContext("2d", { alpha: true });
  if (!context) throw new Error("surface_cursor_context_unavailable");
  const rgba = new Uint8ClampedArray(cursor.pixels);
  for (let offset = 0; offset < rgba.length; offset += 4) {
    const blue = rgba[offset];
    rgba[offset] = rgba[offset + 2];
    rgba[offset + 2] = blue;
  }
  context.putImageData(new ImageData(rgba, cursor.width, cursor.height), 0, 0);
  target.style.cursor = `url("${canvas.toDataURL("image/png")}") ${cursor.hotspotX} ${cursor.hotspotY}, default`;
}
