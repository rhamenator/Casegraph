#![forbid(unsafe_code)]

//! Versioned HTTP adapter over shared application services.

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use casegraph_application::{
    AppError, CasegraphService, CorrectClaimRequest, CreateCaseRequest, ErrorKind,
    EvaluateRuleRequest, ExtractArtifactRequest, ExtractionPipeline, IngestBytesRequest,
    RegisterRuleRequest, ReviewClaimRequest, RuleWorkflowService,
};
use casegraph_domain::{KnowledgeValue, RecordId, ReviewDecision};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

/// Versioned API prefix reserved by the foundation.
pub const API_PREFIX: &str = "/api/v1";

/// Shared adapter state; services contain all business behavior.
#[derive(Clone)]
pub struct ApiState {
    pub evidence: CasegraphService,
    pub extraction: ExtractionPipeline,
    pub rules: RuleWorkflowService,
    pub max_artifact_bytes: u64,
}

/// Construct the implemented versioned routes.
pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/openapi.json", get(openapi))
        .route("/api/v1/cases", post(create_case))
        .route("/api/v1/cases/{case_id}", get(get_case))
        .route(
            "/api/v1/cases/{case_id}/artifacts",
            get(list_artifacts).post(ingest_artifact),
        )
        .route("/api/v1/cases/{case_id}/claims", get(list_claims))
        .route(
            "/api/v1/cases/{case_id}/contradictions",
            get(list_contradictions),
        )
        .route("/api/v1/cases/{case_id}/query", post(query_case))
        .route("/api/v1/cases/{case_id}/tasks", get(list_tasks))
        .route("/api/v1/artifacts/{version_id}", get(get_artifact))
        .route(
            "/api/v1/artifacts/{version_id}/extract",
            post(extract_artifact),
        )
        .route("/api/v1/provenance/{provenance_id}", get(get_provenance))
        .route("/api/v1/claims/{claim_id}/verify", post(verify_claim))
        .route("/api/v1/claims/{claim_id}/correct", post(correct_claim))
        .route("/api/v1/rules", post(register_rule))
        .route("/api/v1/rules/evaluate", post(evaluate_rule))
        .layer(DefaultBodyLimit::max(
            usize::try_from(state.max_artifact_bytes).unwrap_or(usize::MAX),
        ))
        .with_state(Arc::new(state))
}

/// Serve until the listener fails or the task is cancelled.
pub async fn serve(listener: tokio::net::TcpListener, state: ApiState) -> std::io::Result<()> {
    axum::serve(listener, router(state)).await
}

async fn liveness() -> Json<Value> {
    Json(json!({"status":"live"}))
}

async fn readiness() -> Json<Value> {
    // Composition opens/migrates persistence and artifact storage before ApiState exists. Optional
    // model providers are deliberately excluded from deterministic readiness.
    Json(json!({"status":"ready","model_provider":"optional"}))
}

async fn openapi() -> Response {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        include_str!("openapi.json"),
    )
        .into_response()
}

async fn create_case(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<CreateCaseRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let case_record = state.evidence.create_case(request)?;
    Ok((StatusCode::CREATED, Json(to_value(case_record)?)))
}

async fn get_case(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.evidence.get_case(&id(case_id)?)?)?))
}

async fn list_claims(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.evidence.list_claims(&id(case_id)?)?)?))
}

async fn list_artifacts(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(
        state.evidence.list_artifact_versions(&id(case_id)?)?,
    )?))
}

async fn ingest_artifact(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let source_key = header(&headers, "x-casegraph-source-key")?;
    let filename = header(&headers, "x-casegraph-filename")?;
    let media_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let result = state.evidence.ingest_bytes(IngestBytesRequest {
        case_id: id(case_id)?,
        source_key: source_key.clone(),
        connector: "api".to_owned(),
        locator: source_key,
        external_record_id: None,
        endpoint: None,
        source_revision: None,
        media_type,
        original_filename: Some(filename),
        received_at: None,
        bytes: body.to_vec(),
        actor: "api:client".to_owned(),
        correlation_id: None,
    })?;
    Ok((StatusCode::CREATED, Json(to_value(result)?)))
}

