//! `ubra-cli`: drive the daemon (Phase 5a) from scripts and terminals.
//!
//! All output is pretty-printed JSON. When no daemon answers, the CLI
//! starts one (sibling `ubra-daemon` binary, else `PATH`) and retries.
//!
//! ```text
//! ubra-cli [--state-dir DIR] <command> [args]
//!   ping | spawn | write | resize | kill | read | panes | agents | snapshot
//!   rules-reload | shutdown | help
//! ```

use crate::daemon::{
    connect_authenticated, default_state_dir, open_private_file, read_frame, write_frame,
    MAX_FRAME_BYTES, PROTOCOL_VERSION,
};
use crate::pty_manager::PaneId;
use std::io::BufReader;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum CliCommand {
    Ping,
    Spawn {
        shell: Option<String>,
        cwd: Option<String>,
        args: Vec<String>,
        cols: u16,
        rows: u16,
    },
    Write {
        pane: PaneId,
        data: String,
    },
    Resize {
        pane: PaneId,
        cols: u16,
        rows: u16,
    },
    Kill {
        pane: PaneId,
    },
    Read {
        pane: PaneId,
    },
    SendText {
        pane: PaneId,
        data: String,
    },
    SendKeys {
        pane: PaneId,
        keys: Vec<String>,
    },
    Prompt {
        pane: PaneId,
        data: String,
    },
    WaitState {
        pane: PaneId,
        want: Vec<String>,
        timeout: Option<u64>,
    },
    WaitOutput {
        pane: PaneId,
        contains: String,
        timeout: Option<u64>,
    },
    Panes,
    Agents {
        watch: bool,
    },
    Snapshot,
    RulesReload,
    Shutdown,
    Help,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CliOptions {
    pub state_dir: Option<PathBuf>,
    pub command: CliCommand,
}

pub const CLI_USAGE: &str = "\
usage: ubra-cli [--state-dir DIR] <command> [args]

  ping                          daemon + protocol version
  spawn [--shell S] [--cwd D] [--cols N] [--rows N] [-- args...]
  write <pane> <text...>        write text to a pane (args joined by space)
  send-text <pane> <text...>    write as one bracketed paste
  send-keys <pane> <key...>     send key names (enter, escape, ctrl-c, ...)
  prompt <pane> <text...>       paste text and submit with Enter
  wait-state <pane> <want...> [--timeout N]
  wait-output <pane> <text...> [--timeout N]
  resize <pane> <cols> <rows>
  kill <pane>
  read <pane>                   pane screen text
  panes                         live panes
  agents [--watch]              agent states (watch streams until killed)
  snapshot                      version + panes + agent states
  rules-reload                  re-read detection rule files
  shutdown                      kill panes and stop the daemon
  help";

fn pane_arg(args: &[String], i: usize, what: &str) -> Result<PaneId, String> {
    args.get(i)
        .ok_or_else(|| format!("missing {what}"))?
        .parse()
        .map_err(|_| format!("invalid {what} (want a pane id)"))
}

fn u16_arg(args: &[String], i: usize, what: &str) -> Result<u16, String> {
    args.get(i)
        .ok_or_else(|| format!("missing {what}"))?
        .parse()
        .map_err(|_| format!("invalid {what} (want 0-65535)"))
}

/// Split trailing args into positionals plus an optional `--timeout N`.
fn split_timeout(args: &[String]) -> Result<(Vec<String>, Option<u64>), String> {
    let mut words = Vec::new();
    let mut timeout = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--timeout" {
            i += 1;
            timeout = Some(
                args.get(i)
                    .ok_or("missing --timeout value")?
                    .parse()
                    .map_err(|_| "invalid --timeout (want seconds)")?,
            );
        } else if args[i].starts_with("--") {
            return Err(format!("unknown flag: {}", args[i]));
        } else {
            words.push(args[i].clone());
        }
        i += 1;
    }
    Ok((words, timeout))
}

