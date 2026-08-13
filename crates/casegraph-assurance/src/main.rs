//! Dependency-free validation for Casegraph's controlled assurance data.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

const REQUIREMENTS_HEADER: &str =
    "id\tlevel\tparent\tstatus\tderived\tallocation_source\trequirement";
const TRACEABILITY_HEADER: &str = "requirement_id\tdesign\tcode\ttests\tmethod";
const CONFIGURATION_HEADER: &str = "id\tclass\tpath\tcontrol";
const PROBLEMS_HEADER: &str = "id\tstatus\tseverity\tdescription\tverification";

#[derive(Debug)]
struct Requirement {
    level: String,
    parent: String,
    status: String,
    derived: String,
}

fn main() {
    let root = repository_root();
    match check(&root) {
        Ok(summary) => println!(
            "assurance data valid: {} requirements, {} trace records, {} configuration items, {} problem reports",
            summary.requirements,
            summary.traces,
            summary.configuration_items,
            summary.problem_reports
        ),
        Err(errors) => {
            eprintln!("assurance data validation failed:");
            for error in errors {
                eprintln!("- {error}");
            }
            std::process::exit(1);
        }
    }
}

fn repository_root() -> PathBuf {
    let mut path = env::current_dir().expect("current directory must be available");
    loop {
        if path.join("Cargo.toml").is_file() && path.join("assurance").is_dir() {
            return path;
        }
        if !path.pop() {
            panic!("run casegraph-assurance from within the Casegraph repository");
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Summary {
    requirements: usize,
    traces: usize,
    configuration_items: usize,
    problem_reports: usize,
}

fn check(root: &Path) -> Result<Summary, Vec<String>> {
    let mut errors = Vec::new();
    let requirement_rows = read_tsv(
        root,
        "assurance/requirements.tsv",
        REQUIREMENTS_HEADER,
        7,
        &mut errors,
    );
    let trace_rows = read_tsv(
        root,
        "assurance/traceability.tsv",
        TRACEABILITY_HEADER,
        5,
        &mut errors,
    );
    let configuration_rows = read_tsv(
        root,
        "assurance/configuration-index.tsv",
        CONFIGURATION_HEADER,
        4,
        &mut errors,
    );
    let problem_rows = read_tsv(
        root,
        "assurance/problem-reports.tsv",
        PROBLEMS_HEADER,
        5,
        &mut errors,
    );

    let requirements = validate_requirements(root, &requirement_rows, &mut errors);
    validate_traceability(root, &requirements, &trace_rows, &mut errors);
    validate_configuration(root, &configuration_rows, &mut errors);
    validate_problems(&problem_rows, &mut errors);

    if errors.is_empty() {
        Ok(Summary {
            requirements: requirements.len(),
            traces: trace_rows.len(),
            configuration_items: configuration_rows.len(),
            problem_reports: problem_rows.len(),
        })
    } else {
        Err(errors)
    }
}

fn read_tsv(
    root: &Path,
    relative: &str,
    expected_header: &str,
    columns: usize,
    errors: &mut Vec<String>,
) -> Vec<Vec<String>> {
    let path = root.join(relative);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents.replace("\r\n", "\n"),
        Err(error) => {
            errors.push(format!("cannot read {relative}: {error}"));
            return Vec::new();
        }
    };
    let mut lines = contents.lines();
    if lines.next() != Some(expected_header) {
        errors.push(format!("{relative} has an unexpected header"));
    }
    lines
        .enumerate()
        .filter_map(|(index, line)| {
            if line.trim().is_empty() || line.starts_with('#') {
                return None;
            }
            let values = line.split('\t').map(str::to_owned).collect::<Vec<_>>();
            if values.len() != columns {
                errors.push(format!(
                    "{relative}:{} has {} columns; expected {columns}",
                    index + 2,
                    values.len()
                ));
                None
            } else if values
                .iter()
                .any(|value| value.trim() != value || value.is_empty())
            {
                errors.push(format!(
                    "{relative}:{} contains an empty or space-padded field",
                    index + 2
                ));
                None
            } else {
                Some(values)
            }
        })
        .collect()
}

fn validate_requirements(
    root: &Path,
    rows: &[Vec<String>],
    errors: &mut Vec<String>,
) -> BTreeMap<String, Requirement> {
    let mut requirements = BTreeMap::new();
    for row in rows {
        let id = &row[0];
        let expected_prefix = match row[1].as_str() {
            "HLR" => "CG-HLR-",
            "LLR" => "CG-LLR-",
            other => {
                errors.push(format!("{id} has invalid level {other}"));
                ""
            }
        };
        if !valid_identifier(id, expected_prefix) {
            errors.push(format!("{id} is not a canonical requirement identifier"));
        }
        if !matches!(row[3].as_str(), "verified" | "deferred") {
            errors.push(format!("{id} has invalid status {}", row[3]));
        }
        if !matches!(row[4].as_str(), "yes" | "no") {
            errors.push(format!("{id} must classify derived as yes or no"));
        }
        if row[6].len() < 20 || !row[6].ends_with('.') {
            errors.push(format!(
                "{id} requirement must be a complete, testable sentence"
            ));
        }
        if row[1] == "HLR" {
            validate_reference(root, id, "allocation", &row[5], false, errors);
        } else if row[5] != row[2] {
            errors.push(format!(
                "{id} LLR allocation source {} must match parent {}",
                row[5], row[2]
            ));
        }
        if requirements
            .insert(
                id.clone(),
                Requirement {
                    level: row[1].clone(),
                    parent: row[2].clone(),
                    status: row[3].clone(),
                    derived: row[4].clone(),
                },
            )
            .is_some()
        {
            errors.push(format!("duplicate requirement {id}"));
        }
    }
    for (id, requirement) in &requirements {
        if requirement.level == "HLR" && requirement.parent != "-" {
            errors.push(format!("{id} HLR must use '-' as its parent"));
        }
        if requirement.level == "LLR" {
            match requirements.get(&requirement.parent) {
                Some(parent) if parent.level == "HLR" => {}
                _ => errors.push(format!(
                    "{id} LLR parent {} is not an existing HLR",
                    requirement.parent
                )),
            }
        }
        if requirement.derived == "yes" && requirement.parent == "-" {
            errors.push(format!("derived requirement {id} must identify a parent"));
        }
    }
    requirements
}

fn validate_traceability(
    root: &Path,
    requirements: &BTreeMap<String, Requirement>,
    rows: &[Vec<String>],
    errors: &mut Vec<String>,
) {
    let mut traced = BTreeSet::new();
    for row in rows {
        let id = &row[0];
        if !requirements.contains_key(id) {
            errors.push(format!("trace record references unknown requirement {id}"));
        }
        if !traced.insert(id.clone()) {
            errors.push(format!("duplicate trace record for {id}"));
        }
        for reference in split_references(&row[1]) {
            validate_reference(root, id, "design", reference, false, errors);
        }
        for reference in split_references(&row[2]) {
            validate_reference(root, id, "code", reference, false, errors);
        }
        for reference in split_references(&row[3]) {
            validate_reference(root, id, "test", reference, true, errors);
        }
        if !matches!(row[4].as_str(), "test" | "test+analysis") {
            errors.push(format!(
                "{id} has unsupported verification method {}",
                row[4]
            ));
        }
    }
    for (id, requirement) in requirements {
        if requirement.status == "verified" && !traced.contains(id) {
            errors.push(format!("verified requirement {id} has no trace record"));
        }
        if requirement.status == "deferred" && traced.contains(id) {
            errors.push(format!(
                "deferred requirement {id} must not claim verification trace"
            ));
        }
    }
}

fn split_references(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(';')
        .filter(|item| !item.is_empty() && *item != "-")
}

fn validate_reference(
    root: &Path,
    requirement_id: &str,
    kind: &str,
    reference: &str,
    symbol_required: bool,
    errors: &mut Vec<String>,
) {
    let (path, symbol) = reference
        .split_once('#')
        .map_or((reference, None), |(path, symbol)| (path, Some(symbol)));
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        errors.push(format!(
            "{requirement_id} {kind} reference escapes the repository: {reference}"
        ));
        return;
    }
    let resolved = root.join(relative);
    if !resolved.is_file() {
        errors.push(format!(
            "{requirement_id} {kind} path does not exist: {path}"
        ));
        return;
    }
    if symbol_required && symbol.is_none() {
        errors.push(format!(
            "{requirement_id} test reference lacks a function name: {reference}"
        ));
    }
    if let Some(symbol) = symbol {
        if symbol.is_empty()
            || !symbol
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            errors.push(format!(
                "{requirement_id} has invalid symbol in {reference}"
            ));
            return;
        }
        match fs::read_to_string(&resolved) {
            Ok(contents) if contents.contains(&format!("fn {symbol}(")) => {}
            Ok(_) => errors.push(format!(
                "{requirement_id} {kind} symbol {symbol} was not found in {path}"
            )),
            Err(error) => errors.push(format!("cannot inspect {path}: {error}")),
        }
    }
}

