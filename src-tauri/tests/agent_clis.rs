//! Detection smoke test: whatever the scan reports must be real.
use std::collections::HashSet;
use std::path::Path;
use ubra_lib::agent_clis::detect;
use ubra_lib::agent_watch::AGENT_TABLE;

#[test]
fn detected_clis_exist_and_come_from_the_agent_table() {
    let known: HashSet<&str> = AGENT_TABLE.iter().map(|(stem, _)| *stem).collect();
    for cli in detect() {
        assert!(
            known.contains(cli.cli.as_str()),
            "unexpected cli: {}",
            cli.cli
        );
        assert!(!cli.label.is_empty(), "missing label for {}", cli.cli);
        assert!(
            Path::new(&cli.path).is_file(),
            "missing binary for {}: {}",
            cli.cli,
            cli.path
        );
    }
}
