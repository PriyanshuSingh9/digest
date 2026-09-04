use digest_lib::{DigestService, DigestTools, ReadArtifactInput, WriteAnalysisInput};
use std::sync::Arc;

#[test]
fn digest_tools_expose_schema_specific_analysis_write_and_artifact_read() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let tools = DigestTools::new(service);

    let written = tools
        .write_analysis(WriteAnalysisInput {
            job_id: "job-1".into(),
            article_id: "article-1".into(),
            summary: "A tool-written analysis.".into(),
        })
        .expect("write analysis through tool facade");
    let read = tools
        .read_artifact(ReadArtifactInput {
            artifact_id: written.artifact_id.clone(),
        })
        .expect("read analysis through tool facade");

    assert_eq!(read.artifact.artifact_id, written.artifact_id);
    assert_eq!(read.artifact.payload["summary"], "A tool-written analysis.");
}
