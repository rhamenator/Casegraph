#![forbid(unsafe_code)]

//! Useful CLI adapter over the same application services as the HTTP API.

use casegraph_api::ApiState;
use casegraph_application::{
    CasegraphService, CorrectClaimRequest, CreateCaseRequest, DomainPackage, EvaluateRuleRequest,
    ExtractArtifactRequest, ExtractionPipeline, IngestBytesRequest, RegisterRuleRequest,
    ReviewClaimRequest, RuleWorkflowService,
};
use casegraph_domain::{KnowledgeValue, MaterialValue, RecordId, ReviewDecision};
use casegraph_infrastructure::{
    Config, CoreDeterministicExtractor, FilesystemArtifactStore, Sha256IdGenerator,
    SqliteEvidenceRepository, SystemClock,
};
use casegraph_sample_domain::SampleAdministrativeCase;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::sync::Arc;

struct Services {
    evidence: CasegraphService,
    rules: RuleWorkflowService,
}

impl Services {
    fn open(data_dir: &Path, max_artifact_bytes: u64) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|_| "could not create data directory".to_owned())?;
        let repository = Arc::new(
            SqliteEvidenceRepository::open(data_dir.join("casegraph.db"))
                .map_err(|error| error.to_string())?,
        );
        let artifacts = Arc::new(
            FilesystemArtifactStore::new(data_dir.join("artifacts"))
                .map_err(|error| error.to_string())?,
        );
        let clock = Arc::new(SystemClock);
        let ids = Arc::new(Sha256IdGenerator::default());
        let evidence = CasegraphService::new(
            repository.clone(),
            artifacts,
            clock.clone(),
            ids.clone(),
            max_artifact_bytes,
        )
        .map_err(|error| error.to_string())?;
        let rules = RuleWorkflowService::new(repository, clock, ids);
        Ok(Self { evidence, rules })
    }
}

/// Parse and execute one CLI command.
pub async fn run(args: Vec<String>) -> Result<(), String> {
    let config = Config::from_env().map_err(|error| error.to_string())?;
    let command = args.first().map(String::as_str).unwrap_or("help");
    if command == "help" || command == "--help" || command == "-h" {
        println!("{}", help());
        return Ok(());
    }
    let services = Services::open(&config.data_dir, config.max_artifact_bytes)?;
    match args.as_slice() {
        [command] if command == "init" => output(json!({
            "status": "initialized",
            "data_dir": config.data_dir,
            "model_policy": format!("{:?}", config.model_policy).to_ascii_lowercase(),
        })),
        [case, create, title] if case == "case" && create == "create" => {
            let created = services
                .evidence
                .create_case(CreateCaseRequest {
                    title: title.clone(),
                    actor: "cli:user".to_owned(),
                    correlation_id: None,
                })
                .map_err(|error| error.to_string())?;
            output(to_value(created)?)
        }
        [ingest, path, flag, case_id] if ingest == "ingest" && flag == "--case" => {
            output(ingest_path(&services, Path::new(path), id(case_id)?)?)
        }
        [claims, list, flag, case_id]
            if claims == "claims" && list == "list" && flag == "--case" =>
        {
            output(to_value(
                services
                    .evidence
                    .list_claims(&id(case_id)?)
                    .map_err(|error| error.to_string())?,
            )?)
        }
        [artifacts, list, flag, case_id]
            if artifacts == "artifacts" && list == "list" && flag == "--case" =>
        {
            output(to_value(
                services
                    .evidence
                    .list_artifact_versions(&id(case_id)?)
                    .map_err(|error| error.to_string())?,
            )?)
        }
        [contradictions, list, flag, case_id]
            if contradictions == "contradictions" && list == "list" && flag == "--case" =>
        {
            output(to_value(
                services
                    .evidence
                    .list_contradictions(&id(case_id)?)
                    .map_err(|error| error.to_string())?,
            )?)
        }
        [query, case_id, question] if query == "query" => output(to_value(
            services
                .rules
                .query(&id(case_id)?, question)
                .map_err(|error| error.to_string())?,
        )?),
        [verify, claim_id] if verify == "verify" => output(to_value(
            services
                .evidence
                .review_claim(ReviewClaimRequest {
                    claim_id: id(claim_id)?,
                    decision: ReviewDecision::Verified,
                    actor: "cli:user".to_owned(),
                    rationale: None,
                    correlation_id: None,
                })
                .map_err(|error| error.to_string())?,
        )?),
        [correct, claim_id, corrected] if correct == "correct" => output(to_value(
            services
                .evidence
                .correct_claim(CorrectClaimRequest {
                    claim_id: id(claim_id)?,
                    corrected_original_representation: corrected.clone(),
                    corrected_value: KnowledgeValue::Known(MaterialValue::Text(corrected.clone())),
                    corrected_temporal: None,
                    actor: "cli:user".to_owned(),
                    rationale: None,
                    correlation_id: None,
                })
                .map_err(|error| error.to_string())?,
        )?),
        [demo] if demo == "demo" => output(run_demo(&services)?),
        [test] if test == "test" => output(json!({
            "status": "ok",
            "checks": ["configuration", "database_migrations", "artifact_store"],
            "note": "Run cargo test --workspace --locked for the automated suite."
        })),
        [serve] if serve == "serve" => {
            let listener = tokio::net::TcpListener::bind(config.bind_addr)
                .await
                .map_err(|_| "could not bind API address".to_owned())?;
            eprintln!(
                "{}",
                json!({"event":"api.started","address":config.bind_addr.to_string()})
            );
            casegraph_api::serve(
                listener,
                ApiState {
                    extraction: ExtractionPipeline::new(
                        services.evidence.clone(),
                        vec![Arc::new(CoreDeterministicExtractor)],
                    ),
                    evidence: services.evidence,
                    rules: services.rules,
                    max_artifact_bytes: config.max_artifact_bytes,
                },
            )
            .await
            .map_err(|_| "API server failed".to_owned())?;
        }
        _ => return Err(format!("unsupported command\n\n{}", help())),
    }
    Ok(())
}

