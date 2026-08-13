//! Optional provider-neutral reasoning boundary. Raw model output is always untrusted.

use casegraph_domain::{KnowledgeValue, RecordId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Deployment privacy policy for optional model invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderPolicy {
    Disabled,
    LocalOnly,
    AllowListedRemote,
}

/// Provider locality required to enforce privacy policy before invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderLocality {
    Local,
    Remote,
}

/// Provider metadata recorded whenever a model contributes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub provider: String,
    pub model: String,
    pub version: String,
    pub configuration_json: String,
}

/// Minimal raw provider interface, suitable for hosted or local adapters.
pub trait RawReasoningProvider: Send + Sync {
    fn identity(&self) -> ModelIdentity;
    fn locality(&self) -> ProviderLocality;
    fn invoke(&self, redacted_context: &str) -> Result<String, ReasoningError>;
}

/// Strict model-produced claim schema. Unknown fields are rejected.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterpretedClaim {
    pub subject_key: String,
    pub predicate: String,
    pub original_value: String,
    pub normalized_value: KnowledgeValue,
    pub text_span_start: Option<u64>,
    pub text_span_end: Option<u64>,
}

/// Strict top-level output schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReasoningOutput {
    claims: Vec<InterpretedClaim>,
}

/// Safe failure category for optional reasoning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningFailureKind {
    Disabled,
    PolicyDenied,
    ProviderUnavailable,
    MalformedOutput,
    ValidationRejected,
}

/// Safe failure record: it deliberately excludes prompts and provider output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReasoningFailure {
    pub id: RecordId,
    pub correlation_id: RecordId,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub kind: ReasoningFailureKind,
    pub safe_message: String,
}

/// Failure persistence/logging boundary.
pub trait ReasoningFailureSink: Send + Sync {
    fn record(&self, failure: &ReasoningFailure);
}

/// Optional reasoning failure returned to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReasoningError {
    pub kind: ReasoningFailureKind,
    pub safe_message: String,
}

impl ReasoningError {
    pub fn new(kind: ReasoningFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            safe_message: message.into(),
        }
    }
}

/// Policy-enforcing, schema-validating wrapper over a raw model adapter.
pub struct ReasoningGateway {
    policy: ProviderPolicy,
    provider: Option<Arc<dyn RawReasoningProvider>>,
    failures: Arc<dyn ReasoningFailureSink>,
}

impl ReasoningGateway {
    pub fn new(
        policy: ProviderPolicy,
        provider: Option<Arc<dyn RawReasoningProvider>>,
        failures: Arc<dyn ReasoningFailureSink>,
    ) -> Self {
        Self {
            policy,
            provider,
            failures,
        }
    }

    /// Invoke only when policy permits, then reject malformed or semantically invalid output.
    pub fn interpret(
        &self,
        redacted_context: &str,
        failure_id: RecordId,
        correlation_id: RecordId,
    ) -> Result<Vec<InterpretedClaim>, ReasoningError> {
        let provider = self.provider.as_ref().ok_or_else(|| {
            self.fail(
                failure_id.clone(),
                correlation_id.clone(),
                None,
                ReasoningFailureKind::Disabled,
                "no reasoning provider is configured",
            )
        })?;
        let identity = provider.identity();
        if self.policy == ProviderPolicy::Disabled
            || (self.policy == ProviderPolicy::LocalOnly
                && provider.locality() == ProviderLocality::Remote)
        {
            return Err(self.fail(
                failure_id,
                correlation_id,
                Some(&identity),
                ReasoningFailureKind::PolicyDenied,
                "reasoning provider invocation is denied by deployment policy",
            ));
        }
        let raw = provider.invoke(redacted_context).map_err(|_| {
            self.fail(
                failure_id.clone(),
                correlation_id.clone(),
                Some(&identity),
                ReasoningFailureKind::ProviderUnavailable,
                "reasoning provider invocation failed",
            )
        })?;
        let output = serde_json::from_str::<ReasoningOutput>(&raw).map_err(|_| {
            self.fail(
                failure_id.clone(),
                correlation_id.clone(),
                Some(&identity),
                ReasoningFailureKind::MalformedOutput,
                "reasoning provider output did not match the required schema",
            )
        })?;
        for claim in &output.claims {
            if claim.subject_key.trim().is_empty()
                || claim.predicate.trim().is_empty()
                || matches!((claim.text_span_start, claim.text_span_end), (Some(start), Some(end)) if end < start)
            {
                return Err(self.fail(
                    failure_id,
                    correlation_id,
                    Some(&identity),
                    ReasoningFailureKind::ValidationRejected,
                    "reasoning provider output violated a semantic invariant",
                ));
            }
        }
        Ok(output.claims)
    }