pub fn parse_cli_args(args: &[String]) -> Result<CliOptions, String> {
    let mut rest = args;
    let mut state_dir = None;
    if rest.first().is_some_and(|s| s == "--state-dir") {
        state_dir = Some(PathBuf::from(
            rest.get(1).ok_or("missing value for --state-dir")?,
        ));
        rest = &rest[2..];
    }
    let (cmd, args) = rest
        .split_first()
        .ok_or_else(|| "missing command\n".to_string() + CLI_USAGE)?;
    let command = match cmd.as_str() {
        "ping" => CliCommand::Ping,
        "spawn" => {
            let mut shell = None;
            let mut cwd = None;
            let mut cols = 80;
            let mut rows = 24;
            let mut extra = Vec::new();
            let mut i = 0;
            let mut trailing = false;
            while i < args.len() {
                match args[i].as_str() {
                    "--" => trailing = true,
                    "--shell" if !trailing => {
                        i += 1;
                        shell = Some(args.get(i).cloned().ok_or("missing --shell value")?);
                    }
                    "--cwd" if !trailing => {
                        i += 1;
                        cwd = Some(args.get(i).cloned().ok_or("missing --cwd value")?);
                    }
                    "--cols" if !trailing => {
                        i += 1;
                        cols = u16_arg(args, i, "--cols")?;
                    }
                    "--rows" if !trailing => {
                        i += 1;
                        rows = u16_arg(args, i, "--rows")?;
                    }
                    flag if !trailing && flag.starts_with("--") => {
                        return Err(format!("unknown spawn flag: {flag}"));
                    }
                    other => extra.push(other.to_string()),
                }
                i += 1;
                if trailing && i < args.len() && args[i - 1] == "--" {
                    // "--" consumed; the rest are literal args.
                    extra.extend(args[i..].iter().cloned());
                    break;
                }
            }
            CliCommand::Spawn {
                shell,
                cwd,
                args: extra,
                cols,
                rows,
            }
        }
        "write" => {
            let pane = pane_arg(args, 0, "pane")?;
            if args.len() < 2 {
                return Err("missing text".to_string());
            }
            CliCommand::Write {
                pane,
                data: args[1..].join(" "),
            }
        }
        "send-text" => {
            let pane = pane_arg(args, 0, "pane")?;
            if args.len() < 2 {
                return Err("missing text".to_string());
            }
            CliCommand::SendText {
                pane,
                data: args[1..].join(" "),
            }
        }
        "send-keys" => {
            let pane = pane_arg(args, 0, "pane")?;
            if args.len() < 2 {
                return Err("missing keys".to_string());
            }
            CliCommand::SendKeys {
                pane,
                keys: args[1..].to_vec(),
            }
        }
        "prompt" => {
            let pane = pane_arg(args, 0, "pane")?;
            if args.len() < 2 {
                return Err("missing text".to_string());
            }
            CliCommand::Prompt {
                pane,
                data: args[1..].join(" "),
            }
        }
        "wait-state" => {
            let pane = pane_arg(args, 0, "pane")?;
            let (want, timeout) = split_timeout(&args[1..])?;
            if want.is_empty() {
                return Err("missing want states".to_string());
            }
            CliCommand::WaitState {
                pane,
                want,
                timeout,
            }
        }
        "wait-output" => {
            let pane = pane_arg(args, 0, "pane")?;
            let (words, timeout) = split_timeout(&args[1..])?;
            if words.is_empty() {
                return Err("missing text".to_string());
            }
            CliCommand::WaitOutput {
                pane,
                contains: words.join(" "),
                timeout,
            }
        }
        "resize" => CliCommand::Resize {
            pane: pane_arg(args, 0, "pane")?,
            cols: u16_arg(args, 1, "cols")?,
            rows: u16_arg(args, 2, "rows")?,
        },
        "kill" => CliCommand::Kill {
            pane: pane_arg(args, 0, "pane")?,
        },
        "read" => CliCommand::Read {
            pane: pane_arg(args, 0, "pane")?,
        },
        "panes" => CliCommand::Panes,
        "agents" => CliCommand::Agents {
            watch: match args.first().map(String::as_str) {
                None => false,
                Some("--watch") => true,
                Some(flag) => return Err(format!("unknown agents flag: {flag}")),
            },
        },
        "snapshot" => CliCommand::Snapshot,
        "rules-reload" => CliCommand::RulesReload,
        "shutdown" => CliCommand::Shutdown,
        "help" | "--help" | "-h" => CliCommand::Help,
        other => return Err(format!("unknown command: {other}\n{CLI_USAGE}")),
    };
    Ok(CliOptions { state_dir, command })
}

fn state_dir_of(opts: &CliOptions) -> PathBuf {
    opts.state_dir.clone().unwrap_or_else(default_state_dir)
}

fn try_authenticated(state_dir: &std::path::Path) -> Option<TcpStream> {
    connect_authenticated(state_dir, Duration::from_millis(500)).ok()
}

fn daemon_log(state_dir: &std::path::Path) -> Result<std::fs::File, String> {
    open_private_file(&state_dir.join("daemon.log"), true, true)
        .map_err(|e| format!("cannot open private daemon log: {e}"))
}