fn ingest_path(services: &Services, path: &Path, case_id: RecordId) -> Result<Value, String> {
    let metadata = fs::metadata(path).map_err(|_| "artifact path is not readable".to_owned())?;
    if !metadata.is_file() {
        return Err("artifact path must name one regular file".to_owned());
    }
    let bytes = fs::read(path).map_err(|_| "artifact could not be read".to_owned())?;
    let canonical = fs::canonicalize(path).map_err(|_| "artifact path is invalid".to_owned())?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "artifact filename is not valid UTF-8".to_owned())?;
    let media_type = media_type(path);
    let result = services
        .evidence
        .ingest_bytes(IngestBytesRequest {
            case_id,
            source_key: canonical.to_string_lossy().into_owned(),
            connector: "filesystem".to_owned(),
            locator: canonical.to_string_lossy().into_owned(),
            external_record_id: None,
            endpoint: None,
            source_revision: None,
            media_type,
            original_filename: Some(filename.to_owned()),
            received_at: None,
            bytes,
            actor: "cli:user".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())?;
    let extraction = ExtractionPipeline::new(
        services.evidence.clone(),
        vec![Arc::new(CoreDeterministicExtractor)],
    )
    .extract(ExtractArtifactRequest {
        case_id: result.artifact.case_id.clone(),
        artifact_version_id: result.artifact_version.id.clone(),
        media_type: result.artifact_version.media_type.clone(),
        connector: Some("filesystem".to_owned()),
        actor: "cli:deterministic-extractor".to_owned(),
        correlation_id: None,
    });
    match extraction {
        Ok(extraction) => Ok(json!({"ingestion": result, "extraction": extraction})),
        Err(error)
            if error.kind == casegraph_application::PipelineFailureKind::UnsupportedFormat =>
        {
            Ok(json!({
                "ingestion": result,
                "extraction": {
                    "status": "unsupported",
                    "message": error.safe_message
                }
            }))
        }
        Err(error) => Err(error.safe_message),
    }
}

