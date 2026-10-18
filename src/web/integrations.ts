// Pure rules for the web edition's integration choices, matching the core's
// validation so the page never offers a choice the core will refuse.

import type { ImageChoice, ImageProvider, LiveTestingChoice, PortraitChoice, SourcePreview } from "./protocol";

export const NO_IMAGE_PROVIDER = "none";
export const MAX_ADDITIONAL_IMAGE_PROVIDERS = 8;

/** Providers a selected client can use. Manual providers work with every client. */
export function availableImageProviders(preview: SourcePreview | null, clients: string[], primary: string): ImageProvider[] {
  return (preview?.image_generation?.providers ?? []).filter((provider) => {
    if (provider.requires_primary_coding_environment && provider.requires_primary_coding_environment !== primary) return false;
    if (provider.route_kind === "manual") return true;
    return (provider.supported_coding_environments ?? []).some((id) => clients.includes(id));
  });
}

/** The settings a provider's route accepts, as the core allows them. */
export function imageSettingKeys(provider: ImageProvider): string[] {
  switch (provider.api_protocol) {
    case "comfyui":
    case "automatic1111":
      return ["server_url", "checkpoint"];
    case "microsoft_foundry":
      return ["deployment_name"];
    case "aws_bedrock":
      return ["region", "model"];
    default:
      return (provider.supported_models ?? []).length > 0 ? ["model"] : [];
  }
}

export function imageSettingLabel(key: string): string {
  return ({ server_url: "Local server address", checkpoint: "Checkpoint", deployment_name: "Deployment name", region: "Region", model: "Model" } as Record<string, string>)[key] ?? key;
}

/** A local image server must run on this computer. */
export function isLoopbackUrl(value: string): boolean {
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname) && !url.username && !url.password;
  } catch {
    return false;
  }
}

export function defaultImageChoice(preview: SourcePreview | null): ImageChoice {
  return {
    primary_provider: preview?.image_generation?.default_provider ?? NO_IMAGE_PROVIDER,
    additional_providers: [],
    route_settings: {},
  };
}

/** Keep only choices the current clients can use; fall back to none. */
export function normalizeImageChoice(choice: ImageChoice, available: ImageProvider[]): ImageChoice {
  const ids = new Set(available.map((provider) => provider.id));
  const primary = ids.has(choice.primary_provider) ? choice.primary_provider : ids.has(NO_IMAGE_PROVIDER) ? NO_IMAGE_PROVIDER : available[0]?.id ?? NO_IMAGE_PROVIDER;
  const additional = primary === NO_IMAGE_PROVIDER
    ? []
    : choice.additional_providers.filter((id) => id !== primary && ids.has(id) && id !== NO_IMAGE_PROVIDER).slice(0, MAX_ADDITIONAL_IMAGE_PROVIDERS);
  const keep = new Set([primary, ...additional]);
  const route_settings = Object.fromEntries(Object.entries(choice.route_settings).filter(([id]) => keep.has(id)));
  return { primary_provider: primary, additional_providers: additional, route_settings };
}

/** What a provider needs before images can be made, in plain words. */
export function imageProviderNeeds(provider: ImageProvider): string | null {
  switch (provider.auth_method) {
    case "api_key_vault":
    case "cloud_credentials":
    case "enterprise_credentials":
      return "Its key is read from the operating-system credential store. A browser cannot save it there; add it later with the desktop app.";
    case "chatgpt_subscription":
      return "It uses your ChatGPT sign-in in the provider's own tool.";
    case "local":
      return "Its server must run on this computer.";
    default:
      return null;
  }
}

export const NO_LIVE_TESTING: LiveTestingChoice = { provider: "none", linux_ready: false, wsl_distribution: "", ssh_host: "" };

const SAFE_NAME = /^[A-Za-z0-9][A-Za-z0-9_.-]*$/;

/** The same rules as the core: safe names, and an SSH alias on macOS. */
export function liveTestingProblem(choice: LiveTestingChoice, platform: string): string | null {
  if (choice.provider !== "clausewitz_mcp") return null;
  if (choice.wsl_distribution && (choice.wsl_distribution.length > 64 || !SAFE_NAME.test(choice.wsl_distribution))) return "Enter a WSL distribution name with letters, digits, dots, dashes, or underscores.";
  if (choice.ssh_host && (choice.ssh_host.length > 128 || !SAFE_NAME.test(choice.ssh_host))) return "Enter an SSH host alias from your SSH config, without a user name.";
  if (choice.linux_ready && platform === "macos" && !choice.ssh_host) return "On macOS, enter the SSH host alias of the Linux computer.";
  return null;
}

export const NO_PORTRAITS: PortraitChoice = { provider: "disabled" };

export function portraitProblem(choice: PortraitChoice): string | null {
  if (choice.provider === "local" && choice.localServerUrl && !isLoopbackUrl(choice.localServerUrl)) return "A local ComfyUI server must use a loopback address such as http://127.0.0.1:8188.";
  if (choice.provider === "runpod" && choice.runpodUrl) {
    try {
      const url = new URL(choice.runpodUrl);
      if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash) return "Enter the RunPod address as HTTPS without a user name or query.";
    } catch {
      return "Enter the RunPod address as HTTPS without a user name or query.";
    }
  }
  return null;
}
