use digest_lib::{ArticleBlock, ArticleIngestionService, ArtifactKind, DigestService};
use std::sync::Arc;

const ARTICLE_HTML: &str = r#"
<!doctype html>
<html>
  <head><title>Fallback title</title></head>
  <body>
    <nav><p>Navigation should not be extracted.</p></nav>
    <article>
      <h1>Small feedback loops</h1>
      <p>Short loops reveal mistakes while context is fresh.</p>
      <h2>Use evidence</h2>
      <pre><code class="language-rust">fn main() {
    assert!(result.is_ok());
}</code></pre>
      <blockquote><p>Keep the source immutable.</p></blockquote>
      <img src="/diagram.png" alt="A feedback loop diagram">
    </article>
  </body>
</html>
"#;

#[test]
fn extraction_prefers_article_content_and_resolves_images() {
    let article = ArticleIngestionService::extract(
        "https://example.com/posts/feedback",
        ARTICLE_HTML.as_bytes(),
    )
    .expect("extract article");

    assert_eq!(article.title.as_deref(), Some("Small feedback loops"));
    assert!(article.blocks.iter().any(|block| matches!(
        block,
        ArticleBlock::Heading { level: 2, text } if text == "Use evidence"
    )));
    assert!(article.blocks.iter().any(|block| matches!(
        block,
        ArticleBlock::Code { language, text }
            if language.as_deref() == Some("rust")
                && text == "fn main() {\n    assert!(result.is_ok());\n}"
    )));
    assert_eq!(
        article
            .blocks
            .iter()
            .filter(|block| format!("{block:?}").contains("Keep the source immutable."))
            .count(),
        1,
        "paragraphs nested in a quote must not be duplicated as standalone blocks"
    );
    assert!(!article
        .blocks
        .iter()
        .any(|block| format!("{block:?}").contains("Navigation")));
    assert_eq!(
        article.images[0].source_url,
        "https://example.com/diagram.png"
    );
}

#[test]
fn captured_article_persists_raw_source_and_normalized_artifacts() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let ingestion = ArticleIngestionService::new(service.clone());

    let result = ingestion
        .persist_response(
            "job-1",
            "https://example.com/posts/feedback",
            "https://example.com/posts/feedback",
            200,
            Some("text/html; charset=utf-8"),
            ARTICLE_HTML.as_bytes(),
        )
        .expect("persist ingestion");

    assert_eq!(result.source.kind, ArtifactKind::SourceCapture);
    assert_eq!(result.article.kind, ArtifactKind::NormalizedArticle);
    assert_eq!(
        result.article.payload["sourceArtifactId"],
        result.source.artifact_id
    );
    assert!(directory
        .path()
        .join("artifacts")
        .join("objects")
        .join(format!("{}.bin", result.source.content_hash))
        .exists());
    assert_eq!(service.list_artifacts("job-1").unwrap().len(), 2);
}

#[tokio::test]
async fn fetch_rejects_loopback_targets_before_network_access() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let ingestion = ArticleIngestionService::new(service);

    let error = ingestion
        .ingest_url("job-1", "http://127.0.0.1/private")
        .await
        .expect_err("loopback URL must be rejected");

    assert!(error.to_string().contains("public"));
}