fn daemon_binary() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(windows)]
            let name = "ubra-daemon.exe";
            #[cfg(not(windows))]
            let name = "ubra-daemon";
            let sibling = dir.join(name);
            if sibling.is_file() {
                return sibling;
            }
        }
    }
    PathBuf::from("ubra-daemon")
}

/// Connect, starting a daemon first when none answers.
pub fn connect_or_start(state_dir: &std::path::Path) -> Result<TcpStream, String> {
    if let Some(stream) = try_authenticated(state_dir) {
        return Ok(stream);
    }
    let log = daemon_log(state_dir)?;
    let mut child = Command::new(daemon_binary())
        .arg("--state-dir")
        .arg(state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .map_err(|e| format!("cannot start ubra-daemon: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(stream) = connect_authenticated(
            state_dir,
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(250)),
        ) {
            return Ok(stream);
        }
    }
    Err(format!(
        "ubra-daemon did not answer (state dir {}; log at daemon.log)",
        state_dir.display()
    ))
}

fn valid_pane_id(value: &serde_json::Value) -> bool {
    value
        .as_u64()
        .is_some_and(|id| id > 0 && id <= u64::from(PaneId::MAX))
}

fn valid_panes(value: &serde_json::Value) -> bool {
    value.as_array().is_some_and(|panes| {
        panes.iter().all(|pane| {
            valid_pane_id(&pane["id"])
                && pane["rootPid"]
                    .as_u64()
                    .is_some_and(|pid| pid <= u64::from(u32::MAX))
        })
    })
}

fn valid_states(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|states| {
        states.iter().all(|(pane, status)| {
            pane.parse::<PaneId>().is_ok_and(|id| id != 0)
                && status["state"].is_string()
                && status["reason"].is_string()
                && status["source"].is_string()
        })
    })
}

fn validate_response(value: &serde_json::Value, op: &str) -> Result<(), String> {
    let Some(ok) = value["ok"].as_bool() else {
        return Err("daemon reply missing boolean ok".into());
    };
    if !ok {
        return Err(value["error"]
            .as_str()
            .unwrap_or("daemon operation failed")
            .to_string());
    }
    let shape_ok = match op {
        "ping" => value["protocol"] == PROTOCOL_VERSION && value["version"].is_string(),
        "pty_spawn" => valid_pane_id(&value["pane"]),
        "pty_read" => value["screen"].is_string(),
        "panes" => valid_panes(&value["panes"]),
        "agent_states" => valid_states(&value["states"]) && value["revision"].as_u64().is_some(),
        "snapshot" => {
            value["protocol"] == PROTOCOL_VERSION
                && value["version"].is_string()
                && valid_panes(&value["panes"])
                && valid_states(&value["states"])
                && value["revision"].as_u64().is_some()
        }
        "wait_state" => {
            valid_pane_id(&value["pane"])
                && value["state"].is_string()
                && value["reason"].is_string()
        }
        "wait_output" => valid_pane_id(&value["pane"]),
        "shutdown" => value["shutdown"] == true,
        _ => true,
    };
    if shape_ok {
        Ok(())
    } else {
        Err(format!("malformed {op} reply"))
    }
}

