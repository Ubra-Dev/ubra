/**
 * Implicit agent-launch decision: the workspace default, last-used, or
 * first-detected CLI for a new terminal. Null when auto-launch is off or
 * there is no candidate. Explicit user picks bypass this entirely and
 * always run.
 */
export function implicitLaunchCommand(
  autoLaunch: boolean,
  candidate: string | null | undefined,
): string | null {
  if (!autoLaunch) return null;
  const trimmed = candidate?.trim() ?? "";
  return trimmed ? trimmed : null;
}