async fn list_contradictions(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(
        state.evidence.list_contradictions(&id(case_id)?)?,
    )?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryRequest {
    question: String,
}

async fn query_case(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
    Json(request): Json<QueryRequest>,
) -> Result<Json<Value>, ApiError> {
    if request.question.trim().is_empty() || request.question.len() > 2_000 {
        return Err(ApiError::invalid("question must contain 1-2000 bytes"));
    }
    Ok(Json(to_value(
        state.rules.query(&id(case_id)?, &request.question)?,
    )?))
}

async fn list_tasks(
    State(state): State<Arc<ApiState>>,
    Path(case_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.rules.list_workflow(&id(case_id)?)?)?))
}

async fn get_artifact(
    State(state): State<Arc<ApiState>>,
    Path(version_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(
        state.evidence.get_artifact_version(&id(version_id)?)?,
    )?))
}

async fn get_provenance(
    State(state): State<Arc<ApiState>>,
    Path(provenance_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(
        state.evidence.get_provenance(&id(provenance_id)?)?,
    )?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractRequest {
    case_id: RecordId,
    media_type: String,
    connector: Option<String>,
}

async fn extract_artifact(
    State(state): State<Arc<ApiState>>,
    Path(version_id): Path<String>,
    Json(request): Json<ExtractRequest>,
) -> Result<Json<Value>, ApiError> {
    let result = state
        .extraction
        .extract(ExtractArtifactRequest {
            case_id: request.case_id,
            artifact_version_id: id(version_id)?,
            media_type: request.media_type,
            connector: request.connector,
            actor: "api:deterministic-extractor".to_owned(),
            correlation_id: None,
        })
        .map_err(|error| ApiError::invalid(&error.safe_message))?;
    Ok(Json(to_value(result)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyRequest {
    actor: String,
    rationale: Option<String>,
}

async fn verify_claim(
    State(state): State<Arc<ApiState>>,
    Path(claim_id): Path<String>,
    Json(request): Json<VerifyRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.evidence.review_claim(
        ReviewClaimRequest {
            claim_id: id(claim_id)?,
            decision: ReviewDecision::Verified,
            actor: request.actor,
            rationale: request.rationale,
            correlation_id: None,
        },
    )?)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CorrectRequest {
    actor: String,
    corrected_original_representation: String,
    corrected_value: KnowledgeValue,
    rationale: Option<String>,
}

async fn correct_claim(
    State(state): State<Arc<ApiState>>,
    Path(claim_id): Path<String>,
    Json(request): Json<CorrectRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.evidence.correct_claim(
        CorrectClaimRequest {
            claim_id: id(claim_id)?,
            corrected_original_representation: request.corrected_original_representation,
            corrected_value: request.corrected_value,
            corrected_temporal: None,
            actor: request.actor,
            rationale: request.rationale,
            correlation_id: None,
        },
    )?)?))
}

async fn register_rule(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<RegisterRuleRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(to_value(state.rules.register_rule(request)?)?),
    ))
}

async fn evaluate_rule(
    State(state): State<Arc<ApiState>>,
    Json(request): Json<EvaluateRuleRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(to_value(state.rules.evaluate(request)?)?))
}

fn id(raw: String) -> Result<RecordId, ApiError> {
    RecordId::parse(raw).map_err(|error| ApiError::invalid(&error.to_string()))
}

fn header(headers: &HeaderMap, name: &'static str) -> Result<String, ApiError> {
    let value = headers
        .get(name)
        .ok_or_else(|| ApiError::invalid(&format!("missing required header {name}")))?
        .to_str()
        .map_err(|_| ApiError::invalid(&format!("header {name} is not valid ASCII")))?;
    if value.is_empty() || value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(ApiError::invalid(&format!("header {name} is invalid")));
    }
    Ok(value.to_owned())
}

