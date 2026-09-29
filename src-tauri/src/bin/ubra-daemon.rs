//! `ubra-daemon`: headless agent runtime (Phase 5a). Owns PTYs and the
//! agent watcher; see [`ubra_lib::daemon`] for the wire protocol.

use std::net::TcpListener;
use std::time::Duration;
use ubra_lib::daemon::{
    app_data_dir, default_state_dir, parse_daemon_args, read_port_file, serve, write_port_file,
    DaemonCore, DAEMON_USAGE,
};

fn already_running(state_dir: &std::path::Path) -> bool {
    let Some(port) = read_port_file(state_dir) else {
        return false;
    };
    let addr: std::net::SocketAddr = match format!("127.0.0.1:{}", port.port).parse() {
        Ok(addr) => addr,
        Err(_) => return false,
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok()
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
    if already_running(&state_dir) {
        eprintln!(
            "ubra-daemon: already running (state dir {})",
            state_dir.display()
        );
        std::process::exit(1);
    }
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
    if let Err(e) = write_port_file(&state_dir, port) {
        eprintln!("ubra-daemon: cannot write port file: {e}");
        std::process::exit(1);
    }
    let rules_dir = args
        .rules_dir
        .unwrap_or_else(|| app_data_dir().join("agent-detection"));
    eprintln!("ubra-daemon: listening on 127.0.0.1:{port} (protocol 1)");
    serve(DaemonCore::new(Some(rules_dir), state_dir), listener);
}
