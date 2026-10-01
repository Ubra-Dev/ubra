<script lang="ts">
  import {
    DEFAULT_LOADING_TEXT,
    brandGradientImage,
  } from "./loadingGradient";
  import { UI_FONTS } from "./uiFonts";

  interface Props {
    /** Text to render; defaults to "Loading...". */
    text?: string;
    /** Font size in pixels. */
    size?: number;
    /** Full gradient sweep in milliseconds. */
    durationMs?: number;
  }
  let {
    text = DEFAULT_LOADING_TEXT,
    size = 20,
    durationMs = 3000,
  }: Props = $props();
</script>

<span
  class="loading-text"
  role="status"
  style={`font-family:${UI_FONTS.silkscreen.stack};font-size:${size}px;--brand-gradient:${brandGradientImage()};animation-duration:${durationMs}ms`}
>{text}</span
>

<style>
  .loading-text {
    display: inline-block;
    font-weight: 700;
    background-image: var(--brand-gradient);
    background-size: 250% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    animation-name: loading-sweep;
    animation-timing-function: ease-in-out;
    animation-iteration-count: infinite;
  }
  @keyframes loading-sweep {
    0% {
      background-position: 0% 50%;
    }
    50% {
      background-position: 100% 50%;
    }
    100% {
      background-position: 0% 50%;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .loading-text {
      animation: none;
    }
  }
  /* Solid brand magenta where gradient-clipped text is unsupported. */
  @supports not (background-clip: text) {
    .loading-text {
      background-image: none;
      color: #ff00ff;
    }
  }
</style>
