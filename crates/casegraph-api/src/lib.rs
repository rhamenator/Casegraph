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
    use super::{ApiError, ApiState, serve, to_value};
    use axum::response::IntoResponse;
    use casegraph_application::{
        AppError, CasegraphService, ErrorKind, ExtractionPipeline, RuleWorkflowService,
    };
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
        let live = request(
            address,
            "GET /health/live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert_status(&live, 200);
        let openapi = request(
            address,
            "GET /openapi.json HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        );
        assert_status(&openapi, 200);
        assert!(openapi.contains("\"openapi\""));

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
        assert_status(&get(address, &format!("/api/v1/cases/{case_id}")), 200);

        let missing_header = raw_upload(
            address,
            case_id,
            "Content-Type: text/plain\r\n",
            "amount: $1.00\n",
        );
        assert_status(&missing_header, 400);
        assert!(missing_header.contains("missing required header"));

        let artifact_body =
            "received_date: 2026-08-12\nresponse_required: true\namount: $1,427.00\n";
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
        let uploaded = json_body(&upload);
        let version_id = uploaded["artifact_version"]["id"]
            .as_str()
            .expect("artifact version id");
        assert_status(
            &get(address, &format!("/api/v1/artifacts/{version_id}")),
            200,
        );
        let listed = request(
            address,
            &format!(
                "GET /api/v1/cases/{case_id}/artifacts HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(listed.starts_with("HTTP/1.1 200"));
        assert!(listed.contains("content_sha256"));

        let extraction = json_request(
            address,
            "POST",
            &format!("/api/v1/artifacts/{version_id}/extract"),
            &format!(r#"{{"case_id":"{case_id}","media_type":"text/plain","connector":"api"}}"#),
        );
        assert_status(&extraction, 200);
        let extracted = json_body(&extraction);
        let claims = extracted["claims"].as_array().expect("claims array");
        assert_eq!(claims.len(), 3);
        let received = claims
            .iter()
            .find(|claim| claim["predicate"] == "received_date")
            .expect("received date claim");
        let response_required = claims
            .iter()
            .find(|claim| claim["predicate"] == "response_required")
            .expect("response required claim");
        let amount = claims
            .iter()
            .find(|claim| claim["predicate"] == "amount")
            .expect("amount claim");

        let claims_response = get(address, &format!("/api/v1/cases/{case_id}/claims"));
        assert_status(&claims_response, 200);
        assert_eq!(
            json_body(&claims_response).as_array().map(Vec::len),
            Some(3)
        );
        let provenance_id = amount["primary_provenance_id"]
            .as_str()
            .expect("provenance id");
        assert_status(
            &get(address, &format!("/api/v1/provenance/{provenance_id}")),
            200,
        );

        for claim in [received, response_required] {
            let claim_id = claim["id"].as_str().expect("claim id");
            let verification = json_request(
                address,
                "POST",
                &format!("/api/v1/claims/{claim_id}/verify"),
                r#"{"actor":"api-reviewer","rationale":"verified fixture"}"#,
            );
            assert_status(&verification, 200);
        }
        let amount_id = amount["id"].as_str().expect("amount id");
        let correction = json_request(
            address,
            "POST",
            &format!("/api/v1/claims/{amount_id}/correct"),
            r#"{"actor":"api-reviewer","corrected_original_representation":"$1,427.50","corrected_value":{"knowledge":"known","value":{"type":"text","value":"$1,427.50"}},"rationale":"fixture correction"}"#,
        );
        assert_status(&correction, 200);

        let register = json_request(
            address,
            "POST",
            "/api/v1/rules",
            r#"{"package_id":"api-fixture","stable_key":"response","title":"Response rule","version":1,"definition":{"all":[{"subject_key":"document","predicate":"response_required","expected":{"knowledge":"known","value":{"type":"boolean","value":true}}}],"effect":{"obligation_kind":"respond","obligation_description":"Respond to the synthetic record.","deadline_anchor_predicate":"received_date","deadline_days_after":10,"task_title":"Prepare response"}},"effective_from":null,"effective_until":null,"actor":"api-test","correlation_id":null}"#,
        );
        assert_status(&register, 201);
        let rule_id = json_body(&register)["id"]
            .as_str()
            .expect("rule version id")
            .to_owned();
        let evaluate = json_request(
            address,
            "POST",
            "/api/v1/rules/evaluate",
            &format!(
                r#"{{"case_id":"{case_id}","rule_version_id":"{rule_id}","actor":"api-test","correlation_id":null}}"#
            ),
        );
        assert_status(&evaluate, 200);
        assert_eq!(json_body(&evaluate)["evaluation"]["result"], "satisfied");
        assert_status(
            &get(address, &format!("/api/v1/cases/{case_id}/tasks")),
            200,
        );

        let query = json_request(
            address,
            "POST",
            &format!("/api/v1/cases/{case_id}/query"),
            r#"{"question":"What must happen next?"}"#,
        );
        assert_status(&query, 200);
        assert_eq!(json_body(&query)["mode"], "established");
        let empty_query = json_request(
            address,
            "POST",
            &format!("/api/v1/cases/{case_id}/query"),
            r#"{"question":" "}"#,
        );
        assert_status(&empty_query, 400);

        let second_upload = raw_upload(
            address,
            case_id,
            "Content-Type: text/plain\r\nx-casegraph-source-key: api/fixture.txt\r\nx-casegraph-filename: fixture.txt\r\n",
            "received_date: 2026-08-12\nresponse_required: true\namount: $1,500.00\n",
        );
        assert_status(&second_upload, 201);
        let second_version = json_body(&second_upload)["artifact_version"]["id"]
            .as_str()
            .expect("second version")
            .to_owned();
        assert_status(
            &json_request(
                address,
                "POST",
                &format!("/api/v1/artifacts/{second_version}/extract"),
                &format!(r#"{{"case_id":"{case_id}","media_type":"text/plain","connector":null}}"#),
            ),
            200,
        );
        let contradictions = get(address, &format!("/api/v1/cases/{case_id}/contradictions"));
        assert_status(&contradictions, 200);
        assert!(
            !json_body(&contradictions)
                .as_array()
                .expect("contradictions")
                .is_empty()
        );

        assert_status(&get(address, "/api/v1/cases/not-a-valid-id!"), 400);
        assert_status(&get(address, "/api/v1/cases/case_missing"), 404);

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

    fn get(address: std::net::SocketAddr, path: &str) -> String {
        request(
            address,
            &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        )
    }

    fn json_request(address: std::net::SocketAddr, method: &str, path: &str, body: &str) -> String {
        request(
            address,
            &format!(
                "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    fn raw_upload(
        address: std::net::SocketAddr,
        case_id: &str,
        headers: &str,
        body: &str,
    ) -> String {
        request(
            address,
            &format!(
                "POST /api/v1/cases/{case_id}/artifacts HTTP/1.1\r\nHost: localhost\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
    }

    fn assert_status(response: &str, status: u16) {
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "unexpected response: {response}"
        );
    }

    fn json_body(response: &str) -> serde_json::Value {
        serde_json::from_str(response_body(response)).expect("JSON response")
    }

    #[test]
    fn api_error_mapping_is_stable_and_internal_details_are_hidden() {
        for (kind, expected) in [
            (ErrorKind::InvalidInput, 400),
            (ErrorKind::NotFound, 404),
            (ErrorKind::Conflict, 409),
            (ErrorKind::Unsupported, 501),
            (ErrorKind::TooLarge, 413),
            (ErrorKind::Storage, 500),
            (ErrorKind::Internal, 500),
        ] {
            let response = ApiError::from(AppError::new(kind, "sensitive detail")).into_response();
            assert_eq!(response.status().as_u16(), expected);
        }

        struct SerializationFailure;
        impl serde::Serialize for SerializationFailure {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                Err(serde::ser::Error::custom("intentional"))
            }
        }
        let error = to_value(SerializationFailure).expect_err("serialization must fail");
        assert_eq!(error.status.as_u16(), 500);
        assert_eq!(error.message, "response serialization failed");
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
