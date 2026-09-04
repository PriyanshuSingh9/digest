use digest_lib::{
    ArticleBlock, ArticleIngestionService, ArtifactKind, DigestService, ImageCaptureStatus,
};
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
      <figure>
        <img
          src="/diagram.png"
          srcset="/diagram-small.png 640w, /diagram.png 1280w"
          alt="A feedback loop diagram"
          width="1280"
          height="720"
        >
        <figcaption>Evidence closes the loop.</figcaption>
      </figure>
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
        ArticleBlock::Heading { level: 2, text, .. } if text == "Use evidence"
    )));
    assert!(article.blocks.iter().any(|block| matches!(
        block,
        ArticleBlock::Code { language, text, .. }
            if language.as_deref() == Some("rust")
                && text == "fn main() {\n    assert!(result.is_ok());\n}"
    )));
    assert!(matches!(
        &article.blocks[0],
        ArticleBlock::Heading { id, .. } if id == "block-1"
    ));
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
    assert_eq!(article.images[0].width, Some(1280));
    assert_eq!(article.images[0].height, Some(720));
    assert_eq!(
        article.images[0].caption.as_deref(),
        Some("Evidence closes the loop.")
    );
    assert_eq!(article.images[0].srcset_candidates.len(), 2);
    assert_eq!(
        article.images[0].capture_status,
        ImageCaptureStatus::Pending
    );
    assert!(article.diagnostics.confidence >= 60);
    assert_eq!(article.diagnostics.image_count, 1);
}

#[test]
fn extraction_selects_the_most_substantial_article_candidate() {
    let html = br#"
        <html><body>
          <article><p>Teaser only.</p></article>
          <article>
            <h1>Complete article</h1>
            <p>This is the first substantial paragraph with useful source material.</p>
            <p>This is the second substantial paragraph with supporting evidence.</p>
          </article>
        </body></html>
    "#;

    let article = ArticleIngestionService::extract("https://example.com/story", html)
        .expect("extract strongest article candidate");

    assert_eq!(article.title.as_deref(), Some("Complete article"));
    assert_eq!(article.blocks.len(), 3);
    assert!(!format!("{:?}", article.blocks).contains("Teaser only"));
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

#[test]
fn localized_image_records_bytes_and_provenance_as_an_immutable_artifact() {
    let directory = tempfile::tempdir().expect("create temporary data directory");
    let service = Arc::new(DigestService::open(directory.path()).expect("open Digest service"));
    let ingestion = ArticleIngestionService::new(service.clone());
    let mut image = ArticleIngestionService::extract(
        "https://example.com/posts/feedback",
        ARTICLE_HTML.as_bytes(),
    )
    .expect("extract article")
    .images
    .remove(0);

    ingestion
        .persist_image_asset(
            "job-1",
            &mut image,
            "application/octet-stream",
            b"\x89PNG\r\n\x1a\nimage",
        )
        .expect("persist localized image");

    assert_eq!(image.capture_status, ImageCaptureStatus::Localized);
    assert_eq!(image.mime_type.as_deref(), Some("image/png"));
    assert_eq!(image.byte_length, Some(13));
    let artifact_id = image.artifact_id.as_deref().expect("localized artifact ID");
    assert_eq!(
        service.read_artifact(artifact_id).unwrap().kind,
        ArtifactKind::ImageAsset
    );
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
