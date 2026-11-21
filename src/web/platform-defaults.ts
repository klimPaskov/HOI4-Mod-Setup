import type { Platform } from "./protocol";

/**
 * Components the setup package offers on every computer but that only help on
 * Linux. A new setup ticks them on Linux and leaves them to the user elsewhere.
 */
export const LINUX_ONLY_DEFAULT_COMPONENT_IDS: readonly string[] = ["docs.linux"];

/** The optional components ticked when a setup starts: the assistant's recommendations plus this computer's defaults. */
export function seededOptionalIds(
  optional: ReadonlyArray<{ id: string }>,
  recommendedIds: ReadonlySet<string>,
  desktopOnly: ReadonlySet<string>,
  platform: Platform,
): Set<string> {
  return new Set(
    optional
      .filter((component) =>
        !desktopOnly.has(component.id)
        && (recommendedIds.has(component.id) || (platform === "linux" && LINUX_ONLY_DEFAULT_COMPONENT_IDS.includes(component.id))))
      .map((component) => component.id),
  );
}
