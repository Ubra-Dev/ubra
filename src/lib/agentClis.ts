/** An installed agent CLI reported by the detect_agent_clis command. */
export interface DetectedCli {
  cli: string;
  label: string;
  path: string;
}

/** Dropdown value selecting the free-text command field. */
export const CUSTOM_COMMAND = "custom";

/** The command to run for an agent dropdown selection. */
export function resolveAgentCommand(selection: string, custom: string): string {
  return selection === CUSTOM_COMMAND ? custom.trim() : selection;
}
