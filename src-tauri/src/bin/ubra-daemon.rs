//! `ubra-daemon`: headless agent runtime (Phase 5a). Owns PTYs and the
//! agent watcher; see [`ubra_lib::daemon`] for the wire protocol.

use std::net::TcpListener;
use std::time::Duration;
use ubra_lib::daemon::{
    acquire_startup_lock, app_data_dir, connect_authenticated, default_state_dir, new_auth_token,
    parse_daemon_args, secure_state_dir, serve, write_auth_token, write_port_file, DaemonCore,
    DAEMON_USAGE, PROTOCOL_VERSION,
};

fn already_running(state_dir: &std::path::Path) -> bool {
    connect_authenticated(state_dir, Duration::from_millis(500)).is_ok()
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_daemon_args(&raw) {
        Ok(args) => args,
        Err(e) if e == "help" => {
            println!("{DAEMON_USAGE}");
            return;
        }
        Err(e) => {
            eprintln!("ubra-daemon: {e}\n{DAEMON_USAGE}");
            std::process::exit(2);
        }
    };
    let state_dir = args.state_dir.unwrap_or_else(default_state_dir);
    if let Err(e) = secure_state_dir(&state_dir) {
        eprintln!(
            "ubra-daemon: cannot secure state dir {}: {e}",
            state_dir.display()
        );
        std::process::exit(1);
    }
    if already_running(&state_dir) {
        eprintln!(
            "ubra-daemon: already running (state dir {})",
            state_dir.display()
        );
        std::process::exit(1);
    }
    let _lock = match acquire_startup_lock(&state_dir) {
        Ok(lock) => lock,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            eprintln!(
                "ubra-daemon: already running (state dir {})",
                state_dir.display()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!(
                "ubra-daemon: cannot acquire startup lock {}: {e}",
                state_dir.display()
            );
            std::process::exit(1);
        }
    };
    let auth_token = match new_auth_token() {
        Ok(token) => token,
        Err(e) => {
            eprintln!("ubra-daemon: cannot create daemon credential: {e}");
            std::process::exit(1);
        }
    };
    let listener = match TcpListener::bind(("127.0.0.1", args.port)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("ubra-daemon: cannot bind 127.0.0.1:{}: {e}", args.port);
            std::process::exit(1);
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(e) => {
            eprintln!("ubra-daemon: cannot read bound port: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = write_auth_token(&state_dir, &auth_token) {
        eprintln!("ubra-daemon: cannot write daemon credential: {e}");
        std::process::exit(1);
    }
    if let Err(e) = write_port_file(&state_dir, port) {
        eprintln!("ubra-daemon: cannot write port file: {e}");
        std::process::exit(1);
    }
    let rules_dir = args
        .rules_dir
        .unwrap_or_else(|| app_data_dir().join("agent-detection"));
    eprintln!("ubra-daemon: listening on 127.0.0.1:{port} (protocol {PROTOCOL_VERSION})");
    serve(
        DaemonCore::new(Some(rules_dir), state_dir),
        listener,
        auth_token,
    );
}
