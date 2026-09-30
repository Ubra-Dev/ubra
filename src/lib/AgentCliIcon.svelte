<script lang="ts">
  import { cliBrand } from "./agentCliIcons";

  interface Props {
    /** Agent CLI id (e.g. "claude"); matching is case-insensitive. */
    cli: string;
    /** Display label; seeds the monogram fallback when no logo exists. */
    label?: string;
    /** Tile edge length in pixels. */
    size?: number;
  }
  let { cli, label, size = 16 }: Props = $props();

  const logo = $derived.by(() => {
    const brand = cliBrand(cli, label);
    return brand.kind === "logo" ? brand : null;
  });
  const mono = $derived.by(() => {
    const brand = cliBrand(cli, label);
    return brand.kind === "mono" ? brand : null;
  });
  const radius = $derived(Math.max(2, Math.round(size / 4)));
</script>

{#if logo}
  <span
    class="tile logo"
    aria-hidden="true"
    style:width={size + "px"}
    style:height={size + "px"}
    style:border-radius={radius + "px"}
  >
    <svg
      viewBox="0 0 24 24"
      width={Math.round(size * 0.66)}
      height={Math.round(size * 0.66)}
      fill={"#" + logo.hex}
    >
      <path d={logo.path} />
    </svg>
  </span>
{:else if mono}
  <span
    class="tile mono"
    aria-hidden="true"
    style:width={size + "px"}
    style:height={size + "px"}
    style:border-radius={radius + "px"}
    style:background={mono.bg}
    style:font-size={Math.round(size * 0.62) + "px"}
  >
    {mono.letter}
  </span>
{/if}

<style>
  .tile {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    vertical-align: middle;
  }
  .tile.logo {
    box-sizing: border-box;
    background: #ffffff;
    border: 1px solid var(--border);
  }
  .tile.mono {
    color: #ffffff;
    font-weight: 700;
    font-family: var(--font-ui);
    line-height: 1;
  }
</style>
