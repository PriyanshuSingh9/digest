//! Owned decoding of raw MCP tool arguments.
//!
//! `Parameters<T>` deserializes before a tool handler body runs, so a malformed call surfaces as
//! serde's own sentence with no Digest field path, no corrected example, and nothing separating the
//! three overlapping enums. Each `decode_*` function here walks the raw JSON first and reports the
//! path, the value actually received, the allowed values, and a minimal corrected object, then
//! hands the original value to serde, so the accepted result is always serde's own parse.
//!
//! Every check below mirrors a rule serde already enforces, so a check can only fire on input that
//! serde would have rejected anyway. The accept/reject boundary is unchanged; only the message is.

use crate::{
    tools::{
        AnalysisFindingKind, IngestArticleInput, NarrationImportance, NarrationIntent,
        PresentationType, ProvenanceKind, ReadArtifactInput, SourceCoverageTreatment,
        WriteAnalysisInput, WriteNarrationPlanInput,
    },
    DigestError,
};
use serde::Serialize;
use serde_json::{Map, Value};

/// Minimal valid `write_analysis` arguments.
pub const WRITE_ANALYSIS_EXAMPLE: &str = concat!(
    r#"{"jobId": "job-1", "articleId": "article-1", "centralArgument": "#,
    r#"{"text": "The article argues that durable queues decouple producers from consumers.", "#,
    r#""sourceBlocks": ["block-1", "block-2"], "kind": "source_derived"}, "findings": "#,
    r#"[{"category": "key_claim", "text": "Independent components can absorb load spikes.", "#,
    r#""sourceBlocks": ["block-2"], "kind": "ai_inference"}]}"#
);

/// Minimal valid `write_narration_plan` arguments.
pub const WRITE_NARRATION_PLAN_EXAMPLE: &str = concat!(
    r#"{"jobId": "job-1", "articleId": "article-1", "title": "Durable queues", "segments": "#,
    r#"[{"displayText": "io_uring submits work asynchronously.", "ttsText": "#,
    r#""eye-oh uring submits work asynchronously.", "sourceBlocks": ["block-1"], "#,
    r#""presentationType": "article-text", "importance": "core", "intent": "explanation", "#,
    r#""provenance": "source_derived"}], "sourceCoverageDecisions": "#,
    r#"[{"sourceBlocks": ["block-1"], "treatment": "teach", "rationale": "Core definition."}]}"#
);

/// Minimal valid `AnalysisClaim`.
pub const ANALYSIS_CLAIM_EXAMPLE: &str = concat!(
    r#"{"text": "The article argues that durable queues decouple producers from consumers.", "#,
    r#""sourceBlocks": ["block-1", "block-2"], "kind": "source_derived"}"#
);

/// Minimal valid `AnalysisFinding`.
pub const ANALYSIS_FINDING_EXAMPLE: &str = concat!(
    r#"{"category": "key_claim", "text": "Independent components can absorb load spikes.", "#,
    r#""sourceBlocks": ["block-2"], "kind": "ai_inference"}"#
);

/// Minimal valid narration segment draft.
pub const NARRATION_SEGMENT_EXAMPLE: &str = concat!(
    r#"{"displayText": "io_uring submits work asynchronously.", "ttsText": "#,
    r#""eye-oh uring submits work asynchronously.", "sourceBlocks": ["block-1"], "#,
    r#""presentationType": "article-text", "importance": "core", "intent": "explanation", "#,
    r#""provenance": "source_derived"}"#
);

/// Minimal valid source coverage decision.
pub const COVERAGE_DECISION_EXAMPLE: &str =
    r#"{"sourceBlocks": ["block-1"], "treatment": "teach", "rationale": "Core definition."}"#;

/// Longest echoed fragment of a received value before it is truncated.
const MAX_ECHO: usize = 200;

/// The wire names of one enum plus the note that keeps it apart from its neighbours.
pub(crate) struct Vocabulary {
    pub(crate) name: &'static str,
    pub(crate) names: String,
    pub(crate) cross_note: &'static str,
}