    fn fail(
        &self,
        id: RecordId,
        correlation_id: RecordId,
        identity: Option<&ModelIdentity>,
        kind: ReasoningFailureKind,
        safe_message: &str,
    ) -> ReasoningError {
        self.failures.record(&ReasoningFailure {
            id,
            correlation_id,
            provider: identity.map(|value| value.provider.clone()),
            model: identity.map(|value| value.model.clone()),
            kind,
            safe_message: safe_message.to_owned(),
        });
        ReasoningError::new(kind, safe_message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeProvider {
        output: String,
        locality: ProviderLocality,
    }

    impl RawReasoningProvider for FakeProvider {
        fn identity(&self) -> ModelIdentity {
            ModelIdentity {
                provider: "synthetic".to_owned(),
                model: "fixture".to_owned(),
                version: "1".to_owned(),
                configuration_json: "{}".to_owned(),
            }
        }

        fn locality(&self) -> ProviderLocality {
            self.locality
        }

        fn invoke(&self, _redacted_context: &str) -> Result<String, ReasoningError> {
            Ok(self.output.clone())
        }
    }

    #[derive(Default)]
    struct Failures(Mutex<Vec<ReasoningFailure>>);

    impl ReasoningFailureSink for Failures {
        fn record(&self, failure: &ReasoningFailure) {
            self.0.lock().expect("failure lock").push(failure.clone());
        }
    }

    fn id(value: &str) -> RecordId {
        RecordId::parse(value).expect("fixture id")
    }

    #[test]
    fn core_operates_with_provider_disabled_and_attempt_is_explicit() {
        let failures = Arc::new(Failures::default());
        let gateway = ReasoningGateway::new(ProviderPolicy::Disabled, None, failures.clone());
        let error = gateway
            .interpret("redacted", id("failure_1"), id("correlation_1"))
            .expect_err("disabled provider cannot run");
        assert_eq!(error.kind, ReasoningFailureKind::Disabled);
        assert_eq!(failures.0.lock().expect("failure lock").len(), 1);
    }

    #[test]
    fn malformed_output_is_rejected_and_recorded_without_raw_output() {
        let secret_output = "not-json SECRET-SOURCE-CONTENT";
        let failures = Arc::new(Failures::default());
        let gateway = ReasoningGateway::new(
            ProviderPolicy::LocalOnly,
            Some(Arc::new(FakeProvider {
                output: secret_output.to_owned(),
                locality: ProviderLocality::Local,
            })),
            failures.clone(),
        );
        let error = gateway
            .interpret("redacted", id("failure_1"), id("correlation_1"))
            .expect_err("malformed output must fail");
        assert_eq!(error.kind, ReasoningFailureKind::MalformedOutput);
        let recorded = failures.0.lock().expect("failure lock");
        assert_eq!(recorded.len(), 1);
        assert!(!recorded[0].safe_message.contains(secret_output));
        assert!(!recorded[0].safe_message.contains("SECRET"));
    }

    #[test]
    fn remote_provider_is_denied_under_local_only_policy_before_invocation() {
        let failures = Arc::new(Failures::default());
        let gateway = ReasoningGateway::new(
            ProviderPolicy::LocalOnly,
            Some(Arc::new(FakeProvider {
                output: r#"{"claims":[]}"#.to_owned(),
                locality: ProviderLocality::Remote,
            })),
            failures,
        );
        let error = gateway
            .interpret("redacted", id("failure_1"), id("correlation_1"))
            .expect_err("remote invocation must be blocked");
        assert_eq!(error.kind, ReasoningFailureKind::PolicyDenied);
    }
}
