// Pure rules for the web edition's integration choices, matching the core's
// validation so the page never offers a choice the core will refuse.

import { imageSettingLabel, imageSettingProblem, isLoopbackUrl } from "../lib/imageSettings";
import type { ImageChoice, ImageProvider, LiveTestingChoice, PortraitChoice, SourcePreview } from "./protocol";

export { imageSettingLabel, imageSettingProblem, isLoopbackUrl };

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

/** A new mod starts on hoi4-imagegen, or on none when the package does not offer it; an older provider is only ever one the project already records. */
export function defaultImageChoice(preview: SourcePreview | null): ImageChoice {
  const catalog = preview?.image_generation;
  const imagegen = catalog?.providers.find((provider) => provider.route_kind === "release_binary");
  const named = catalog?.providers.find((provider) => provider.id === catalog.default_provider);
  const usable = named && (named.route_kind === "release_binary" || named.id === NO_IMAGE_PROVIDER) ? named.id : imagegen?.id;
  return {
    primary_provider: usable ?? NO_IMAGE_PROVIDER,
    additional_providers: [],
    route_settings: {},
  };
}

/** Keep only choices the current clients can use; fall back to none. */
export function normalizeImageChoice(choice: ImageChoice, available: ImageProvider[]): ImageChoice {
  // The retired GPTImage route (an older package still lists it) is replaced by hoi4-imagegen's ChatGPT sign-in, or by none when the package lacks it, as the desktop does.
  const retired = new Set(available.filter((provider) => provider.route_kind === "local_package").map((provider) => provider.id));
  const replacement = available.find((provider) => provider.route_kind === "release_binary")?.id ?? NO_IMAGE_PROVIDER;
  const ids = new Set(available.filter((provider) => !retired.has(provider.id)).map((provider) => provider.id));
  const wanted = retired.has(choice.primary_provider) ? replacement : choice.primary_provider;
  const primary = ids.has(wanted) ? wanted : ids.has(NO_IMAGE_PROVIDER) ? NO_IMAGE_PROVIDER : [...ids][0] ?? NO_IMAGE_PROVIDER;
  const additional = primary === NO_IMAGE_PROVIDER
    ? []
    : choice.additional_providers.filter((id) => id !== primary && ids.has(id) && id !== NO_IMAGE_PROVIDER).slice(0, MAX_ADDITIONAL_IMAGE_PROVIDERS);
  const keep = new Set([primary, ...additional]);
  const route_settings = Object.fromEntries(Object.entries(choice.route_settings).filter(([id]) => keep.has(id)));
  return { primary_provider: primary, additional_providers: additional, route_settings };
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

/** The first problem in the chosen backend's settings, if any. */
export function imageChoiceProblem(choice: ImageChoice, providers: ImageProvider[]): string | null {
  const row = providers.find((provider) => provider.route_kind === "release_binary");
  if (!row || choice.primary_provider !== row.id) return null;
  const backends = row.backends ?? [];
  const backend = backends.find((option) => option.id === choice.route_settings[row.id]?.backend) ?? backends.find((option) => option.default) ?? backends[0];
  for (const setting of backend?.settings ?? []) {
    const problem = imageSettingProblem(setting.kind, choice.route_settings[row.id]?.[setting.key] ?? "");
    if (problem) return `${imageSettingLabel(setting.key)}: ${problem}`;
  }
  return null;
}
