//! Dependency-light deterministic extraction for plain text, flat JSON, and CSV.

use casegraph_application::{
    ArtifactFormat, DeterministicExtractor, ExtractedField, ExtractedLocation, PipelineError,
    PipelineFailureKind,
};
use casegraph_domain::{Date, Decimal, KnowledgeValue, MaterialValue, Money, TemporalValue};
use serde_json::Value;

/// Core domain-neutral extractor for simple deterministic structures.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreDeterministicExtractor;

impl DeterministicExtractor for CoreDeterministicExtractor {
    fn name(&self) -> &'static str {
        "casegraph.core-deterministic"
    }

    fn version(&self) -> &'static str {
        "1"
    }

    fn supports(&self, format: ArtifactFormat) -> bool {
        matches!(
            format,
            ArtifactFormat::PlainText | ArtifactFormat::Json | ArtifactFormat::Csv
        )
    }

    fn extract(
        &self,
        format: ArtifactFormat,
        bytes: &[u8],
    ) -> Result<Vec<ExtractedField>, PipelineError> {
        let text = std::str::from_utf8(bytes).map_err(|_| {
            PipelineError::new(
                PipelineFailureKind::MalformedInput,
                "deterministic text extraction requires valid UTF-8",
            )
        })?;
        match format {
            ArtifactFormat::PlainText => extract_text(text),
            ArtifactFormat::Json => extract_json(text),
            ArtifactFormat::Csv => extract_csv(text),
        }
    }
}

fn extract_text(text: &str) -> Result<Vec<ExtractedField>, PipelineError> {
    let mut fields = Vec::new();
    let mut offset = 0_usize;
    for (index, line_with_ending) in text.split_inclusive('\n').enumerate() {
        let line = line_with_ending.trim_end_matches(['\r', '\n']);
        if let Some((key, raw_value)) = line.split_once(':') {
            let key = key.trim();
            let value = raw_value.trim();
            if !key.is_empty() && !value.is_empty() {
                let value_start_in_line = line.find(value).ok_or_else(|| {
                    PipelineError::new(
                        PipelineFailureKind::Internal,
                        "could not calculate deterministic text span",
                    )
                })?;
                let start = offset.checked_add(value_start_in_line).ok_or_else(|| {
                    PipelineError::new(
                        PipelineFailureKind::Internal,
                        "text span exceeds supported range",
                    )
                })?;
                let end = start.checked_add(value.len()).ok_or_else(|| {
                    PipelineError::new(
                        PipelineFailureKind::Internal,
                        "text span exceeds supported range",
                    )
                })?;
                fields.push(field(
                    "document",
                    key,
                    value,
                    ExtractedLocation {
                        source_field: Some(key.to_owned()),
                        paragraph_number: Some(u32::try_from(index + 1).map_err(|_| {
                            PipelineError::new(
                                PipelineFailureKind::MalformedInput,
                                "document has too many lines",
                            )
                        })?),
                        text_span_start: Some(u64::try_from(start).map_err(|_| {
                            PipelineError::new(
                                PipelineFailureKind::Internal,
                                "text span exceeds supported range",
                            )
                        })?),
                        text_span_end: Some(u64::try_from(end).map_err(|_| {
                            PipelineError::new(
                                PipelineFailureKind::Internal,
                                "text span exceeds supported range",
                            )
                        })?),
                        row_number: None,
                        column_number: None,
                    },
                )?);
            }
        }
        offset = offset.checked_add(line_with_ending.len()).ok_or_else(|| {
            PipelineError::new(
                PipelineFailureKind::Internal,
                "document offset exceeds supported range",
            )
        })?;
    }
    Ok(fields)
}

