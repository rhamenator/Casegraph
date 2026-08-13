use casegraph_application::{ArtifactFormat, DeterministicExtractor};
use casegraph_domain::{KnowledgeValue, MaterialValue};
use casegraph_infrastructure::CoreDeterministicExtractor;

struct Expected<'a> {
    format: ArtifactFormat,
    bytes: &'a [u8],
    field_count: usize,
    expected_predicates: &'a [&'a str],
}

#[test]
fn deterministic_fixture_evaluation_reports_separate_quality_metrics() {
    let fixtures = [
        Expected {
            format: ArtifactFormat::PlainText,
            bytes: include_bytes!("../../../fixtures/evaluation/text/simple_record.txt"),
            field_count: 4,
            expected_predicates: &[
                "name",
                "received_date",
                "monthly_amount",
                "response_required",
            ],
        },
        Expected {
            format: ArtifactFormat::Json,
            bytes: include_bytes!("../../../fixtures/evaluation/json/simple_record.json"),
            field_count: 3,
            expected_predicates: &["active", "count", "received_date"],
        },
        Expected {
            format: ArtifactFormat::Csv,
            bytes: include_bytes!("../../../fixtures/evaluation/csv/simple_records.csv"),
            field_count: 6,
            expected_predicates: &["name", "monthly_amount", "received_date"],
        },
    ];
    let mut expected_fields = 0_usize;
    let mut extracted_fields = 0_usize;
    let mut correct_predicates = 0_usize;
    let mut normalized_values = 0_usize;
    let mut complete_locations = 0_usize;
    let mut unsupported_claims = 0_usize;
    for fixture in fixtures {
        let fields = CoreDeterministicExtractor
            .extract(fixture.format, fixture.bytes)
            .expect("fixture extraction");
        expected_fields += fixture.field_count;
        extracted_fields += fields.len();
        for field in fields {
            if fixture
                .expected_predicates
                .contains(&field.predicate.as_str())
            {
                correct_predicates += 1;
            }
            if matches!(field.normalized_value, KnowledgeValue::Known(_)) {
                normalized_values += 1;
            }
            if field.location.source_field.is_some()
                && (field.location.text_span_start.is_some()
                    || field.location.row_number.is_some()
                    || fixture.format == ArtifactFormat::Json)
            {
                complete_locations += 1;
            }
            if field.predicate.trim().is_empty()
                || matches!(
                    field.normalized_value,
                    KnowledgeValue::Known(MaterialValue::Text(ref value)) if value.trim().is_empty()
                )
            {
                unsupported_claims += 1;
            }
        }
    }
    assert_eq!(extracted_fields, expected_fields, "extraction correctness");
    assert_eq!(
        correct_predicates, expected_fields,
        "semantic field correctness"
    );
    assert_eq!(
        normalized_values, expected_fields,
        "normalization correctness"
    );
    assert_eq!(
        complete_locations, expected_fields,
        "provenance completeness"
    );
    assert_eq!(unsupported_claims, 0, "unsupported-claim rate");
}

#[test]
fn malformed_fixture_is_preserved_as_a_regression_case() {
    let error = CoreDeterministicExtractor
        .extract(
            ArtifactFormat::Csv,
            include_bytes!("../../../fixtures/evaluation/malformed/uneven.csv"),
        )
        .expect_err("uneven row must remain rejected");
    assert_eq!(
        error.kind,
        casegraph_application::PipelineFailureKind::MalformedInput
    );
}
