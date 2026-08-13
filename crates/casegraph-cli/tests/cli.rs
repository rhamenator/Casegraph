use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn binary_help_and_demo_are_machine_readable() {
    let binary = env!("CARGO_BIN_EXE_casegraph");
    let help = Command::new(binary)
        .arg("--help")
        .output()
        .expect("run help");
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("casegraph demo"));

    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "casegraph-cli-process-{}-{sequence}",
        std::process::id()
    ));
    let demo = Command::new(binary)
        .arg("demo")
        .env("CASEGRAPH_DATA_DIR", &root)
        .output()
        .expect("run demo");
    assert!(
        demo.status.success(),
        "{}",
        String::from_utf8_lossy(&demo.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&demo.stdout).expect("one JSON demo result");
    assert_eq!(document["grounded_answer"]["mode"], "established");
    assert_eq!(document["model_provider_used"], false);
    fs::remove_dir_all(root).ok();
}
