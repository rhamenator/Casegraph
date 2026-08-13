#![forbid(unsafe_code)]

//! Deliberately artificial package used only to prove domain extension isolation.

use casegraph_application::{DomainPackage, RuleContribution};
use casegraph_domain::{
    KnowledgeValue, MaterialValue, RuleCondition, RuleDefinition, WorkflowEffect,
};

/// Stable package identifier; this is deliberately not a production vertical.
pub const PACKAGE_ID: &str = "sample-administrative-case";

/// Artificial package implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SampleAdministrativeCase;

impl DomainPackage for SampleAdministrativeCase {
    fn package_id(&self) -> &'static str {
        PACKAGE_ID
    }

    fn version(&self) -> &'static str {
        "1"
    }

    fn rules(&self) -> Vec<RuleContribution> {
        vec![RuleContribution {
            stable_key: "synthetic-response-required",
            title: "Synthetic response requirement",
            version: 1,
            definition: RuleDefinition {
                all: vec![RuleCondition {
                    subject_key: "document".to_owned(),
                    predicate: "response_required".to_owned(),
                    expected: KnowledgeValue::Known(MaterialValue::Boolean(true)),
                }],
                effect: WorkflowEffect {
                    obligation_kind: "synthetic_response".to_owned(),
                    obligation_description:
                        "Provide the invented response for the synthetic administrative record."
                            .to_owned(),
                    deadline_anchor_predicate: "received_date".to_owned(),
                    deadline_days_after: 10,
                    task_title: "Prepare synthetic response".to_owned(),
                },
            },
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::SampleAdministrativeCase;
    use casegraph_application::DomainPackage;

    #[test]
    fn artificial_package_contributes_through_the_extension_contract() {
        let package = SampleAdministrativeCase;
        assert_eq!(package.rules().len(), 1);
        package.rules()[0]
            .definition
            .validate()
            .expect("sample rule validates");
    }
}
