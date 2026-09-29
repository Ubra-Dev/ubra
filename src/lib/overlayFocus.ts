import { tick } from "svelte";

const owners: HTMLElement[] = [];
const focusable = 'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

/** Svelte action: the topmost overlay contains focus and restores its opener. */
export function overlayFocus(node: HTMLElement, options: { initial?: string; opener?: HTMLElement | null } = {}) {
  const opener = options.opener ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
  owners.push(node);
  let disposed = false;
  const items = () => [...node.querySelectorAll<HTMLElement>(focusable)]
    .filter((el) => el.getClientRects().length > 0 && !el.closest('[inert]'));
  const focusInitial = () => {
    const initial = options.initial ? node.querySelector<HTMLElement>(options.initial) : null;
    (initial ?? items()[0] ?? node).focus({ preventScroll: true });
  };
  const contain = (event: FocusEvent) => {
    if (owners.at(-1) === node && !node.contains(event.target as Node)) focusInitial();
  };
  const keydown = (event: KeyboardEvent) => {
    if (owners.at(-1) !== node || event.key !== "Tab") return;
    const controls = items();
    const index = controls.indexOf(document.activeElement as HTMLElement);
    if (index < 0 || (event.shiftKey ? index === 0 : index === controls.length - 1)) {
      event.preventDefault();
      (event.shiftKey ? controls.at(-1) ?? node : controls[0] ?? node).focus();
    }
  };
  document.addEventListener("focusin", contain, true);
  node.addEventListener("keydown", keydown);
  void tick().then(() => {
    if (!disposed && owners.at(-1) === node) focusInitial();
  });
  return {
    destroy() {
      disposed = true;
      owners.splice(owners.indexOf(node), 1);
      document.removeEventListener("focusin", contain, true);
      node.removeEventListener("keydown", keydown);
      requestAnimationFrame(() => {
        const active = document.activeElement;
        // Do not steal focus from a rename input or a newly opened dialog.
        if (active instanceof HTMLElement && active !== document.body && active.isConnected && !node.contains(active)) return;
        if (opener?.isConnected && !opener.closest('[inert]')) {
          opener.focus({ preventScroll: true });
          if (document.activeElement === opener) return;
        }
        const fallback = [...document.querySelectorAll<HTMLElement>('.pane-view .xterm-helper-textarea, .tab.active .name, .ws.active .name, .settings-btn')]
          .find((el) => el.getClientRects().length > 0 && !el.closest('[inert]'));
        fallback?.focus({ preventScroll: true });
      });
    },
  };
}