fn run_demo(services: &Services) -> Result<Value, String> {
    let case_record = services
        .evidence
        .create_case(CreateCaseRequest {
            title: "Invented Sample Administrative Case".to_owned(),
            actor: "demo:runner".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())?;
    let pipeline = ExtractionPipeline::new(
        services.evidence.clone(),
        vec![Arc::new(CoreDeterministicExtractor)],
    );
    let first = ingest_demo_bytes(
        services,
        &case_record.id,
        "demo/notice.txt",
        b"received_date: 2026-08-12\nresponse_required: true\nmonthly_amount: $1,427.00\n",
    )?;
    let first_extraction = pipeline
        .extract(ExtractArtifactRequest {
            case_id: case_record.id.clone(),
            artifact_version_id: first.artifact_version.id.clone(),
            media_type: "text/plain".to_owned(),
            connector: Some("filesystem".to_owned()),
            actor: "demo:deterministic-extractor".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.safe_message)?;
    let second = ingest_demo_bytes(
        services,
        &case_record.id,
        "demo/notice.txt",
        b"received_date: 2026-08-12\nresponse_required: true\nmonthly_amount: $1,511.00\n",
    )?;
    let second_extraction = pipeline
        .extract(ExtractArtifactRequest {
            case_id: case_record.id.clone(),
            artifact_version_id: second.artifact_version.id.clone(),
            media_type: "text/plain".to_owned(),
            connector: Some("filesystem".to_owned()),
            actor: "demo:deterministic-extractor".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.safe_message)?;
    for claim in first_extraction.claims.iter().filter(|claim| {
        matches!(
            claim.predicate.as_str(),
            "received_date" | "response_required"
        )
    }) {
        services
            .evidence
            .review_claim(ReviewClaimRequest {
                claim_id: claim.id.clone(),
                decision: ReviewDecision::Verified,
                actor: "demo:reviewer".to_owned(),
                rationale: Some("Verified against invented source".to_owned()),
                correlation_id: None,
            })
            .map_err(|error| error.to_string())?;
    }
    let amount_claim = first_extraction
        .claims
        .iter()
        .find(|claim| claim.predicate == "monthly_amount")
        .ok_or_else(|| "demo amount claim was not extracted".to_owned())?;
    let correction = services
        .evidence
        .correct_claim(CorrectClaimRequest {
            claim_id: amount_claim.id.clone(),
            corrected_original_representation: "$1,427.50".to_owned(),
            corrected_value: KnowledgeValue::Known(MaterialValue::Text("$1,427.50".to_owned())),
            corrected_temporal: None,
            actor: "demo:reviewer".to_owned(),
            rationale: Some("Invented correction for the demonstration".to_owned()),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())?;
    let package = SampleAdministrativeCase;
    let contribution = package
        .rules()
        .into_iter()
        .next()
        .ok_or_else(|| "sample package has no rule".to_owned())?;
    let rule_version = services
        .rules
        .register_rule(RegisterRuleRequest {
            package_id: package.package_id().to_owned(),
            stable_key: contribution.stable_key.to_owned(),
            title: contribution.title.to_owned(),
            version: contribution.version,
            definition: contribution.definition,
            effective_from: None,
            effective_until: None,
            actor: "demo:sample-package".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())?;
    let workflow = services
        .rules
        .evaluate(EvaluateRuleRequest {
            case_id: case_record.id.clone(),
            rule_version_id: rule_version.id,
            actor: "demo:rules".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())?;
    let answer = services
        .rules
        .query(&case_record.id, "What deadlines and what must happen next?")
        .map_err(|error| error.to_string())?;
    let contradictions = services
        .evidence
        .list_contradictions(&case_record.id)
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "case": case_record,
        "artifact_versions": [first.artifact_version, second.artifact_version],
        "claims_created": first_extraction.claims.len() + second_extraction.claims.len(),
        "contradictions": contradictions,
        "correction": correction,
        "rule_evaluation": workflow.evaluation,
        "obligation": workflow.obligation,
        "deadline": workflow.deadline,
        "task": workflow.task,
        "grounded_answer": answer,
        "model_provider_used": false
    }))
}

fn ingest_demo_bytes(
    services: &Services,
    case_id: &RecordId,
    source_key: &str,
    bytes: &[u8],
) -> Result<casegraph_application::IngestionResult, String> {
    services
        .evidence
        .ingest_bytes(IngestBytesRequest {
            case_id: case_id.clone(),
            source_key: source_key.to_owned(),
            connector: "filesystem".to_owned(),
            locator: source_key.to_owned(),
            external_record_id: None,
            endpoint: None,
            source_revision: None,
            media_type: "text/plain".to_owned(),
            original_filename: Some("notice.txt".to_owned()),
            received_at: None,
            bytes: bytes.to_vec(),
            actor: "demo:runner".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| error.to_string())
}

fn media_type(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => "application/octet-stream",
    }
    .to_owned()
}

fn id(value: &str) -> Result<RecordId, String> {
    RecordId::parse(value).map_err(|error| error.to_string())
}

fn to_value(value: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|_| "could not serialize command result".to_owned())
}

fn output(value: Value) {
    println!("{value}");
}

fn help() -> &'static str {
    "Casegraph foundation CLI\n\n\
     casegraph init\n\
     casegraph case create <title>\n\
     casegraph ingest <path> --case <case-id>\n\
     casegraph artifacts list --case <case-id>\n\
     casegraph claims list --case <case-id>\n\
     casegraph contradictions list --case <case-id>\n\
     casegraph query <case-id> <question>\n\
     casegraph verify <claim-id>\n\
     casegraph correct <claim-id> <corrected-text>\n\
     casegraph demo\n\
     casegraph serve\n\
     casegraph test"
}

#[cfg(test)]
mod tests {
    use super::{Services, media_type};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn cli_demo_exercises_complete_shared_service_flow() {
        let root = TestDirectory::new("casegraph-cli-demo");
        let services = Services::open(root.path(), 1024 * 1024).expect("services");
        let result = super::run_demo(&services).expect("demo");
        assert_eq!(result["model_provider_used"], false);
        assert_eq!(result["grounded_answer"]["mode"], "established");
        assert!(
            result["contradictions"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(result["task"].is_object());
    }

    #[test]
    fn media_types_are_bounded_and_unknown_is_not_guessed() {
        assert_eq!(media_type(std::path::Path::new("fixture.csv")), "text/csv");
        assert_eq!(
            media_type(std::path::Path::new("fixture.exe")),
            "application/octet-stream"
        );
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
}
