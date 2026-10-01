Build a marketing website for Ubra, an open-source agent-runtime desktop
app, at getubra.com. MIT-licensed, free forever.

THEME (sampled from the attached logo.png — follow it closely):
- Near-black background surfaces; the logo is a pixel/blocky "UBRA"
  wordmark with a horizontal rainbow gradient: cyan → blue → violet →
  magenta → red-orange → orange → yellow.
- Use that exact gradient for the hero headline (gradient text), primary
  CTA buttons, section dividers, badges, card borders, and link hovers.
- Body copy stays high-contrast neutral (white/zinc) for readability;
  rainbow is for accents, never body text. Subtle retro-pixel motif
  (pixel dividers, pixel-cursor details) as a secondary brand element.
- Overall feel: playful-retro meets pro dev-tool. Dark only.

STRUCTURE INSPIRATION: herdr.dev skeleton (top nav, Install CTA, bold
hero, stats band, product mock, footer) — but with Ubra's rainbow-on-
black identity, not their ink/paper look.

PRODUCT FACTS (use exactly; never invent stats, testimonials, or logos):
- Tagline: "An agent runtime as a cross-platform desktop app."
- What it is: workspaces → tabs → terminal panes running real coding-agent
  CLIs (claude, codex, opencode, …) with agent state badges, layout
  persistence, tray behavior, Explorer + Source Control sidebar, and plan
  usage windows for Codex/Claude. Built with Tauri v2 (Rust), Svelte 5,
  xterm.js. Platforms: macOS, Linux, Windows.
- Install: download bundles from https://github.com/Ubra-Dev/ubra/releases
  (show macOS / Linux / Windows cards). Source + docs/README at
  https://github.com/Ubra-Dev/ubra. License: MIT © 2026 Ubra.
- Live GitHub star count via shields badge (no hardcoded numbers).
- Logo: attached logo.png (use in nav + footer + hero). Screenshots:
  attached app screenshots for the hero product mock.

PAGES & ROUTES:
1. `/` Homepage: nav (Docs, Blog, Compare, Sponsors, GitHub icon, Download
   button) → hero headline with rainbow gradient text positioning Ubra as
   "the open-source desktop home for your coding agents" + Download CTA +
   GitHub stars → platform/license strip → stats band (live stars,
   platforms, license) → feature grid (agent awareness/badges,
   multiplexer, layout restore, tray, Explorer/Source Control, plan
   usage) → product screenshot section → "Open source alternative to
   Herdr" teaser linking /compare → sponsor band (tier cards + Sponsor
   buttons) → footer.
2. `/download`: OS cards linking to GitHub Releases, build-from-source
   snippet (npm ci + npm run tauri build), system requirements.
3. `/compare`: honest Ubra vs Herdr table. Herdr strengths: large community,
   remote/SSH sessions, plugin ecosystem. Ubra strengths: native desktop
   GUI, MIT open source, free. Tone: factual alternative, never negative.
4. `/docs`: docs hub with sidebar (Install, Workspaces & panes, Agent
   awareness, Layout & tray, Usage, Headless daemon/CLI, Telemetry).
   Seed with concise accurate pages from the facts above; mark each page
   "Docs improve with the project — PRs welcome" linking the repo.
5. `/blog`: changelog-style blog with one launch post ("Introducing Ubra")
   drafted from the facts above.
6. `/sponsors`: why sponsor (signed builds, cross-OS release testing,
   maintainer time), tier cards, FAQ. Buttons: [PLACEHOLDER: GitHub
   Sponsors URL] and [PLACEHOLDER: Open Collective URL], rendered as
   visible "coming soon" badges until URLs are provided.
7. `/privacy`, `/terms`, `/contact`: full legal-page layouts with sensible
   OSS-project starter copy (telemetry is opt-in, no account data sold).
   Contact: hello@getubra.com [PLACEHOLDER — verify], GitHub issues link,
   [PLACEHOLDER: X handle], [PLACEHOLDER: Discord invite].

GLOBAL: sticky nav, footer (© 2026 Ubra, MIT License, Privacy/Terms/
Contact links, GitHub icon, version placeholder), mobile-responsive,
SEO titles/meta per page, semantic HTML, accessible contrast.