fn validate_configuration(root: &Path, rows: &[Vec<String>], errors: &mut Vec<String>) {
    let mut ids = BTreeSet::new();
    for row in rows {
        if !valid_identifier(&row[0], "CG-CI-") {
            errors.push(format!(
                "{} is not a canonical configuration item id",
                row[0]
            ));
        }
        if !ids.insert(row[0].clone()) {
            errors.push(format!("duplicate configuration item {}", row[0]));
        }
        if !matches!(
            row[1].as_str(),
            "planning"
                | "requirements"
                | "design"
                | "source"
                | "verification"
                | "environment"
                | "release"
        ) {
            errors.push(format!(
                "{} has invalid configuration class {}",
                row[0], row[1]
            ));
        }
        let path = Path::new(&row[2]);
        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            errors.push(format!(
                "{} has unsafe configuration path {}",
                row[0], row[2]
            ));
        } else if !root.join(path).exists() {
            errors.push(format!(
                "{} configuration path does not exist: {}",
                row[0], row[2]
            ));
        }
        if row[3] != "git" {
            errors.push(format!(
                "{} has unsupported control mechanism {}",
                row[0], row[3]
            ));
        }
    }
}

fn validate_problems(rows: &[Vec<String>], errors: &mut Vec<String>) {
    let mut ids = BTreeSet::new();
    for row in rows {
        if !valid_identifier(&row[0], "CG-PR-") || !ids.insert(row[0].clone()) {
            errors.push(format!("invalid or duplicate problem report id {}", row[0]));
        }
        if !matches!(row[1].as_str(), "open" | "closed" | "deferred") {
            errors.push(format!("{} has invalid status {}", row[0], row[1]));
        }
        if !matches!(row[2].as_str(), "critical" | "major" | "minor") {
            errors.push(format!("{} has invalid severity {}", row[0], row[2]));
        }
        if row[1] == "closed" && row[4] == "-" {
            errors.push(format!(
                "closed problem report {} lacks verification evidence",
                row[0]
            ));
        }
    }
}

