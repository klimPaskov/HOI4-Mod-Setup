// Renders the user's context images into the bounded files the core accepts:
// a cover-cropped square PNG thumbnail and a downscaled JPEG visual reference.
// Both return plain base64 (no data: prefix). The core validates the bytes
// again before anything is staged.

const JPEG_QUALITIES = [0.85, 0.75, 0.65, 0.55, 0.45, 0.35];
const MAX_DOWNSCALE_STEPS = 4;

function loadImage(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      if (image.naturalWidth > 0 && image.naturalHeight > 0) resolve(image);
      else reject(new Error("The image has no visible size."));
    };
    image.onerror = () => reject(new Error("The image could not be read."));
    image.src = dataUrl;
  });
}

function canvasContext(width: number, height: number): { canvas: HTMLCanvasElement; context: CanvasRenderingContext2D } {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Image rendering is unavailable.");
  context.imageSmoothingEnabled = true;
  context.imageSmoothingQuality = "high";
  return { canvas, context };
}

function base64FromDataUrl(dataUrl: string, mediaType: string): string {
  const prefix = `data:${mediaType};base64,`;
  if (!dataUrl.startsWith(prefix)) throw new Error("The image could not be encoded.");
  return dataUrl.slice(prefix.length);
}

/** Decoded byte length of a base64 payload. */
export function base64ByteLength(base64: string): number {
  const payload = base64.trim();
  const padding = payload.endsWith("==") ? 2 : payload.endsWith("=") ? 1 : 0;
  return Math.floor((payload.length * 3) / 4) - padding;
}

/** A data URL for showing a rendered base64 image. */
export function base64DataUrl(base64: string, mediaType: string): string {
  return base64.startsWith("data:") ? base64 : `data:${mediaType};base64,${base64}`;
}

/** Render the image centered and cover-cropped to an exact `size` square PNG. */
export async function renderCoverPng(dataUrl: string, size: number): Promise<string> {
  const image = await loadImage(dataUrl);
  const width = image.naturalWidth;
  const height = image.naturalHeight;
  const crop = Math.min(width, height);
  const { canvas, context } = canvasContext(size, size);
  context.drawImage(image, (width - crop) / 2, (height - crop) / 2, crop, crop, 0, 0, size, size);
  return base64FromDataUrl(canvas.toDataURL("image/png"), "image/png");
}

/** Render the image as a JPEG whose longest edge is at most `maxEdge` pixels
 * and whose size is at most `maxBytes`, lowering quality and then size. */
export async function renderReferenceJpeg(dataUrl: string, maxEdge: number, maxBytes: number): Promise<string> {
  const image = await loadImage(dataUrl);
  let scale = Math.min(1, maxEdge / Math.max(image.naturalWidth, image.naturalHeight));
  for (let step = 0; step <= MAX_DOWNSCALE_STEPS; step += 1) {
    const width = Math.max(1, Math.round(image.naturalWidth * scale));
    const height = Math.max(1, Math.round(image.naturalHeight * scale));
    const { canvas, context } = canvasContext(width, height);
    // JPEG has no transparency; flatten onto white instead of black.
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, width, height);
    context.drawImage(image, 0, 0, width, height);
    for (const quality of JPEG_QUALITIES) {
      const encoded = base64FromDataUrl(canvas.toDataURL("image/jpeg", quality), "image/jpeg");
      if (base64ByteLength(encoded) <= maxBytes) return encoded;
    }
    scale *= 0.75;
  }
  throw new Error("The image is too detailed to save as a reference.");
}
