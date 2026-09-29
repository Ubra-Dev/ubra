//! `ubra-cli`: drive the daemon from scripts and terminals (Phase 5a).
//! See [`ubra_lib::cli`] for commands.

use ubra_lib::cli::{parse_cli_args, run, CLI_USAGE};

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let opts = match parse_cli_args(&raw) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("ubra-cli: {e}");
            if !e.contains("usage:") {
                eprintln!("{CLI_USAGE}");
            }
            std::process::exit(2);
        }
    };
    if let Err(e) = run(opts) {
        eprintln!("ubra-cli: {e}");
        std::process::exit(1);
    }
}
