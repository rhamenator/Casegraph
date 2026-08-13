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
    if fraction.is_empty()
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        || whole.is_empty()
        || !whole
            .trim_start_matches('-')
            .bytes()
            .all(|byte| byte.is_ascii_digit())
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
    let negative = whole.starts_with('-');
    let digits = format!("{}{}", whole.trim_start_matches('-'), fraction);
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
    use super::CoreDeterministicExtractor;
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
}
