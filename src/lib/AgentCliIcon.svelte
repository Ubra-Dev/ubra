<script lang="ts">
  import { cliBrand, glyphColor } from "./agentCliIcons";

  interface Props {
    /** Agent CLI id (e.g. "claude"); matching is case-insensitive. */
    cli: string;
    /** Display label; seeds the monogram fallback when no logo exists. */
    label?: string;
    /** Glyph edge length in pixels. */
    size?: number;
  }
  let { cli, label, size = 16 }: Props = $props();

  const brand = $derived(cliBrand(cli, label));
  const paint = $derived(
    brand.kind === "logo" ? glyphColor(brand.hex) : glyphColor(brand.color),
  );
</script>

{#if brand.kind === "logo"}
  <svg
    viewBox="0 0 24 24"
    width={size}
    height={size}
    fill={paint}
    aria-hidden="true"
    class="mark"
  >
    <path d={brand.path} />
  </svg>
{:else}
  <span
    class="mono"
    aria-hidden="true"
    style:color={paint}
    style:font-size={Math.round(size * 0.8) + "px"}
  >
    {brand.letter}
  </span>
{/if}

<style>
  .mark {
    flex: 0 0 auto;
    vertical-align: middle;
  }
  .mono {
    flex: 0 0 auto;
    font-weight: 800;
    font-family: var(--font-ui);
    line-height: 1;
    vertical-align: middle;
  }
</style>