fn valid_identifier(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| suffix.len() == 3 && suffix.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn repository_assurance_data_is_internally_consistent() {
        let root = repository_root();
        let summary = check(&root).expect("controlled assurance data must validate");
        assert!(summary.requirements >= 10);
        assert_eq!(summary.requirements, summary.traces);
        assert!(summary.configuration_items >= 10);
    }

    #[test]
    fn malformed_and_escaping_assurance_references_are_rejected() {
        let root = temporary_repository();
        write(
            &root,
            "assurance/requirements.tsv",
            &format!(
                "{REQUIREMENTS_HEADER}\nCG-HLR-001\tHLR\t-\tverified\tno\tCargo.toml\tThe platform shall retain controlled evidence.\n"
            ),
        );
        write(
            &root,
            "assurance/traceability.tsv",
            &format!(
                "{TRACEABILITY_HEADER}\nCG-HLR-001\t../outside.md\tCargo.toml\tCargo.toml#missing_test\ttest\n"
            ),
        );
        write(
            &root,
            "assurance/configuration-index.tsv",
            CONFIGURATION_HEADER,
        );
        write(&root, "assurance/problem-reports.tsv", PROBLEMS_HEADER);
        write(&root, "Cargo.toml", "[workspace]\n");

        let errors = check(&root).expect_err("invalid data must fail closed");
        assert!(errors.iter().any(|error| error.contains("escapes")));
        assert!(errors.iter().any(|error| error.contains("missing_test")));
        fs::remove_dir_all(root).ok();
    }

    fn temporary_repository() -> PathBuf {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!(
            "casegraph-assurance-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("assurance")).expect("test assurance directory");
        root
    }

    fn write(root: &Path, path: &str, contents: &str) {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("parent directory");
        }
        fs::write(target, contents).expect("test fixture");
    }
}