fn extract_json(text: &str) -> Result<Vec<ExtractedField>, PipelineError> {
    let value = serde_json::from_str::<Value>(text).map_err(|_| {
        PipelineError::new(
            PipelineFailureKind::MalformedInput,
            "JSON artifact is malformed",
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        PipelineError::new(
            PipelineFailureKind::MalformedInput,
            "JSON deterministic extraction requires a top-level object",
        )
    })?;
    let mut keys = object.keys().collect::<Vec<_>>();
    keys.sort();
    let mut fields = Vec::new();
    for key in keys {
        let value = &object[key];
        if value.is_object() || value.is_array() {
            continue;
        }
        let original = match value {
            Value::String(value) => value.clone(),
            _ => value.to_string(),
        };
        fields.push(field(
            "document",
            key,
            &original,
            ExtractedLocation {
                source_field: Some(key.clone()),
                paragraph_number: None,
                text_span_start: None,
                text_span_end: None,
                row_number: None,
                column_number: None,
            },
        )?);
    }
    Ok(fields)
}

fn extract_csv(text: &str) -> Result<Vec<ExtractedField>, PipelineError> {
    let records = parse_csv(text)?;
    let Some(headers) = records.first() else {
        return Ok(Vec::new());
    };
    if headers.is_empty() || headers.iter().any(|header| header.trim().is_empty()) {
        return Err(PipelineError::new(
            PipelineFailureKind::MalformedInput,
            "CSV header contains an empty field",
        ));
    }
    let mut fields = Vec::new();
    for (row_index, record) in records.iter().enumerate().skip(1) {
        if record.len() != headers.len() {
            return Err(PipelineError::new(
                PipelineFailureKind::MalformedInput,
                "CSV row width does not match the header",
            ));
        }
        for (column_index, (header, value)) in headers.iter().zip(record).enumerate() {
            if value.is_empty() {
                continue;
            }
            fields.push(field(
                &format!("row:{row_index}"),
                header.trim(),
                value,
                ExtractedLocation {
                    source_field: Some(header.trim().to_owned()),
                    paragraph_number: None,
                    text_span_start: None,
                    text_span_end: None,
                    row_number: Some(u32::try_from(row_index).map_err(|_| {
                        PipelineError::new(
                            PipelineFailureKind::MalformedInput,
                            "CSV has too many rows",
                        )
                    })?),
                    column_number: Some(u32::try_from(column_index).map_err(|_| {
                        PipelineError::new(
                            PipelineFailureKind::MalformedInput,
                            "CSV has too many columns",
                        )
                    })?),
                },
            )?);
        }
    }
    Ok(fields)
}

fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, PipelineError> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut characters = text.chars().peekable();
    let mut quoted = false;
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                if field.ends_with('\r') {
                    field.pop();
                }
                record.push(std::mem::take(&mut field));
                if record.iter().any(|value| !value.is_empty()) {
                    records.push(std::mem::take(&mut record));
                } else {
                    record.clear();
                }
            }
            _ => field.push(character),
        }
    }
    if quoted {
        return Err(PipelineError::new(
            PipelineFailureKind::MalformedInput,
            "CSV contains an unterminated quoted field",
        ));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    Ok(records)
}

fn field(
    subject: &str,
    predicate: &str,
    original: &str,
    location: ExtractedLocation,
) -> Result<ExtractedField, PipelineError> {
    let (normalized_value, temporal) = normalize(original)?;
    Ok(ExtractedField {
        subject_key: subject.to_owned(),
        predicate: predicate.to_owned(),
        original_value: original.to_owned(),
        normalized_value,
        temporal,
        location,
        extraction_confidence: None,
    })
}

fn normalize(original: &str) -> Result<(KnowledgeValue, Option<TemporalValue>), PipelineError> {
    let trimmed = original.trim();
    if trimmed.eq_ignore_ascii_case("null") || trimmed.eq_ignore_ascii_case("unknown") {
        return Ok((KnowledgeValue::Unknown, None));
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return Ok((KnowledgeValue::Known(MaterialValue::Boolean(true)), None));
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return Ok((KnowledgeValue::Known(MaterialValue::Boolean(false)), None));
    }
    if let Ok(date) = Date::parse_iso(trimmed) {
        return Ok((
            KnowledgeValue::Known(MaterialValue::Date(date)),
            Some(TemporalValue::ExactDate {
                date,
                original: original.to_owned(),
            }),
        ));
    }
    if let Some(money) = parse_usd(trimmed)? {
        return Ok((KnowledgeValue::Known(MaterialValue::Money(money)), None));
    }
    if let Ok(integer) = trimmed.parse::<i64>() {
        return Ok((KnowledgeValue::Known(MaterialValue::Integer(integer)), None));
    }
    if let Some(decimal) = parse_decimal(trimmed)? {
        return Ok((KnowledgeValue::Known(MaterialValue::Decimal(decimal)), None));
    }
    Ok((
        KnowledgeValue::Known(MaterialValue::Text(original.to_owned())),
        None,
    ))
}

