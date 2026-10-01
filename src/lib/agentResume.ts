// Agent resume commands: rebuild a restorable command line from the pane's
// last agent CLI plus its captured session. Only verified strategies ship:
// anything else falls back to retyping the original command.
export interface AgentSession {
  cli: string;
  value: string;
}

const MAX_RESUME_ARGS = 64;
const MAX_RESUME_ARGV_BYTES = 8192;

/**
 * Resume argv for a pane command, or null to retype the original. `command`
 * is the full string once queued (often bare, sometimes flagged); only its
 * first token names the CLI. A captured session applies solely to the CLI
 * that produced it.
 */
export function resumeArgvFor(
  command: string,
  session: AgentSession | undefined,
): string[] | null {
  const name = command.split(/\s+/, 1)[0]?.toLowerCase() ?? "";
  if (!/^[a-z0-9._-]+$/.test(name) || name.startsWith("-")) return null;
  const ref =
    session && session.cli.toLowerCase() === name ? session.value : null;
  let argv: string[] | null;
  switch (name) {
    case "claude":
      // --continue resumes the most recent session in the pane's cwd.
      argv = ["claude", "--continue"];
      break;
    case "codex":
      argv = ref ? ["codex", "resume", ref] : null;
      break;
    default:
      return null;
  }
  return argv && isValidResumeArgv(argv) ? argv : null;
}

/** Restore command line: the resume argv, or the original when unresumable. */
export function restoreCommandFor(
  agentCli: string,
  session: AgentSession | undefined,
): string {
  const argv = resumeArgvFor(agentCli, session);
  return argv ? joinArgvPosix(argv) : agentCli;
}

/** Layout files are user-editable: resume input is validated before use. */
export function isValidResumeArgv(argv: string[]): boolean {
  const command = argv[0];
  if (
    !command ||
    argv.length > MAX_RESUME_ARGS ||
    argv.some(
      (arg) =>
        [...arg].some((ch) => ch.charCodeAt(0) < 32 || ch.charCodeAt(0) === 127) ||
        arg.includes("'"),
    ) ||
    argv.reduce((n, arg) => n + arg.length, 0) > MAX_RESUME_ARGV_BYTES
  ) {
    return false;
  }
  return /^[a-z0-9._-]+$/.test(command) && !command.startsWith("-");
}

/** POSIX shell joining; argv must pass validation (no apostrophes). */
export function joinArgvPosix(argv: string[]): string {
  return argv
    .map((arg) => (/^[a-zA-Z0-9._:/=@%+-]+$/.test(arg) ? arg : `'${arg}'`))
    .join(" ");
}