impl Vocabulary {
    pub(crate) fn new<T: Copy + Serialize>(
        name: &'static str,
        variants: &[T],
        cross_note: &'static str,
    ) -> Self {
        Self {
            name,
            names: variants
                .iter()
                .filter_map(|variant| {
                    serde_json::to_value(variant)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                })
                .collect::<Vec<_>>()
                .join(", "),
            cross_note,
        }
    }

    /// The first allowed name, used to show the shape of a correct value.
    fn first(&self) -> &str {
        self.names.split(", ").next().unwrap_or_default()
    }
}

/// Decodes `write_analysis` arguments, reporting the exact JSON path on failure.
pub fn decode_write_analysis(arguments: &Value) -> Result<WriteAnalysisInput, DigestError> {
    const TOOL: &str = "write_analysis";
    const CENTRAL_ARGUMENT: &str = concat!(
        "an AnalysisClaim object with `text`, `sourceBlocks`, and `kind`; a correct value is ",
        r#"{"text": "...", "sourceBlocks": ["block-1"], "kind": "source_derived"}"#
    );
    const FINDINGS: &str = concat!(
        "a non-empty array of AnalysisFinding objects; a correct value is [",
        r#"{"category": "key_claim", "text": "...", "sourceBlocks": ["block-1"], "kind": "source_derived"}]"#
    );

    let arguments = arguments_object(
        TOOL,
        arguments,
        "jobId, articleId, centralArgument, findings",
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    all_present(
        TOOL,
        arguments,
        &["jobId", "articleId", "centralArgument", "findings"],
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    string_field(
        TOOL,
        arguments,
        "jobId",
        "the running job",
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    string_field(
        TOOL,
        arguments,
        "articleId",
        "the normalized article id returned by ingest_article",
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    check_claim(
        TOOL,
        required(
            TOOL,
            "centralArgument",
            arguments,
            "centralArgument",
            CENTRAL_ARGUMENT,
            ANALYSIS_CLAIM_EXAMPLE,
        )?,
    )?;
    let findings = array_field(
        TOOL,
        arguments,
        "findings",
        FINDINGS,
        ANALYSIS_FINDING_EXAMPLE,
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    for (index, finding) in findings.iter().enumerate() {
        check_finding(TOOL, &format!("findings[{index}]"), finding)?;
    }

    serde_parse(TOOL, arguments, WRITE_ANALYSIS_EXAMPLE)
}

/// Decodes `write_narration_plan` arguments, reporting the exact JSON path on failure.
pub fn decode_write_narration_plan(
    arguments: &Value,
) -> Result<WriteNarrationPlanInput, DigestError> {
    const TOOL: &str = "write_narration_plan";
    const SEGMENTS: &str = concat!(
        "a non-empty array of narration segment drafts; a correct value is [",
        r#"{"displayText": "...", "ttsText": "...", "sourceBlocks": ["block-1"], "presentationType": "article-text", "importance": "core", "intent": "explanation", "provenance": "source_derived"}]"#
    );
    const DECISIONS: &str = concat!(
        "an array that accounts for every source block exactly once; a correct value is [",
        r#"{"sourceBlocks": ["block-1"], "treatment": "teach", "rationale": "..."}]"#
    );

    let arguments = arguments_object(
        TOOL,
        arguments,
        "jobId, articleId, title, segments, sourceCoverageDecisions",
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    all_present(
        TOOL,
        arguments,
        &[
            "jobId",
            "articleId",
            "title",
            "segments",
            "sourceCoverageDecisions",
        ],
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    string_field(
        TOOL,
        arguments,
        "jobId",
        "the running job",
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    string_field(
        TOOL,
        arguments,
        "articleId",
        "the normalized article id returned by ingest_article",
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    string_field(
        TOOL,
        arguments,
        "title",
        "the lesson title",
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    let segments = array_field(
        TOOL,
        arguments,
        "segments",
        SEGMENTS,
        NARRATION_SEGMENT_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    for (index, segment) in segments.iter().enumerate() {
        check_segment(TOOL, &format!("segments[{index}]"), segment)?;
    }
    let decisions = array_field(
        TOOL,
        arguments,
        "sourceCoverageDecisions",
        DECISIONS,
        COVERAGE_DECISION_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    for (index, decision) in decisions.iter().enumerate() {
        check_coverage_decision(TOOL, &format!("sourceCoverageDecisions[{index}]"), decision)?;
    }

    serde_parse(TOOL, arguments, WRITE_NARRATION_PLAN_EXAMPLE)
}

/// Decodes `read_artifact` arguments, reporting the exact JSON path on failure.
pub fn decode_read_artifact(arguments: &Value) -> Result<ReadArtifactInput, DigestError> {
    const TOOL: &str = "read_artifact";
    const EXAMPLE: &str = r#"{"artifactId": "article-1"}"#;

    let arguments = arguments_object(TOOL, arguments, "artifactId", EXAMPLE)?;
    all_present(TOOL, arguments, &["artifactId"], EXAMPLE)?;
    string_field(
        TOOL,
        arguments,
        "artifactId",
        "a persisted artifact id",
        EXAMPLE,
    )?;

    serde_parse(TOOL, arguments, EXAMPLE)
}

/// Decodes `ingest_article` arguments, reporting the exact JSON path on failure.
pub fn decode_ingest_article(arguments: &Value) -> Result<IngestArticleInput, DigestError> {
    const TOOL: &str = "ingest_article";
    const EXAMPLE: &str = r#"{"jobId": "job-1", "url": "https://example.com/article"}"#;

    let arguments = arguments_object(TOOL, arguments, "jobId, url", EXAMPLE)?;
    all_present(TOOL, arguments, &["jobId", "url"], EXAMPLE)?;
    string_field(TOOL, arguments, "jobId", "the running job", EXAMPLE)?;
    string_field(TOOL, arguments, "url", "the absolute article url", EXAMPLE)?;

    serde_parse(TOOL, arguments, EXAMPLE)
}

fn check_claim(tool: &str, value: &Value) -> Result<(), DigestError> {
    const PATH: &str = "centralArgument";
    const EXPECTATION: &str = "an AnalysisClaim object with `text`, `sourceBlocks`, and `kind`";

    let object = object_value(
        tool,
        PATH,
        value,
        EXPECTATION,
        ANALYSIS_CLAIM_EXAMPLE,
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    string_member(
        tool,
        &format!("{PATH}.text"),
        object,
        "text",
        "the claim sentence",
        ANALYSIS_CLAIM_EXAMPLE,
    )?;
    array_member(
        tool,
        &format!("{PATH}.sourceBlocks"),
        object,
        "sourceBlocks",
        "a non-empty array of article block ids",
        ANALYSIS_CLAIM_EXAMPLE,
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{PATH}.kind"),
        object,
        "kind",
        &Vocabulary::new(
            "ProvenanceKind",
            &ProvenanceKind::ALL,
            "`kind` takes ProvenanceKind values such as \"source_derived\"; it never takes \
             AnalysisFindingKind values, which belong to `category` on a finding",
        ),
        ANALYSIS_CLAIM_EXAMPLE,
    )
}

fn check_finding(tool: &str, path: &str, value: &Value) -> Result<(), DigestError> {
    const EXPECTATION: &str =
        "an AnalysisFinding object with `category`, `text`, `sourceBlocks`, and `kind`";

    let object = object_value(
        tool,
        path,
        value,
        EXPECTATION,
        ANALYSIS_FINDING_EXAMPLE,
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.category"),
        object,
        "category",
        &Vocabulary::new(
            "AnalysisFindingKind",
            &AnalysisFindingKind::ALL,
            "`category` takes AnalysisFindingKind values such as \"key_claim\"; it never takes \
             ProvenanceKind values such as \"ai_explanation\", which belong to the sibling `kind` \
             field",
        ),
        ANALYSIS_FINDING_EXAMPLE,
    )?;
    string_member(
        tool,
        &format!("{path}.text"),
        object,
        "text",
        "the finding sentence",
        ANALYSIS_FINDING_EXAMPLE,
    )?;
    array_member(
        tool,
        &format!("{path}.sourceBlocks"),
        object,
        "sourceBlocks",
        "a non-empty array of article block ids",
        ANALYSIS_FINDING_EXAMPLE,
        WRITE_ANALYSIS_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.kind"),
        object,
        "kind",
        &Vocabulary::new(
            "ProvenanceKind",
            &ProvenanceKind::ALL,
            "`kind` takes ProvenanceKind values such as \"ai_inference\"; it never takes \
             AnalysisFindingKind values such as \"key_claim\", which belong to the sibling \
             `category` field",
        ),
        ANALYSIS_FINDING_EXAMPLE,
    )
}

fn check_segment(tool: &str, path: &str, value: &Value) -> Result<(), DigestError> {
    const EXPECTATION: &str = concat!(
        "a narration segment object with `displayText`, `ttsText`, `sourceBlocks`, ",
        "`presentationType`, `importance`, `intent`, and `provenance`"
    );

    let object = object_value(
        tool,
        path,
        value,
        EXPECTATION,
        NARRATION_SEGMENT_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    string_member(
        tool,
        &format!("{path}.displayText"),
        object,
        "displayText",
        "the on-screen sentence",
        NARRATION_SEGMENT_EXAMPLE,
    )?;
    string_member(
        tool,
        &format!("{path}.ttsText"),
        object,
        "ttsText",
        "the spoken sentence",
        NARRATION_SEGMENT_EXAMPLE,
    )?;
    array_member(
        tool,
        &format!("{path}.sourceBlocks"),
        object,
        "sourceBlocks",
        "a non-empty array of article block ids",
        NARRATION_SEGMENT_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.presentationType"),
        object,
        "presentationType",
        &Vocabulary::new(
            "PresentationType",
            &PresentationType::ALL,
            "`presentationType` takes PresentationType values in kebab-case such as \"article-text\"; \
             it never takes NarrationIntent values such as \"quantification\" or \"comparison\", \
             which belong to `intent`",
        ),
        NARRATION_SEGMENT_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.importance"),
        object,
        "importance",
        &Vocabulary::new(
            "NarrationImportance",
            &NarrationImportance::ALL,
            "`importance` takes \"core\" or \"supporting\"",
        ),
        NARRATION_SEGMENT_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.intent"),
        object,
        "intent",
        &Vocabulary::new(
            "NarrationIntent",
            &NarrationIntent::ALL,
            "`intent` takes NarrationIntent values in snake_case such as \"explanation\"; it never \
             takes PresentationType values such as \"article-text\" or \"diagram\", which belong to \
             `presentationType`",
        ),
        NARRATION_SEGMENT_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.provenance"),
        object,
        "provenance",
        &Vocabulary::new(
            "ProvenanceKind",
            &ProvenanceKind::ALL,
            "`provenance` takes ProvenanceKind values such as \"source_derived\"; it never takes \
             AnalysisFindingKind values and never takes NarrationIntent values",
        ),
        NARRATION_SEGMENT_EXAMPLE,
    )
}

fn check_coverage_decision(tool: &str, path: &str, value: &Value) -> Result<(), DigestError> {
    const EXPECTATION: &str =
        "a coverage decision object with `sourceBlocks`, `treatment`, and `rationale`";

    let object = object_value(
        tool,
        path,
        value,
        EXPECTATION,
        COVERAGE_DECISION_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    array_member(
        tool,
        &format!("{path}.sourceBlocks"),
        object,
        "sourceBlocks",
        "a non-empty array of article block ids",
        COVERAGE_DECISION_EXAMPLE,
        WRITE_NARRATION_PLAN_EXAMPLE,
    )?;
    enum_member(
        tool,
        &format!("{path}.treatment"),
        object,
        "treatment",
        &Vocabulary::new(
            "SourceCoverageTreatment",
            &SourceCoverageTreatment::ALL,
            concat!(
                "`treatment` decides whether a block is cited: \"teach\" and \"summarize\" blocks ",
                "must be cited by at least one segment, and a \"skip\" block must be cited by none"
            ),
        ),
        COVERAGE_DECISION_EXAMPLE,
    )?;
    string_member(
        tool,
        &format!("{path}.rationale"),
        object,
        "rationale",
        "why this treatment was chosen",
        COVERAGE_DECISION_EXAMPLE,
    )
}

/// Reports every absent top-level field at once, so a call missing three keys is fixed in one
/// round trip instead of three.
fn all_present(
    tool: &str,
    object: &Map<String, Value>,
    fields: &[&str],
    corrected: &str,
) -> Result<(), DigestError> {
    let missing = fields
        .iter()
        .filter(|field| !object.contains_key(**field))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    Err(DigestError::InvalidInput(format!(
        "{tool}: missing required top-level field(s): {}. Nothing was sent for {}.\n\
         Received keys: {}.\nFix: add every missing field.\nCorrected call: {corrected}",
        missing.join(", "),
        if missing.len() == 1 {
            "it".to_owned()
        } else {
            "them".to_owned()
        },
        if object.is_empty() {
            "(none)".to_owned()
        } else {
            object.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    )))
}

fn arguments_object<'a>(
    tool: &str,
    arguments: &'a Value,
    fields: &str,
    corrected: &str,
) -> Result<&'a Map<String, Value>, DigestError> {
    match arguments {
        Value::Object(object) => Ok(object),
        received => Err(DigestError::InvalidInput(format!(
            "{tool}: the tool arguments must be one JSON object with the fields {fields}, but a {} \
             was received.\nReceived: {}\nFix: wrap the values in that object rather than sending a \
             bare list.\nCorrected call: {corrected}",
            type_name(received),
            render(received)
        ))),
    }
}

/// Fetches a required member, naming `full_path` in the message so a nested miss reads as
/// `findings[0].text` rather than `text`.
fn required<'a>(
    tool: &str,
    full_path: &str,
    object: &'a Map<String, Value>,
    field: &str,
    expectation: &str,
    corrected: &str,
) -> Result<&'a Value, DigestError> {
    object.get(field).ok_or_else(|| {
        DigestError::InvalidInput(format!(
            "{tool}: missing required field `{full_path}`. Expected {expectation}, and it was \
             absent from the object that holds it.\nReceived keys: {}\nFix: add `{field}`. \
             Corrected: {corrected}",
            if object.is_empty() {
                "(none)".to_owned()
            } else {
                object.keys().cloned().collect::<Vec<_>>().join(", ")
            }
        ))
    })
}

fn string_field(
    tool: &str,
    arguments: &Map<String, Value>,
    field: &str,
    expectation: &str,
    corrected: &str,
) -> Result<(), DigestError> {
    let value = required(
        tool,
        field,
        arguments,
        field,
        &format!("a JSON string holding {expectation}"),
        corrected,
    )?;
    match value {
        Value::String(_) => Ok(()),
        received => Err(type_error(
            tool,
            field,
            received,
            &format!("a JSON string holding {expectation}"),
            corrected,
        )),
    }
}

fn array_field<'a>(
    tool: &str,
    arguments: &'a Map<String, Value>,
    field: &str,
    expectation: &str,
    corrected: &str,
    call: &str,
) -> Result<&'a [Value], DigestError> {
    array_member(tool, field, arguments, field, expectation, corrected, call)
}

fn object_value<'a>(
    tool: &str,
    path: &str,
    value: &'a Value,
    expectation: &str,
    corrected: &str,
    call: &str,
) -> Result<&'a Map<String, Value>, DigestError> {
    match value {
        Value::Object(object) => Ok(object),
        Value::String(text) if is_encoded_json(text) => Err(encoded_error(
            tool,
            path,
            text,
            expectation,
            corrected,
            call,
        )),
        received => Err(type_error(tool, path, received, expectation, corrected)),
    }
}

fn enum_member(
    tool: &str,
    full_path: &str,
    object: &Map<String, Value>,
    field: &str,
    vocabulary: &Vocabulary,
    corrected: &str,
) -> Result<(), DigestError> {
    let value = required(
        tool,
        full_path,
        object,
        field,
        &format!("a {} value", vocabulary.name),
        corrected,
    )?;
    match value {
        Value::String(received)
            if vocabulary
                .names
                .split(", ")
                .any(|allowed| allowed == received) =>
        {
            Ok(())
        }
        Value::String(received) => Err(DigestError::InvalidInput(format!(
            "{tool}: {full_path} has the value \"{received}\", which is not a {} value.\n\
             Allowed: {}.\n{}.\nFix: replace \"{received}\" with one of the allowed values, for \
             example \"{}\".\nCorrected field: \"{field}\": \"{}\".\nCorrected object: {corrected}",
            vocabulary.name,
            vocabulary.names,
            vocabulary.cross_note,
            vocabulary.first(),
            vocabulary.first()
        ))),
        received => Err(type_error(
            tool,
            full_path,
            received,
            &format!(
                "a {} string such as \"{}\"",
                vocabulary.name,
                vocabulary.first()
            ),
            corrected,
        )),
    }
}

fn string_member(
    tool: &str,
    full_path: &str,
    object: &Map<String, Value>,
    field: &str,
    expectation: &str,
    corrected: &str,
) -> Result<(), DigestError> {
    let value = required(
        tool,
        full_path,
        object,
        field,
        &format!("a JSON string holding {expectation}"),
        corrected,
    )?;
    match value {
        Value::String(_) => Ok(()),
        received => Err(type_error(
            tool,
            full_path,
            received,
            &format!("a JSON string holding {expectation}"),
            corrected,
        )),
    }
}

fn array_member<'a>(
    tool: &str,
    full_path: &str,
    object: &'a Map<String, Value>,
    field: &str,
    expectation: &str,
    corrected: &str,
    call: &str,
) -> Result<&'a [Value], DigestError> {
    let value = required(tool, full_path, object, field, expectation, corrected)?;
    match value {
        Value::Array(items) => Ok(items.as_slice()),
        Value::String(text) if is_encoded_json(text) => Err(encoded_error(
            tool,
            full_path,
            text,
            expectation,
            corrected,
            call,
        )),
        received => Err(type_error(
            tool,
            full_path,
            received,
            expectation,
            corrected,
        )),
    }
}

/// A string whose content is itself JSON: the double-encoded failure mode, where the intended value
/// was serialized and the resulting string was passed instead of the value.
fn is_encoded_json(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn encoded_error(
    tool: &str,
    path: &str,
    text: &str,
    expectation: &str,
    corrected: &str,
    call: &str,
) -> DigestError {
    DigestError::InvalidInput(format!(
        "{tool}: {path} was received as a string that contains JSON, not as the value itself.\n\
         Received: \"{}\"\nExpected: {expectation}.\nThe value was serialized twice. Fix: pass the \
         inner JSON directly, without wrapping it in a string.\nCorrected value: {corrected}\n\
         Corrected call: {call}",
        truncate(text)
    ))
}

fn type_error(
    tool: &str,
    path: &str,
    received: &Value,
    expectation: &str,
    corrected: &str,
) -> DigestError {
    DigestError::InvalidInput(format!(
        "{tool}: {path} must be {expectation}, but a {} was received.\nReceived: {}\n\
         Corrected: {corrected}",
        type_name(received),
        render(received)
    ))
}

/// The authoritative parse. Reached only after every structural check has passed, so the value
/// returned here is exactly what serde alone would have produced.
fn serde_parse<T: serde::de::DeserializeOwned>(
    tool: &str,
    arguments: &Map<String, Value>,
    corrected: &str,
) -> Result<T, DigestError> {
    serde_json::from_value(Value::Object(arguments.clone())).map_err(|error| {
        DigestError::InvalidInput(format!(
            "{tool}: the arguments do not match the tool contract: {error}.\nReceived: {}\n\
             Corrected call: {corrected}",
            render(&Value::Object(arguments.clone()))
        ))
    })
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn render(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{}\"", truncate(text)),
        other => truncate(&other.to_string()),
    }
}

fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_ECHO {
        return text.to_owned();
    }
    let head = text.chars().take(MAX_ECHO).collect::<String>();
    format!("{head}... (truncated)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_and_kind_report_their_own_vocabulary() {
        let error = decode_write_analysis(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "centralArgument": {
                "text": "Durable queues decouple producers from consumers.",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            },
            "findings": [{
                "category": "ai_explanation",
                "text": "A point.",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            }]
        }))
        .expect_err("ai_explanation is a ProvenanceKind, not an AnalysisFindingKind");
        let message = error.to_string();
        assert!(message.contains("findings[0].category"), "{message}");
        assert!(message.contains("AnalysisFindingKind"), "{message}");
        assert!(message.contains("never takes ProvenanceKind"), "{message}");
        assert!(message.contains("Allowed: major_concept,"), "{message}");
        assert!(
            message.contains(r#""category": "major_concept""#),
            "{message}"
        );
    }

    #[test]
    fn presentation_type_rejects_a_narration_intent() {
        let error = decode_write_narration_plan(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "title": "Durable queues",
            "segments": [{
                "displayText": "Queues decouple producers from consumers.",
                "ttsText": "Queues decouple producers from consumers.",
                "sourceBlocks": ["block-1"],
                "presentationType": "quantification",
                "importance": "core",
                "intent": "explanation",
                "provenance": "source_derived"
            }],
            "sourceCoverageDecisions": []
        }))
        .expect_err("quantification is a NarrationIntent, not a PresentationType");
        let message = error.to_string();
        assert!(
            message.contains("segments[0].presentationType"),
            "{message}"
        );
        assert!(
            message.contains("Allowed: article-text, callout, code"),
            "{message}"
        );
        assert!(message.contains("never takes NarrationIntent"), "{message}");
    }

    #[test]
    fn intent_rejects_a_presentation_type() {
        let error = decode_write_narration_plan(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "title": "Durable queues",
            "segments": [{
                "displayText": "Queues decouple producers from consumers.",
                "ttsText": "Queues decouple producers from consumers.",
                "sourceBlocks": ["block-1"],
                "presentationType": "article-text",
                "importance": "core",
                "intent": "concept-card",
                "provenance": "source_derived"
            }],
            "sourceCoverageDecisions": []
        }))
        .expect_err("concept-card is a PresentationType, not a NarrationIntent");
        let message = error.to_string();
        assert!(message.contains("segments[0].intent"), "{message}");
        assert!(message.contains("NarrationIntent"), "{message}");
        assert!(
            message.contains("Allowed: introduction, explanation, example"),
            "{message}"
        );
        assert!(
            message.contains("never takes PresentationType"),
            "{message}"
        );
    }

    #[test]
    fn double_encoded_claim_shows_the_uncoded_object() {
        let claim =
            r#"{"text": "A claim.", "sourceBlocks": ["block-1"], "kind": "ai_explanation"}"#;
        let error = decode_write_analysis(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "centralArgument": claim,
            "findings": []
        }))
        .expect_err("a string containing the claim object is double encoded");
        let message = error.to_string();
        assert!(message.contains("centralArgument"), "{message}");
        assert!(message.contains("serialized twice"), "{message}");
        assert!(message.contains(r#""text": "A claim.""#), "{message}");
        assert!(
            message.contains(r#""centralArgument": {"text""#),
            "{message}"
        );
    }

    #[test]
    fn a_bare_list_is_reported_against_the_expected_object() {
        let error = decode_write_analysis(&serde_json::json!([{
            "category": "key_claim",
            "text": "A point.",
            "sourceBlocks": ["block-1"],
            "kind": "source_derived"
        }]))
        .expect_err("a bare findings list is not the arguments object");
        let message = error.to_string();
        assert!(message.contains("must be one JSON object"), "{message}");
        assert!(
            message.contains("jobId, articleId, centralArgument, findings"),
            "{message}"
        );
        assert!(message.contains("Corrected call: {"), "{message}");
    }

    #[test]
    fn a_map_where_a_sequence_belongs_names_the_field_and_an_example() {
        let error = decode_write_analysis(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "centralArgument": {
                "text": "A claim.",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            },
            "findings": { "0": { "text": "A point." } }
        }))
        .expect_err("findings must be an array");
        let message = error.to_string();
        assert!(message.contains("findings must be"), "{message}");
        assert!(
            message.contains("a non-empty array of AnalysisFinding objects"),
            "{message}"
        );
        assert!(message.contains(r#""category": "key_claim""#), "{message}");
    }

    #[test]
    fn a_missing_finding_field_names_the_path_and_the_example() {
        let error = decode_write_analysis(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "centralArgument": {
                "text": "A claim.",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            },
            "findings": [{
                "category": "key_claim",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            }]
        }))
        .expect_err("findings[0] is missing text");
        let message = error.to_string();
        assert!(message.contains("findings[0].text"), "{message}");
        assert!(
            message.contains("missing required field `findings[0].text`"),
            "{message}"
        );
        assert!(
            message.contains(r#""category": "key_claim", "text": "#),
            "{message}"
        );
    }

    #[test]
    fn a_missing_top_level_field_lists_the_keys_that_were_sent() {
        let error = decode_write_narration_plan(&serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "segments": []
        }))
        .expect_err("title is required");
        let message = error.to_string();
        assert!(
            message.contains("missing required top-level field(s): title, sourceCoverageDecisions"),
            "{message}"
        );
        assert!(
            message.contains("Received keys: jobId, articleId, segments"),
            "{message}"
        );
    }

    #[test]
    fn a_well_formed_call_decodes_to_what_serde_would_produce() {
        let arguments = serde_json::json!({
            "jobId": "job-1",
            "articleId": "article-1",
            "centralArgument": {
                "text": "Durable queues decouple producers from consumers.",
                "sourceBlocks": ["block-1"],
                "kind": "source_derived"
            },
            "findings": [{
                "category": "key_claim",
                "text": "A point.",
                "sourceBlocks": ["block-1"],
                "kind": "ai_inference"
            }]
        });
        let decoded = decode_write_analysis(&arguments).expect("valid arguments decode");
        let direct: WriteAnalysisInput =
            serde_json::from_value(arguments).expect("serde accepts the same arguments");
        assert_eq!(
            decoded.central_argument.source_blocks,
            direct.central_argument.source_blocks
        );
        assert!(matches!(
            decoded.findings[0].category,
            AnalysisFindingKind::KeyClaim
        ));
        assert!(matches!(
            decoded.findings[0].kind,
            ProvenanceKind::AiInference
        ));
    }

    #[test]
    fn the_decode_layer_never_rejects_input_serde_accepts() {
        // Empty strings, empty arrays, and unused keys are the facade's business, not the decoder's,
        // so anything serde accepts must survive the owned decode unchanged.
        let accepted = [
            serde_json::json!({
                "jobId": "",
                "articleId": "",
                "centralArgument": { "text": "", "sourceBlocks": [], "kind": "source_derived" },
                "findings": [],
                "unusedExtraKey": 7
            }),
            serde_json::json!({
                "jobId": "job-1",
                "articleId": "article-1",
                "centralArgument": { "text": "t", "sourceBlocks": ["b"], "kind": "ai_inference" },
                "findings": [{
                    "category": "example",
                    "text": "",
                    "sourceBlocks": [],
                    "kind": "source_derived"
                }]
            }),
        ];
        for arguments in accepted {
            assert!(serde_json::from_value::<WriteAnalysisInput>(arguments.clone()).is_ok());
            assert!(decode_write_analysis(&arguments).is_ok(), "{arguments}");
        }
    }
}
