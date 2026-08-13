use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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

    let root = TestDirectory::new("casegraph-cli-process");
    let demo = Command::new(binary)
        .arg("demo")
        .env("CASEGRAPH_DATA_DIR", root.path())
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
}

#[test]
fn binary_commands_cover_persistent_evidence_workflow_and_fail_closed() {
    let binary = env!("CARGO_BIN_EXE_casegraph");
    let root = TestDirectory::new("casegraph-cli-commands");

    assert_eq!(
        run_json(binary, root.path(), &["init"])["status"],
        "initialized"
    );
    let case = run_json(
        binary,
        root.path(),
        &["case", "create", "Synthetic CLI Case"],
    );
    let case_id = case["id"].as_str().expect("case id").to_owned();

    let text_path = root.path().join("record.txt");
    fs::write(
        &text_path,
        "received_date: 2026-08-12\nresponse_required: true\namount: $14.25\n",
    )
    .expect("write text fixture");
    let ingest = run_json(
        binary,
        root.path(),
        &[
            "ingest",
            text_path.to_str().expect("UTF-8 test path"),
            "--case",
            &case_id,
        ],
    );
    assert_eq!(
        ingest["extraction"]["claims"].as_array().map(Vec::len),
        Some(3)
    );

    let artifacts = run_json(
        binary,
        root.path(),
        &["artifacts", "list", "--case", &case_id],
    );
    assert_eq!(artifacts.as_array().map(Vec::len), Some(1));
    let claims = run_json(binary, root.path(), &["claims", "list", "--case", &case_id]);
    let claim_id = claims[0]["id"].as_str().expect("claim id");
    assert_eq!(
        run_json(binary, root.path(), &["verify", claim_id])["decision"],
        "verified"
    );
    assert!(run_json(binary, root.path(), &["correct", claim_id, "corrected"])["id"].is_string());
    assert!(
        run_json(binary, root.path(), &["query", &case_id, "What is known?"])["mode"].is_string()
    );
    assert!(
        run_json(
            binary,
            root.path(),
            &["contradictions", "list", "--case", &case_id]
        )
        .is_array()
    );
    assert_eq!(run_json(binary, root.path(), &["test"])["status"], "ok");

    let pdf_path = root.path().join("preserved.pdf");
    fs::write(&pdf_path, b"not an executable document").expect("write PDF fixture");
    let unsupported = run_json(
        binary,
        root.path(),
        &[
            "ingest",
            pdf_path.to_str().expect("UTF-8 test path"),
            "--case",
            &case_id,
        ],
    );
    assert_eq!(unsupported["extraction"]["status"], "unsupported");

    let bad = run(binary, root.path(), &["query", "bad!", "question"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("invalid"));
    let unknown = run(binary, root.path(), &["not-a-command"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unsupported command"));
    let missing = run(
        binary,
        root.path(),
        &["ingest", "missing.txt", "--case", &case_id],
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("not readable"));

    let file_data_dir = root.path().join("not-a-directory");
    fs::write(&file_data_dir, "file").expect("write invalid data dir");
    let invalid_data_dir = run(binary, &file_data_dir, &["init"]);
    assert!(!invalid_data_dir.status.success());
    assert!(String::from_utf8_lossy(&invalid_data_dir.stderr).contains("data directory"));
}

fn run(binary: &str, data_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(binary)
        .args(args)
        .env("CASEGRAPH_DATA_DIR", data_dir)
        .output()
        .expect("run CLI command")
}

fn run_json(binary: &str, data_dir: &Path, args: &[&str]) -> serde_json::Value {
    let result = run(binary, data_dir, args);
    assert!(
        result.status.success(),
        "command {args:?} failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).expect("one JSON command result")
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(prefix: &str) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock after epoch")
            .as_nanos();
        for _ in 0..100 {
            let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create isolated test directory: {error}"),
            }
        }
        panic!("could not allocate an isolated test directory")
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}
