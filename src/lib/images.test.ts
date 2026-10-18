import { afterEach, describe, expect, it, vi } from "vitest";
import { base64ByteLength, base64DataUrl, renderCoverPng, renderReferenceJpeg } from "./images";

class FakeImage {
  static width = 1200;
  static height = 600;
  naturalWidth = FakeImage.width;
  naturalHeight = FakeImage.height;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  set src(_value: string) {
    queueMicrotask(() => this.onload?.());
  }
}

function fakeCanvas(toDataUrl: (type: string, quality?: number) => string) {
  const context = { drawImage: vi.fn(), fillRect: vi.fn(), fillStyle: "", imageSmoothingEnabled: false, imageSmoothingQuality: "low" };
  const canvases: Array<{ width: number; height: number }> = [];
  vi.spyOn(document, "createElement").mockImplementation((() => {
    const canvas = { width: 0, height: 0, getContext: () => context, toDataURL: toDataUrl };
    canvases.push(canvas);
    return canvas;
  }) as never);
  return { context, canvases };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  FakeImage.width = 1200;
  FakeImage.height = 600;
});

describe("context image rendering", () => {
  it("measures decoded base64 sizes and builds preview URLs", () => {
    expect(base64ByteLength("QUJD")).toBe(3);
    expect(base64ByteLength("QUI=")).toBe(2);
    expect(base64ByteLength("QQ==")).toBe(1);
    expect(base64DataUrl("QUJD", "image/png")).toBe("data:image/png;base64,QUJD");
    expect(base64DataUrl("data:image/jpeg;base64,QUJD", "image/png")).toBe("data:image/jpeg;base64,QUJD");
  });

  it("cover-crops the centre square into an exact PNG", async () => {
    vi.stubGlobal("Image", FakeImage);
    const { context, canvases } = fakeCanvas(() => "data:image/png;base64,UE5H");

    await expect(renderCoverPng("data:image/png;base64,AAAA", 600)).resolves.toBe("UE5H");

    expect(canvases[0]).toMatchObject({ width: 600, height: 600 });
    expect(context.drawImage).toHaveBeenCalledWith(expect.any(FakeImage), 300, 0, 600, 600, 0, 0, 600, 600);
  });

  it("downscales references and lowers JPEG quality until the size fits", async () => {
    FakeImage.width = 3200;
    FakeImage.height = 1600;
    vi.stubGlobal("Image", FakeImage);
    const { canvases } = fakeCanvas((_type, quality = 1) => `data:image/jpeg;base64,${"A".repeat(quality > 0.7 ? 4000 : 400)}`);

    await expect(renderReferenceJpeg("data:image/png;base64,AAAA", 1600, 1000)).resolves.toBe("A".repeat(400));

    expect(canvases[0]).toMatchObject({ width: 1600, height: 800 });
  });

  it("refuses an encoding the browser cannot produce", async () => {
    vi.stubGlobal("Image", FakeImage);
    fakeCanvas(() => "data:image/png;base64,UE5H");

    await expect(renderReferenceJpeg("data:image/webp;base64,AAAA", 1600, 1000)).rejects.toThrow(/could not be encoded/);
  });
});
