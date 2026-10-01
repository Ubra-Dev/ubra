<script lang="ts">
  export type IconName =
    | "layers"
    | "cpu"
    | "settings"
    | "sliders"
    | "palette"
    | "bell"
    | "volume"
    | "activity"
    | "command"
    | "info"
    | "plus"
    | "minus"
    | "x"
    | "check"
    | "edit"
    | "search"
    | "maximize"
    | "minimize"
    | "refresh"
    | "alert"
    | "play"
    | "columns"
    | "grid"
    | "rows"
    | "external"
    | "eye"
    | "chevron-up"
    | "chevron-down"
    | "chevron-right"
    | "folder"
    | "file"
    | "file-text"
    | "code"
    | "image"
    | "music"
    | "video"
    | "archive"
    | "database"
    | "globe"
    | "package"
    | "terminal"
    | "lock"
    | "key"
    | "book"
    | "tool"
    | "type"
    | "git-branch"
    | "download"
    | "upload"
    | "sidebar-left"
    | "sidebar-right";

  interface Props {
    name: IconName;
    size?: number;
  }
  let { name, size = 14 }: Props = $props();

  // Feather-style stroke icons (24x24, stroke=currentColor).
  const PATHS: Record<IconName, string> = {
    layers:
      '<polygon points="12 2 2 7 12 12 22 7 12 2"/>' +
      '<polyline points="2 17 12 22 22 17"/>' +
      '<polyline points="2 12 12 17 22 12"/>',
    cpu:
      '<rect x="4" y="4" width="16" height="16" rx="2" ry="2"/>' +
      '<rect x="9" y="9" width="6" height="6"/>' +
      '<line x1="9" y1="1" x2="9" y2="4"/>' +
      '<line x1="15" y1="1" x2="15" y2="4"/>' +
      '<line x1="9" y1="20" x2="9" y2="23"/>' +
      '<line x1="15" y1="20" x2="15" y2="23"/>' +
      '<line x1="20" y1="9" x2="23" y2="9"/>' +
      '<line x1="20" y1="14" x2="23" y2="14"/>' +
      '<line x1="1" y1="9" x2="4" y2="9"/>' +
      '<line x1="1" y1="14" x2="4" y2="14"/>',
    settings:
      '<circle cx="12" cy="12" r="3"/>' +
      '<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
    sliders:
      '<line x1="4" y1="21" x2="4" y2="14"/>' +
      '<line x1="4" y1="10" x2="4" y2="3"/>' +
      '<line x1="12" y1="21" x2="12" y2="12"/>' +
      '<line x1="12" y1="8" x2="12" y2="3"/>' +
      '<line x1="20" y1="21" x2="20" y2="16"/>' +
      '<line x1="20" y1="12" x2="20" y2="3"/>' +
      '<line x1="1" y1="14" x2="7" y2="14"/>' +
      '<line x1="9" y1="8" x2="15" y2="8"/>' +
      '<line x1="17" y1="16" x2="23" y2="16"/>',
    palette:
      '<circle cx="13.5" cy="6.5" r=".5"/>' +
      '<circle cx="17.5" cy="10.5" r=".5"/>' +
      '<circle cx="8.5" cy="7.5" r=".5"/>' +
      '<circle cx="6.5" cy="12.5" r=".5"/>' +
      '<path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.926 0 1.648-.746 1.648-1.688 0-.437-.18-.835-.437-1.125-.29-.289-.438-.652-.438-1.125a1.64 1.64 0 0 1 1.668-1.668h2.996A5 5 0 0 0 22 11.5C21.5 6.158 17.5 2 12 2z"/>',
    bell:
      '<path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/>' +
      '<path d="M13.73 21a2 2 0 0 1-3.46 0"/>',
    volume:
      '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>' +
      '<path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"/>',
    activity: '<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>',
    command:
      '<path d="M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z"/>',
    info:
      '<circle cx="12" cy="12" r="10"/>' +
      '<line x1="12" y1="16" x2="12" y2="12"/>' +
      '<line x1="12" y1="8" x2="12.01" y2="8"/>',
    plus:
      '<line x1="12" y1="5" x2="12" y2="19"/>' +
      '<line x1="5" y1="12" x2="19" y2="12"/>',
    minus: '<line x1="5" y1="12" x2="19" y2="12"/>',
    x:
      '<line x1="18" y1="6" x2="6" y2="18"/>' +
      '<line x1="6" y1="6" x2="18" y2="18"/>',
    check: '<polyline points="20 6 9 17 4 12"/>',
    edit: '<path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/>',
    search:
      '<circle cx="11" cy="11" r="8"/>' +
      '<line x1="21" y1="21" x2="16.65" y2="16.65"/>',
    maximize:
      '<polyline points="15 3 21 3 21 9"/>' +
      '<polyline points="9 21 3 21 3 15"/>' +
      '<line x1="21" y1="3" x2="14" y2="10"/>' +
      '<line x1="3" y1="21" x2="10" y2="14"/>',
    minimize:
      '<polyline points="4 14 10 14 10 20"/>' +
      '<polyline points="20 10 14 10 14 4"/>' +
      '<line x1="10" y1="14" x2="3" y2="21"/>' +
      '<line x1="14" y1="10" x2="21" y2="3"/>',
    refresh:
      '<polyline points="23 4 23 10 17 10"/>' +
      '<polyline points="1 20 1 14 7 14"/>' +
      '<path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>',
    alert:
      '<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>' +
      '<line x1="12" y1="9" x2="12" y2="13"/>' +
      '<line x1="12" y1="17" x2="12.01" y2="17"/>',
    play: '<polygon points="5 3 19 12 5 21 5 3"/>',
    columns:
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<line x1="12" y1="3" x2="12" y2="21"/>',
    grid:
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<line x1="12" y1="3" x2="12" y2="21"/>' +
      '<line x1="3" y1="12" x2="21" y2="12"/>',
    rows:
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<line x1="3" y1="12" x2="21" y2="12"/>',
    external:
      '<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>' +
      '<polyline points="15 3 21 3 21 9"/>' +
      '<line x1="10" y1="14" x2="21" y2="3"/>',
    eye:
      '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>' +
      '<circle cx="12" cy="12" r="3"/>',
    "chevron-up": '<polyline points="18 15 12 9 6 15"/>',
    "chevron-down": '<polyline points="6 9 12 15 18 9"/>',
    "chevron-right": '<polyline points="9 18 15 12 9 6"/>',
    folder:
      '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>',
    file:
      '<path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/>' +
      '<polyline points="13 2 13 9 20 9"/>',
    "file-text":
      '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>' +
      '<polyline points="14 2 14 8 20 8"/>' +
      '<line x1="16" y1="13" x2="8" y2="13"/>' +
      '<line x1="16" y1="17" x2="8" y2="17"/>' +
      '<polyline points="10 9 9 9 8 9"/>',
    code:
      '<polyline points="16 18 22 12 16 6"/>' +
      '<polyline points="8 6 2 12 8 18"/>',
    image:
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<circle cx="8.5" cy="8.5" r="1.5"/>' +
      '<polyline points="21 15 16 10 5 21"/>',
    music:
      '<path d="M9 18V5l12-2v13"/>' +
      '<circle cx="6" cy="18" r="3"/>' +
      '<circle cx="18" cy="16" r="3"/>',
    video:
      '<polygon points="23 7 16 12 23 17 23 7"/>' +
      '<rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>',
    archive:
      '<polyline points="21 8 21 21 3 21 3 8"/>' +
      '<rect x="1" y="3" width="22" height="5"/>' +
      '<line x1="10" y1="12" x2="14" y2="12"/>',
    database:
      '<ellipse cx="12" cy="5" rx="9" ry="3"/>' +
      '<path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/>' +
      '<path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>',
    globe:
      '<circle cx="12" cy="12" r="10"/>' +
      '<line x1="2" y1="12" x2="22" y2="12"/>' +
      '<path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
    package:
      '<path d="M16.5 9.4L7.55 4.24"/>' +
      '<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>' +
      '<polyline points="3.27 6.96 12 12.01 20.73 6.96"/>' +
      '<line x1="12" y1="22.08" x2="12" y2="12"/>',
    terminal:
      '<polyline points="4 17 10 11 4 5"/>' +
      '<line x1="12" y1="19" x2="20" y2="19"/>',
    lock:
      '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>' +
      '<path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
    key:
      '<path d="M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4"/>',
    book:
      '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/>' +
      '<path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"/>',
    tool:
      '<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>',
    type:
      '<polyline points="4 7 4 4 20 4 20 7"/>' +
      '<line x1="9" y1="20" x2="15" y2="20"/>' +
      '<line x1="12" y1="4" x2="12" y2="20"/>',
    "git-branch":
      '<line x1="6" y1="3" x2="6" y2="15"/>' +
      '<circle cx="18" cy="6" r="3"/>' +
      '<circle cx="6" cy="18" r="3"/>' +
      '<path d="M18 9a9 9 0 0 1-9 9"/>',
    download:
      '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>' +
      '<polyline points="7 10 12 15 17 10"/>' +
      '<line x1="12" y1="15" x2="12" y2="3"/>',
    upload:
      '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>' +
      '<polyline points="17 8 12 3 7 8"/>' +
      '<line x1="12" y1="3" x2="12" y2="15"/>',
    "sidebar-left":
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<line x1="9" y1="3" x2="9" y2="21"/>',
    "sidebar-right":
      '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>' +
      '<line x1="15" y1="3" x2="15" y2="21"/>',
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  class="icon"
>
  {@html PATHS[name]}
</svg>

<style>
  .icon {
    flex: 0 0 auto;
  }
</style>
