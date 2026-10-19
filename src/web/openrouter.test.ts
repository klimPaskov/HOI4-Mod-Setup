import { afterEach, describe, expect, it, vi } from "vitest";
import { callbackUrl, KEY_EXCHANGE_URL, signInWithOpenRouter, waitForCode } from "./openrouter";
import { SIGN_IN_CHANNEL, type SignInMessage } from "./openrouter-channel";
import type { CoreClient } from "./core-client";
import type { Operation } from "./protocol";

/** A same-origin channel shared by every instance with the same name. */
class FakeChannel {
  static open: FakeChannel[] = [];
  onmessage: ((event: MessageEvent<SignInMessage>) => void) | null = null;
  constructor(readonly name: string) {
    FakeChannel.open.push(this);
  }
  postMessage(data: SignInMessage) {
    for (const channel of FakeChannel.open) {
      if (channel !== this && channel.name === this.name) channel.onmessage?.({ data } as MessageEvent<SignInMessage>);
    }
  }
  close() {
    FakeChannel.open = FakeChannel.open.filter((channel) => channel !== this);
  }
}

function deliver(message: SignInMessage) {
  new FakeChannel(SIGN_IN_CHANNEL).postMessage(message);
}

function core() {
  const calls: Array<{ op: Operation; args: unknown }> = [];
  const client: CoreClient = {
    call: vi.fn(async (op: Operation, args: unknown) => {
      calls.push({ op, args });
      if (op === "openRouterStart") return { url: "https://openrouter.ai/auth?state=s1", verifier: "verifier-1", state: "s1" };
      if (op === "openRouterKeyRequest") return JSON.stringify({ code: (args as { code: string }).code, code_verifier: "verifier-1", code_challenge_method: "S256" });
      if (op === "openRouterKey") return JSON.parse((args as { response: string }).response).key;
      throw new Error(`unexpected ${op}`);
    }) as unknown as CoreClient["call"],
    onProgress: () => () => undefined,
    terminate: () => undefined,
  };
  return { client, calls };
}

afterEach(() => {
  vi.unstubAllGlobals();
  FakeChannel.open = [];
});

describe("OpenRouter sign-in", () => {
  it("uses the callback page beside the app without its query", () => {
    expect(callbackUrl("https://klimpaskov.github.io/HOI4-Mod-Setup/app/?x=1#top")).toBe("https://klimpaskov.github.io/HOI4-Mod-Setup/app/openrouter-callback.html");
    expect(callbackUrl("http://localhost:1430/")).toBe("http://localhost:1430/openrouter-callback.html");
  });

  it("accepts only the code for this sign-in's state", async () => {
    vi.stubGlobal("BroadcastChannel", FakeChannel);
    const waiting = waitForCode("s1", new AbortController().signal);
    deliver({ type: "openrouter-sign-in", code: "other", state: "s2" });
    deliver({ type: "openrouter-sign-in", code: "abc", state: "s1" });
    await expect(waiting).resolves.toBe("abc");
    expect(FakeChannel.open).toHaveLength(0);
  });

  it("stops on a declined sign-in, a cancel, or the timeout", async () => {
    vi.stubGlobal("BroadcastChannel", FakeChannel);
    const declined = waitForCode("s1", new AbortController().signal);
    deliver({ type: "openrouter-sign-in", code: null, state: null });
    await expect(declined).rejects.toThrow(/not approved/);
    const controller = new AbortController();
    const cancelled = waitForCode("s1", controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toThrow(/cancelled/);
    await expect(waitForCode("s1", new AbortController().signal, 10)).rejects.toThrow(/timed out/);
    expect(FakeChannel.open).toHaveLength(0);
  });

  it("exchanges the code for the user's key without sending the key anywhere else", async () => {
    vi.stubGlobal("BroadcastChannel", FakeChannel);
    const key = ["sk", "or", "v1", "user"].join("-");
    const fetchMock = vi.fn().mockResolvedValue(new Response(JSON.stringify({ key }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const open = vi.fn(() => {
      queueMicrotask(() => deliver({ type: "openrouter-sign-in", code: "abc", state: "s1" }));
      return {} as Window;
    });
    const { client, calls } = core();
    await expect(signInWithOpenRouter(client, new AbortController().signal, open)).resolves.toBe(key);
    expect(open).toHaveBeenCalledWith("https://openrouter.ai/auth?state=s1", "openrouter-sign-in", expect.stringContaining("popup"));
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe(KEY_EXCHANGE_URL);
    expect(init.credentials).toBe("omit");
    expect(init.redirect).toBe("error");
    expect(JSON.parse(init.body)).toMatchObject({ code: "abc", code_verifier: "verifier-1" });
    expect(calls.map((call) => call.op)).toEqual(["openRouterStart", "openRouterKeyRequest", "openRouterKey"]);
  });

  it("explains a blocked popup and a refused exchange", async () => {
    vi.stubGlobal("BroadcastChannel", FakeChannel);
    await expect(signInWithOpenRouter(core().client, new AbortController().signal, () => null)).rejects.toThrow(/Allow pop-ups/);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("{}", { status: 403 })));
    const open = vi.fn(() => {
      queueMicrotask(() => deliver({ type: "openrouter-sign-in", code: "abc", state: "s1" }));
      return {} as Window;
    });
    await expect(signInWithOpenRouter(core().client, new AbortController().signal, open)).rejects.toThrow(/HTTP 403/);
  });
});
