/** Static updater metadata (`latest.json`) for GitHub Releases distribution. */

export interface UpdaterBundle {
  /** Platform key in `OS-ARCH` format, e.g. "darwin-aarch64". */
  platform: string;
  /** Download URL of the update bundle. */
  url: string;
  /** Contents of the generated `.sig` file (never a path). */
  signature: string;
}

/** Every platform key a release must cover before publication. */
export const REQUIRED_PLATFORMS = [
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
  "windows-x86_64",
] as const;

/**
 * Pair updater bundles with platform keys. A universal macOS bundle covers
 * both darwin keys; on Windows NSIS wins over MSI when both were built.
 * Throws when a bundle has no `.sig` sibling.
 */
export function collectUpdaterBundles(
  names: string[],
  readSig: (sigName: string) => string,
  urlFor: (bundleName: string) => string,
): UpdaterBundle[] {
  const present = new Set(names);
  const bundles: UpdaterBundle[] = [];
  const seen = new Set<string>();
  const take = (bundleName: string, platforms: string[]): void => {
    if (seen.has(bundleName)) {
      throw new Error(
        `duplicate updater bundle name: ${bundleName} (release assets share one flat namespace)`,
      );
    }
    seen.add(bundleName);
    let signature: string;
    try {
      signature = readSig(`${bundleName}.sig`).trim();
    } catch {
      throw new Error(
        `updater bundle ${bundleName} has no signature file (${bundleName}.sig)`,
      );
    }
    if (!signature) {
      throw new Error(`updater signature is empty: ${bundleName}.sig`);
    }
    for (const platform of platforms) {
      bundles.push({ platform, url: urlFor(bundleName), signature });
    }
  };

  for (const name of names) {
    if (!name.endsWith(".app.tar.gz")) continue;
    if (name.includes("_universal.")) {
      take(name, ["darwin-aarch64", "darwin-x86_64"]);
    } else if (name.includes("_aarch64.")) {
      take(name, ["darwin-aarch64"]);
    } else if (name.includes("_x86_64.")) {
      take(name, ["darwin-x86_64"]);
    }
  }

  const nsis = names.find((n) => n.endsWith(".nsis.zip"));
  const msi = names.find((n) => n.endsWith(".msi.zip"));
  const windows = nsis ?? msi;
  if (windows && present.has(windows)) take(windows, ["windows-x86_64"]);

  for (const name of names) {
    if (name.endsWith(".AppImage.tar.gz")) take(name, ["linux-x86_64"]);
  }

  return bundles;
}

/** Fail closed when any required platform key is missing. */
export function assertCompletePlatforms(bundles: UpdaterBundle[]): void {
  const have = new Set(bundles.map((b) => b.platform));
  const missing = REQUIRED_PLATFORMS.filter((p) => !have.has(p));
  if (missing.length > 0) {
    throw new Error(
      `updater metadata is missing platforms: ${missing.join(", ")}`,
    );
  }
}

export function buildLatestJson(args: {
  version: string;
  pubDate: string;
  notes?: string;
  bundles: UpdaterBundle[];
}): string {
  const platforms: Record<string, { url: string; signature: string }> = {};
  for (const b of args.bundles) {
    platforms[b.platform] = { url: b.url, signature: b.signature };
  }
  return (
    JSON.stringify({
      version: args.version,
      pub_date: args.pubDate,
      ...(args.notes ? { notes: args.notes } : {}),
      platforms,
    }) + "\n"
  );
}
