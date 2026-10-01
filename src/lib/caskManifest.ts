/**
 * Homebrew cask renderer for the Ubra-Dev/homebrew-tap repo. Pure logic;
 * scripts/update-cask.mjs is the thin IO wrapper that feeds it release DMGs.
 *
 * Fixed fields mirror the repo's own metadata: productName + identifier from
 * src-tauri/tauri.conf.json, description + homepage from package.json, and
 * the minimum macOS version (10.15 = Catalina) from the bundle config. The
 * LaunchAgent label/file come from the autostart chain (productName via
 * package_info().name into auto-launch's {app_name}.plist); app-data paths
 * follow Tauri's identifier-suffixed directories. tests/caskManifest.test.ts
 * guards these against config drift by rendering from the real files.
 */

export type DmgArch = "arm" | "intel";

export interface CaskDmg {
  arch: DmgArch;
  file: string;
  sha256: string;
}

export interface CaskInput {
  /** Bare version, e.g. "0.1.0". */
  version: string;
  /** Release tag, e.g. "v0.1.0". */
  tag: string;
  /** "owner/name" of the release repo. */
  repo: string;
  productName: string;
  identifier: string;
  description: string;
  homepage: string;
  /** LaunchAgent app name (productName by default). */
  launchAgentName: string;
  arm: { file: string; sha256: string };
  intel: { file: string; sha256: string };
}

/**
 * Classify a Tauri DMG name (`Ubra_0.1.0_aarch64.dmg`) by its explicit arch
 * suffix. Throws on anything unrecognized rather than guessing.
 */
export function classifyDmgArch(fileName: string): DmgArch {
  const lower = fileName.toLowerCase();
  if (/_aarch64\.dmg$/.test(lower) || /_arm64\.dmg$/.test(lower)) return "arm";
  if (
    /_x86_64\.dmg$/.test(lower) ||
    /_x64\.dmg$/.test(lower) ||
    /_intel\.dmg$/.test(lower)
  ) {
    return "intel";
  }
  throw new Error(`unrecognized DMG arch token in file name: ${fileName}`);
}

/** The cask needs exactly one DMG per Mac arch; anything else fails closed. */
export function assertArchPair(dmgs: CaskDmg[]): void {
  const kinds = dmgs.map((dmg) => dmg.arch).sort();
  if (kinds.join(",") !== "arm,intel") {
    throw new Error(
      `cask needs one arm + one intel DMG, got: ${kinds.join(", ") || "none"}`,
    );
  }
}

function dmgUrl(input: CaskInput, file: string): string {
  return `https://github.com/${input.repo}/releases/download/${input.tag}/${encodeURIComponent(file)}`;
}

/** Render the complete `Casks/ubra.rb` file content. */
export function buildCaskRb(input: CaskInput): string {
  const id = input.identifier;
  const agent = input.launchAgentName;
  return `cask "ubra" do
  version "${input.version}"

  on_arm do
    sha256 "${input.arm.sha256}"
    url "${dmgUrl(input, input.arm.file)}"
  end
  on_intel do
    sha256 "${input.intel.sha256}"
    url "${dmgUrl(input, input.intel.file)}"
  end

  name "${input.productName}"
  desc "${input.description}"
  homepage "${input.homepage}"

  auto_updates true
  depends_on macos: ">= :catalina"

  app "${input.productName}.app"

  uninstall quit:      "${id}",
            launchctl: "${agent}"

  zap trash: [
    "~/Library/Application Support/${id}",
    "~/Library/Caches/${id}",
    "~/Library/LaunchAgents/${agent}.plist",
    "~/Library/Preferences/${id}.plist",
    "~/Library/Saved Application State/${id}.savedState",
  ]
end
`;
}