fn read_response(
    reader: &mut BufReader<TcpStream>,
    id: u64,
    op: &str,
    deadline: Instant,
) -> Result<serde_json::Value, String> {
    loop {
        let line =
            read_frame(reader, deadline).map_err(|e| format!("lost daemon connection: {e}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&line).map_err(|e| format!("bad daemon reply: {e}"))?;
        if value["event"].is_string() {
            continue;
        }
        if value["id"].as_u64().is_none() || !value["ok"].is_boolean() {
            print(&value);
            return Err("malformed daemon response envelope".into());
        }
        if value["id"] != id {
            continue;
        }
        if let Err(error) = validate_response(&value, op) {
            print(&value);
            return Err(error);
        }
        return Ok(value);
    }
}

fn request(
    stream: &mut TcpStream,
    mut value: serde_json::Value,
) -> Result<serde_json::Value, String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let op = value["op"]
        .as_str()
        .ok_or("missing request op")?
        .to_string();
    let seconds = if op == "wait_state" || op == "wait_output" {
        value["timeout_secs"].as_u64().unwrap_or(30).clamp(1, 600) + 5
    } else {
        10
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    value["id"] = id.into();
    let encoded = value.to_string();
    if encoded.len() + 1 > MAX_FRAME_BYTES {
        return Err("request exceeds frame byte limit".into());
    }
    write_frame(
        stream,
        &encoded,
        deadline.min(Instant::now() + Duration::from_secs(5)),
    )
    .map_err(|e| format!("lost daemon connection: {e}"))?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    read_response(&mut reader, id, &op, deadline)
}

fn print(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
    );
}

/// Execute one CLI command against the daemon.
pub fn run(opts: CliOptions) -> Result<(), String> {
    if opts.command == CliCommand::Help {
        println!("{CLI_USAGE}");
        return Ok(());
    }
    let state_dir = state_dir_of(&opts);
    let mut stream = if opts.command == CliCommand::Shutdown {
        connect_authenticated(&state_dir, Duration::from_secs(2))
            .map_err(|e| format!("cannot confirm daemon shutdown: {e}"))?
    } else {
        connect_or_start(&state_dir)?
    };
    match opts.command {
        CliCommand::Help => println!("{CLI_USAGE}"),
        CliCommand::Ping => print(&request(&mut stream, serde_json::json!({"op": "ping"}))?),
        CliCommand::Spawn {
            shell,
            cwd,
            args,
            cols,
            rows,
        } => print(&request(
            &mut stream,
            serde_json::json!({
                "op": "pty_spawn", "shell": shell, "cwd": cwd,
                "args": args, "cols": cols, "rows": rows,
            }),
        )?),
        CliCommand::Write { pane, data } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_write", "pane": pane, "data": data}),
        )?),
        CliCommand::SendText { pane, data } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_send_text", "pane": pane, "data": data}),
        )?),
        CliCommand::SendKeys { pane, keys } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_send_keys", "pane": pane, "keys": keys}),
        )?),
        CliCommand::Prompt { pane, data } => print(&request(
            &mut stream,
            serde_json::json!({"op": "agent_prompt", "pane": pane, "data": data}),
        )?),
        CliCommand::WaitState {
            pane,
            want,
            timeout,
        } => print(&request(
            &mut stream,
            serde_json::json!({
                "op": "wait_state", "pane": pane, "want": want, "timeout_secs": timeout,
            }),
        )?),
        CliCommand::WaitOutput {
            pane,
            contains,
            timeout,
        } => print(&request(
            &mut stream,
            serde_json::json!({
                "op": "wait_output", "pane": pane, "contains": contains, "timeout_secs": timeout,
            }),
        )?),
        CliCommand::Resize { pane, cols, rows } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_resize", "pane": pane, "cols": cols, "rows": rows}),
        )?),
        CliCommand::Kill { pane } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_kill", "pane": pane}),
        )?),
        CliCommand::Read { pane } => print(&request(
            &mut stream,
            serde_json::json!({"op": "pty_read", "pane": pane}),
        )?),
        CliCommand::Panes => print(&request(&mut stream, serde_json::json!({"op": "panes"}))?),
        CliCommand::Agents { watch } => {
            if !watch {
                print(&request(
                    &mut stream,
                    serde_json::json!({"op": "agent_states"}),
                )?);
            } else {
                let mut reader = BufReader::new(stream);
                loop {
                    let line = read_frame(&mut reader, Instant::now() + Duration::from_secs(60))
                        .map_err(|e| format!("lost daemon watch connection: {e}"))?;
                    let value: serde_json::Value = serde_json::from_str(&line)
                        .map_err(|e| format!("bad daemon event: {e}"))?;
                    if value["event"] == "agent-states" {
                        if !valid_states(&value["states"]) {
                            print(&value);
                            return Err("malformed agent states event".into());
                        }
                        print(&value);
                    }
                }
            }
        }
        CliCommand::Snapshot => print(&request(
            &mut stream,
            serde_json::json!({"op": "snapshot"}),
        )?),
        CliCommand::RulesReload => print(&request(
            &mut stream,
            serde_json::json!({"op": "rules_reload"}),
        )?),
        CliCommand::Shutdown => {
            print(&request(&mut stream, serde_json::json!({"op":"shutdown"}))?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn parses_simple_commands() {
        assert_eq!(
            parse_cli_args(&args(&["ping"])).unwrap().command,
            CliCommand::Ping
        );
        assert_eq!(
            parse_cli_args(&args(&["panes"])).unwrap().command,
            CliCommand::Panes
        );
        assert_eq!(
            parse_cli_args(&args(&["snapshot"])).unwrap().command,
            CliCommand::Snapshot
        );
        assert_eq!(
            parse_cli_args(&args(&["rules-reload"])).unwrap().command,
            CliCommand::RulesReload
        );
        assert_eq!(
            parse_cli_args(&args(&["shutdown"])).unwrap().command,
            CliCommand::Shutdown
        );
        assert_eq!(
            parse_cli_args(&args(&["help"])).unwrap().command,
            CliCommand::Help
        );
    }

    #[test]
    fn parses_state_dir_prefix() {
        let opts = parse_cli_args(&args(&["--state-dir", "/tmp/x", "ping"])).unwrap();
        assert_eq!(opts.state_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(opts.command, CliCommand::Ping);
        assert!(parse_cli_args(&args(&["--state-dir"])).is_err());
    }

    #[test]
    fn parses_pane_commands() {
        assert_eq!(
            parse_cli_args(&args(&["write", "3", "echo", "hi"]))
                .unwrap()
                .command,
            CliCommand::Write {
                pane: 3,
                data: "echo hi".to_string(),
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["resize", "3", "100", "30"]))
                .unwrap()
                .command,
            CliCommand::Resize {
                pane: 3,
                cols: 100,
                rows: 30,
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["kill", "3"])).unwrap().command,
            CliCommand::Kill { pane: 3 }
        );
        assert_eq!(
            parse_cli_args(&args(&["read", "3"])).unwrap().command,
            CliCommand::Read { pane: 3 }
        );
        assert!(parse_cli_args(&args(&["write", "3"])).is_err());
        assert!(parse_cli_args(&args(&["write", "x", "hi"])).is_err());
        assert!(parse_cli_args(&args(&["resize", "3", "100"])).is_err());
        assert!(parse_cli_args(&args(&["kill"])).is_err());
    }

    #[test]
    fn parses_spawn_flags_and_trailing_args() {
        assert_eq!(
            parse_cli_args(&args(&[
                "spawn", "--shell", "sh", "--cwd", "/tmp", "--cols", "100", "--rows", "30", "--",
                "-c", "echo hi"
            ]))
            .unwrap()
            .command,
            CliCommand::Spawn {
                shell: Some("sh".to_string()),
                cwd: Some("/tmp".to_string()),
                args: vec!["-c".to_string(), "echo hi".to_string()],
                cols: 100,
                rows: 30,
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["spawn"])).unwrap().command,
            CliCommand::Spawn {
                shell: None,
                cwd: None,
                args: vec![],
                cols: 80,
                rows: 24,
            }
        );
        assert!(parse_cli_args(&args(&["spawn", "--shell"])).is_err());
        assert!(parse_cli_args(&args(&["spawn", "--bogus"])).is_err());
    }

    #[test]
    fn parses_automation_commands() {
        assert_eq!(
            parse_cli_args(&args(&["send-text", "2", "hello", "there"]))
                .unwrap()
                .command,
            CliCommand::SendText {
                pane: 2,
                data: "hello there".to_string(),
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["send-keys", "2", "escape", "enter"]))
                .unwrap()
                .command,
            CliCommand::SendKeys {
                pane: 2,
                keys: vec!["escape".to_string(), "enter".to_string()],
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["prompt", "2", "do", "it"]))
                .unwrap()
                .command,
            CliCommand::Prompt {
                pane: 2,
                data: "do it".to_string(),
            }
        );
        assert_eq!(
            parse_cli_args(&args(&[
                "wait-state",
                "2",
                "idle",
                "blocked",
                "--timeout",
                "9"
            ]))
            .unwrap()
            .command,
            CliCommand::WaitState {
                pane: 2,
                want: vec!["idle".to_string(), "blocked".to_string()],
                timeout: Some(9),
            }
        );
        assert_eq!(
            parse_cli_args(&args(&["wait-output", "2", "some", "text"]))
                .unwrap()
                .command,
            CliCommand::WaitOutput {
                pane: 2,
                contains: "some text".to_string(),
                timeout: None,
            }
        );
        assert!(parse_cli_args(&args(&["send-text", "2"])).is_err());
        assert!(parse_cli_args(&args(&["send-keys", "2"])).is_err());
        assert!(parse_cli_args(&args(&["wait-state", "2"])).is_err());
        assert!(parse_cli_args(&args(&["wait-state", "2", "idle", "--timeout"])).is_err());
        assert!(parse_cli_args(&args(&["wait-state", "2", "idle", "--bogus"])).is_err());
    }

    #[test]
    fn parses_agents_watch_and_rejects_unknown() {
        assert_eq!(
            parse_cli_args(&args(&["agents"])).unwrap().command,
            CliCommand::Agents { watch: false }
        );
        assert_eq!(
            parse_cli_args(&args(&["agents", "--watch"]))
                .unwrap()
                .command,
            CliCommand::Agents { watch: true }
        );
        assert!(parse_cli_args(&args(&["agents", "--bogus"])).is_err());
        assert!(parse_cli_args(&[]).is_err());
        assert!(parse_cli_args(&args(&["bogus"])).is_err());
    }
}