fn parse_usd(value: &str) -> Result<Option<Money>, PipelineError> {
    let Some(number) = value.strip_prefix('$') else {
        return Ok(None);
    };
    let ungrouped = number.replace(',', "");
    let decimal = parse_decimal(&ungrouped)?.ok_or_else(|| {
        PipelineError::new(
            PipelineFailureKind::ValidationRejected,
            "monetary value is malformed",
        )
    })?;
    Money::new(decimal, "USD").map(Some).map_err(|_| {
        PipelineError::new(PipelineFailureKind::ValidationRejected, "money is invalid")
    })
}

fn parse_decimal(value: &str) -> Result<Option<Decimal>, PipelineError> {
    let Some((whole, fraction)) = value.split_once('.') else {
        return Ok(None);
    };
    let (negative, unsigned_whole) = whole
        .strip_prefix('-')
        .map_or((false, whole), |digits| (true, digits));
    if fraction.is_empty()
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || unsigned_whole.is_empty()
        || !unsigned_whole.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Ok(None);
    }
    let scale = u8::try_from(fraction.len()).map_err(|_| {
        PipelineError::new(
            PipelineFailureKind::ValidationRejected,
            "decimal scale exceeds supported range",
        )
    })?;
    if scale > 18 {
        return Err(PipelineError::new(
            PipelineFailureKind::ValidationRejected,
            "decimal scale exceeds supported range",
        ));
    }
    let digits = format!("{unsigned_whole}{fraction}");
    let mut coefficient = digits.parse::<i64>().map_err(|_| {
        PipelineError::new(
            PipelineFailureKind::ValidationRejected,
            "decimal coefficient exceeds supported range",
        )
    })?;
    if negative {
        coefficient = coefficient.checked_neg().ok_or_else(|| {
            PipelineError::new(
                PipelineFailureKind::ValidationRejected,
                "decimal coefficient exceeds supported range",
            )
        })?;
    }
    Decimal::new(coefficient, scale).map(Some).map_err(|_| {
        PipelineError::new(
            PipelineFailureKind::ValidationRejected,
            "decimal is invalid",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{CoreDeterministicExtractor, normalize, parse_csv, parse_decimal, parse_usd};
    use casegraph_application::{ArtifactFormat, DeterministicExtractor, PipelineFailureKind};
    use casegraph_domain::{KnowledgeValue, MaterialValue};

    #[test]
    fn text_extraction_preserves_original_span_and_exact_money() {
        let input = b"Name: Synthetic Alex\nAmount: $1,427.00\n";
        let fields = CoreDeterministicExtractor
            .extract(ArtifactFormat::PlainText, input)
            .expect("extract");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[1].original_value, "$1,427.00");
        assert_eq!(fields[1].location.text_span_start, Some(29));
        assert!(matches!(
            fields[1].normalized_value,
            KnowledgeValue::Known(MaterialValue::Money(_))
        ));
    }

    #[test]
    fn flat_json_is_sorted_and_nested_world_knowledge_is_ignored() {
        let fields = CoreDeterministicExtractor
            .extract(
                ArtifactFormat::Json,
                br#"{"z":false,"a":"2026-08-12","nested":{"not":"case fact"}}"#,
            )
            .expect("extract");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].predicate, "a");
        assert_eq!(fields[1].predicate, "z");
    }

    #[test]
    fn quoted_csv_is_deterministic_and_malformed_width_is_rejected() {
        let fields = CoreDeterministicExtractor
            .extract(
                ArtifactFormat::Csv,
                b"name,amount\n\"Synthetic, Alex\",1427.00\n",
            )
            .expect("extract");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].original_value, "Synthetic, Alex");
        let error = CoreDeterministicExtractor
            .extract(ArtifactFormat::Csv, b"a,b\n1\n")
            .expect_err("width mismatch must fail");
        assert_eq!(error.kind, PipelineFailureKind::MalformedInput);
    }

    #[test]
    fn extractor_metadata_formats_and_malformed_inputs_are_explicit() {
        let extractor = CoreDeterministicExtractor;
        assert_eq!(extractor.name(), "casegraph.core-deterministic");
        assert_eq!(extractor.version(), "1");
        assert!(extractor.supports(ArtifactFormat::PlainText));
        assert!(extractor.supports(ArtifactFormat::Json));
        assert!(extractor.supports(ArtifactFormat::Csv));
        assert_eq!(
            extractor
                .extract(ArtifactFormat::PlainText, &[0xff])
                .expect_err("non UTF-8 input")
                .kind,
            PipelineFailureKind::MalformedInput
        );
        assert!(
            extractor
                .extract(ArtifactFormat::Json, b"{")
                .expect_err("malformed JSON")
                .safe_message
                .contains("malformed")
        );
        assert!(
            extractor
                .extract(ArtifactFormat::Json, b"[]")
                .expect_err("top-level array")
                .safe_message
                .contains("top-level object")
        );
    }

    #[test]
    fn normalization_covers_epistemic_boolean_temporal_numeric_and_text_values() {
        for input in ["null", "UNKNOWN"] {
            assert_eq!(
                normalize(input).expect("unknown").0,
                KnowledgeValue::Unknown
            );
        }
        for (input, expected) in [("true", true), ("FALSE", false)] {
            assert_eq!(
                normalize(input).expect("boolean").0,
                KnowledgeValue::Known(MaterialValue::Boolean(expected))
            );
        }
        let (date, temporal) = normalize("2026-08-13").expect("date");
        assert!(matches!(
            date,
            KnowledgeValue::Known(MaterialValue::Date(_))
        ));
        assert!(temporal.is_some());
        assert!(matches!(
            normalize("$1,427.50").expect("money").0,
            KnowledgeValue::Known(MaterialValue::Money(_))
        ));
        assert_eq!(
            normalize("-42").expect("integer").0,
            KnowledgeValue::Known(MaterialValue::Integer(-42))
        );
        assert!(matches!(
            normalize("-1.25").expect("decimal").0,
            KnowledgeValue::Known(MaterialValue::Decimal(_))
        ));
        assert_eq!(
            normalize("invented text").expect("text").0,
            KnowledgeValue::Known(MaterialValue::Text("invented text".to_owned()))
        );

        assert!(parse_usd("plain").expect("not money").is_none());
        assert!(parse_usd("$bad").is_err());
        assert!(parse_decimal("1").expect("not decimal").is_none());
        for malformed in [".1", "1.", "x.1", "1.x", "-.1", "--1.0"] {
            assert!(
                parse_decimal(malformed)
                    .expect("malformed scalar is not a decimal")
                    .is_none()
            );
        }
        assert!(parse_decimal("1.1234567890123456789").is_err());
        assert!(parse_decimal("9223372036854775808.0").is_err());
    }

    #[test]
    fn text_and_csv_parsers_handle_blank_crlf_quotes_and_header_failures() {
        let text = CoreDeterministicExtractor
            .extract(
                ArtifactFormat::PlainText,
                b"ignored\r\n: no-key\r\nempty:   \r\nvalid: value\r\n",
            )
            .expect("text");
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].predicate, "valid");

        assert!(parse_csv("").expect("empty CSV").is_empty());
        assert_eq!(
            parse_csv("name,note\r\nAlex,\"said \"\"hello\"\"\"\r\n\r\n").expect("quoted CSV")[1]
                [1],
            "said \"hello\""
        );
        assert!(parse_csv("name,\"unterminated").is_err());
        let empty_header = CoreDeterministicExtractor
            .extract(ArtifactFormat::Csv, b"name,\nAlex,value\n")
            .expect_err("empty header");
        assert_eq!(empty_header.kind, PipelineFailureKind::MalformedInput);
        let fields = CoreDeterministicExtractor
            .extract(ArtifactFormat::Csv, b"name,note\nAlex,\n")
            .expect("empty values are omitted");
        assert_eq!(fields.len(), 1);
    }
}