fn to_value(value: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(|_| ApiError {
        status: StatusCode::INTERNAL_SERVER_ERROR,
        code: "internal",
        message: "response serialization failed".to_owned(),
    })
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn invalid(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_input",
            message: message.to_owned(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let (status, code) = match error.kind() {
            ErrorKind::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_input"),
            ErrorKind::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ErrorKind::Conflict => (StatusCode::CONFLICT, "conflict"),
            ErrorKind::Unsupported => (StatusCode::NOT_IMPLEMENTED, "unsupported"),
            ErrorKind::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            ErrorKind::Storage | ErrorKind::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            "internal operation failed".to_owned()
        } else {
            error.safe_message().to_owned()
        };
        Self {
            status,
            code,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": {"code": self.code, "message": self.message}
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiState, serve};
    use casegraph_application::{CasegraphService, ExtractionPipeline, RuleWorkflowService};
    use casegraph_infrastructure::{
        CoreDeterministicExtractor, FilesystemArtifactStore, Sha256IdGenerator,
        SqliteEvidenceRepository, SystemClock,
    };
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    #[tokio::test(flavor = "multi_thread")]
    async fn real_http_health_and_case_creation_use_shared_services() {
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "casegraph-api-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("test root");
        let repository = Arc::new(
            SqliteEvidenceRepository::open(root.join("casegraph.db")).expect("repository"),
        );
        let artifacts =
            Arc::new(FilesystemArtifactStore::new(root.join("artifacts")).expect("artifact store"));
        let clock = Arc::new(SystemClock);
        let ids = Arc::new(Sha256IdGenerator::default());
        let evidence = CasegraphService::new(
            repository.clone(),
            artifacts,
            clock.clone(),
            ids.clone(),
            1024,
        )
        .expect("service");
        let rules = RuleWorkflowService::new(repository, clock, ids);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let address = listener.local_addr().expect("address");
        let task = tokio::spawn(serve(
            listener,
            ApiState {
                extraction: ExtractionPipeline::new(
                    evidence.clone(),
                    vec![Arc::new(CoreDeterministicExtractor)],
                ),
                evidence,
                rules,
                max_artifact_bytes: 1024,
            },
        ));

        let health = request(
            address,
            "GET /health/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert!(health.starts_with("HTTP/1.1 200"));
        assert!(health.contains("\"status\":\"ready\""));

        let body = r#"{"title":"Synthetic API Case","actor":"api-test","correlation_id":null}"#;
        let create = request(
            address,
            &format!(
                "POST /api/v1/cases HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            ),
        );
        assert!(create.starts_with("HTTP/1.1 201"), "{create}");
        assert!(create.contains("Synthetic API Case"));
        let created: serde_json::Value =
            serde_json::from_str(response_body(&create)).expect("case response JSON");
        let case_id = created["id"].as_str().expect("case id");

        let artifact_body = "amount: $1,427.00\n";
        let upload = request(
            address,
            &format!(
                "POST /api/v1/cases/{case_id}/artifacts HTTP/1.1\r\nHost: localhost\r\nContent-Type: text/plain\r\nx-casegraph-source-key: api/fixture.txt\r\nx-casegraph-filename: fixture.txt\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                artifact_body.len(),
                artifact_body
            ),
        );
        assert!(upload.starts_with("HTTP/1.1 201"), "{upload}");
        assert!(upload.contains("content_sha256"));
        let listed = request(
            address,
            &format!(
                "GET /api/v1/cases/{case_id}/artifacts HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(listed.starts_with("HTTP/1.1 200"));
        assert!(listed.contains("content_sha256"));

        task.abort();
        fs::remove_dir_all(root).ok();
    }

    fn request(address: std::net::SocketAddr, request: &str) -> String {
        let mut stream = TcpStream::connect(address).expect("connect");
        stream.write_all(request.as_bytes()).expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        response
    }

    fn response_body(response: &str) -> &str {
        response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .expect("HTTP response body")
    }

    #[test]
    fn checked_in_openapi_is_valid_and_describes_implemented_core_routes() {
        let document: serde_json::Value =
            serde_json::from_str(include_str!("openapi.json")).expect("valid OpenAPI JSON");
        let paths = document["paths"].as_object().expect("paths object");
        for path in [
            "/api/v1/cases",
            "/api/v1/cases/{case_id}/artifacts",
            "/api/v1/artifacts/{version_id}/extract",
            "/api/v1/claims/{claim_id}/correct",
            "/api/v1/rules/evaluate",
        ] {
            assert!(paths.contains_key(path), "OpenAPI is missing {path}");
        }
    }
}
