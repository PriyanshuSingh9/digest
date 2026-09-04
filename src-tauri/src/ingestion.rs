use crate::{ArtifactEnvelope, ArtifactKind, DigestError, DigestService};
use reqwest::{
    header::{CONTENT_TYPE, LOCATION},
    redirect::Policy,
    Client, Response,
};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, ToSocketAddrs},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use url::Url;

const ARTICLE_SCHEMA_VERSION: &str = "1.1";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_IMAGE_COUNT: usize = 12;
const MAX_TOTAL_IMAGE_BYTES: usize = 24 * 1024 * 1024;
const SUPPORTED_IMAGE_TYPES: &[&str] = &[
    "image/avif",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/webp",
];

#[derive(Debug, Error)]
pub enum IngestionError {
    #[error("invalid article URL: {0}")]
    InvalidUrl(String),
    #[error("article URL must resolve to a public HTTP address")]
    NonPublicAddress,
    #[error("article response exceeded the {0} byte limit")]
    ResponseTooLarge(usize),
    #[error("image response exceeded the {0} byte limit")]
    ImageTooLarge(usize),
    #[error("article request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("article response could not be read: {0}")]
    Io(#[from] std::io::Error),
    #[error("article artifact failed: {0}")]
    Digest(#[from] DigestError),
    #[error("article serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("article extraction failed: {0}")]
    Extraction(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArticleBlock {
    Heading {
        id: String,
        level: u8,
        text: String,
    },
    Paragraph {
        id: String,
        text: String,
    },
    Code {
        id: String,
        language: Option<String>,
        text: String,
    },
    List {
        id: String,
        ordered: bool,
        items: Vec<String>,
    },
    Quote {
        id: String,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleImage {
    pub id: String,
    pub original_url: String,
    pub source_url: String,
    pub alt: Option<String>,
    pub title: Option<String>,
    pub caption: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub srcset_candidates: Vec<String>,
    pub capture_status: ImageCaptureStatus,
    pub artifact_id: Option<String>,
    pub mime_type: Option<String>,
    pub content_hash: Option<String>,
    pub byte_length: Option<usize>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageCaptureStatus {
    Pending,
    Localized,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionDiagnostics {
    pub confidence: u8,
    pub word_count: usize,
    pub block_count: usize,
    pub image_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedArticle {
    pub schema_version: String,
    pub canonical_url: String,
    pub title: Option<String>,
    pub blocks: Vec<ArticleBlock>,
    pub images: Vec<ArticleImage>,
    pub diagnostics: ExtractionDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestionResult {
    pub source: ArtifactEnvelope,
    pub article: ArtifactEnvelope,
}

struct CapturedResponse<'a> {
    requested_url: &'a str,
    final_url: &'a str,
    status: u16,
    content_type: Option<&'a str>,
    body: &'a [u8],
    redirect_count: usize,
}

#[derive(Clone)]
pub struct ArticleIngestionService {
    digest: Arc<DigestService>,
    client: Client,
}

impl ArticleIngestionService {
    pub fn new(digest: Arc<DigestService>) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(Policy::none())
            .user_agent("Digest/0.1 article-capture")
            .build()
            .expect("build article HTTP client");
        Self { digest, client }
    }

    pub async fn ingest_url(
        &self,
        job_id: &str,
        requested_url: &str,
    ) -> Result<IngestionResult, IngestionError> {
        let mut url = parse_public_url(requested_url)?;
        for redirect_count in 0..=MAX_REDIRECTS {
            validate_public_url(&url)?;
            let response = self.client.get(url.clone()).send().await?;
            if response.status().is_redirection() {
                if redirect_count == MAX_REDIRECTS {
                    return Err(IngestionError::InvalidUrl(
                        "article exceeded the redirect limit".into(),
                    ));
                }
                url = redirect_target(&url, &response)?;
                continue;
            }
            return self
                .persist_http_response(job_id, requested_url, response, redirect_count)
                .await;
        }
        Err(IngestionError::InvalidUrl(
            "article exceeded the redirect limit".into(),
        ))
    }

    pub fn persist_response(
        &self,
        job_id: &str,
        requested_url: &str,
        final_url: &str,
        status: u16,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Result<IngestionResult, IngestionError> {
        self.persist_captured_response(
            job_id,
            CapturedResponse {
                requested_url,
                final_url,
                status,
                content_type,
                body,
                redirect_count: 0,
            },
        )
    }

    fn persist_captured_response(
        &self,
        job_id: &str,
        response: CapturedResponse<'_>,
    ) -> Result<IngestionResult, IngestionError> {
        if response.body.len() > MAX_RESPONSE_BYTES {
            return Err(IngestionError::ResponseTooLarge(MAX_RESPONSE_BYTES));
        }
        let article = Self::extract(response.final_url, response.body)?;
        let source = self.persist_source_capture(job_id, &response)?;
        let article = self.persist_normalized_article(job_id, &source, article)?;
        Ok(IngestionResult { source, article })
    }

    fn persist_source_capture(
        &self,
        job_id: &str,
        response: &CapturedResponse<'_>,
    ) -> Result<ArtifactEnvelope, IngestionError> {
        let source_payload = serde_json::json!({
            "requestedUrl": response.requested_url,
            "finalUrl": response.final_url,
            "status": response.status,
            "contentType": response.content_type,
            "byteLength": response.body.len(),
            "redirectCount": response.redirect_count,
            "capturedAtMs": now_ms(),
        });
        self.digest
            .persist_binary_artifact(
                job_id,
                ArtifactKind::SourceCapture,
                response.body,
                source_payload,
            )
            .map_err(Into::into)
    }

    fn persist_normalized_article(
        &self,
        job_id: &str,
        source: &ArtifactEnvelope,
        article: NormalizedArticle,
    ) -> Result<ArtifactEnvelope, IngestionError> {
        let mut article_payload = serde_json::to_value(article)?;
        article_payload
            .as_object_mut()
            .expect("normalized article serializes as an object")
            .insert(
                "sourceArtifactId".into(),
                serde_json::Value::String(source.artifact_id.clone()),
            );
        self.digest
            .persist_json_artifact(job_id, ArtifactKind::NormalizedArticle, article_payload)
            .map_err(Into::into)
    }

    pub fn extract(final_url: &str, body: &[u8]) -> Result<NormalizedArticle, IngestionError> {
        let base_url =
            Url::parse(final_url).map_err(|error| IngestionError::InvalidUrl(error.to_string()))?;
        let html = String::from_utf8_lossy(body);
        let document = Html::parse_document(&html);
        let root = select_root(&document)?;
        let block_selector = selector("h1,h2,h3,h4,h5,h6,p,pre,ul,ol,blockquote")?;
        let mut blocks = Vec::new();
        for element in root.select(&block_selector) {
            let id = format!("block-{}", blocks.len() + 1);
            if let Some(block) = article_block(element, id)? {
                blocks.push(block);
            }
        }
        let image_selector = selector("img[src]")?;
        let images: Vec<_> = root
            .select(&image_selector)
            .enumerate()
            .filter_map(|(index, element)| {
                let source = element.value().attr("src")?;
                let source_url = base_url.join(source).ok()?.to_string();
                Some(ArticleImage {
                    id: format!("image-{}", index + 1),
                    original_url: source.into(),
                    source_url,
                    alt: optional_text(element.value().attr("alt")),
                    title: optional_text(element.value().attr("title")),
                    caption: image_caption(element),
                    width: numeric_attribute(element, "width"),
                    height: numeric_attribute(element, "height"),
                    srcset_candidates: srcset_candidates(element, &base_url),
                    capture_status: ImageCaptureStatus::Pending,
                    artifact_id: None,
                    mime_type: None,
                    content_hash: None,
                    byte_length: None,
                    error: None,
                })
            })
            .collect();
        let title = first_text(root, "h1")?.or_else(|| {
            document
                .select(&Selector::parse("title").expect("valid title selector"))
                .next()
                .map(element_text)
                .filter(|text| !text.is_empty())
        });
        if blocks.is_empty() {
            return Err(IngestionError::Extraction(
                "no readable article blocks were found".into(),
            ));
        }
        let diagnostics = extraction_diagnostics(title.as_deref(), &blocks, images.len());
        Ok(NormalizedArticle {
            schema_version: ARTICLE_SCHEMA_VERSION.into(),
            canonical_url: base_url.to_string(),
            title,
            blocks,
            images,
            diagnostics,
        })
    }

    pub fn persist_image_asset(
        &self,
        job_id: &str,
        image: &mut ArticleImage,
        reported_mime_type: &str,
        bytes: &[u8],
    ) -> Result<(), IngestionError> {
        let mime_type = detect_image_mime(bytes).ok_or_else(|| {
            IngestionError::Extraction("image bytes have an unsupported format".into())
        })?;
        let payload = serde_json::json!({
            "imageId": image.id,
            "originalUrl": image.original_url,
            "resolvedUrl": image.source_url,
            "mimeType": mime_type,
            "reportedMimeType": reported_mime_type,
            "byteLength": bytes.len(),
        });
        let artifact = self.digest.persist_binary_artifact(
            job_id,
            ArtifactKind::ImageAsset,
            bytes,
            payload,
        )?;
        image.capture_status = ImageCaptureStatus::Localized;
        image.artifact_id = Some(artifact.artifact_id);
        image.mime_type = Some(mime_type.into());
        image.content_hash = Some(artifact.content_hash);
        image.byte_length = Some(bytes.len());
        image.error = None;
        Ok(())
    }

    async fn persist_http_response(
        &self,
        job_id: &str,
        requested_url: &str,
        mut response: Response,
        redirect_count: usize,
    ) -> Result<IngestionResult, IngestionError> {
        let status = response.status();
        if !status.is_success() {
            return Err(IngestionError::InvalidUrl(format!(
                "article returned HTTP {status}"
            )));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(IngestionError::ResponseTooLarge(MAX_RESPONSE_BYTES));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        if content_type.as_deref().is_some_and(|value| {
            let media_type = value.split(';').next().unwrap_or_default().trim();
            !matches!(media_type, "text/html" | "application/xhtml+xml")
        }) {
            return Err(IngestionError::Extraction(format!(
                "unsupported article content type: {}",
                content_type.as_deref().unwrap_or_default()
            )));
        }
        let final_url = response.url().to_string();
        let mut body = Vec::with_capacity(response.content_length().unwrap_or_default() as usize);
        while let Some(chunk) = response.chunk().await? {
            if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(IngestionError::ResponseTooLarge(MAX_RESPONSE_BYTES));
            }
            body.extend_from_slice(&chunk);
        }
        let captured = CapturedResponse {
            requested_url,
            final_url: &final_url,
            status: status.as_u16(),
            content_type: content_type.as_deref(),
            body: &body,
            redirect_count,
        };
        let source = self.persist_source_capture(job_id, &captured)?;
        let mut article = Self::extract(&final_url, &body)?;
        if tokio::time::timeout(
            Duration::from_secs(45),
            self.localize_images(job_id, &mut article),
        )
        .await
        .is_err()
        {
            for image in article
                .images
                .iter_mut()
                .filter(|image| image.capture_status == ImageCaptureStatus::Pending)
            {
                image.capture_status = ImageCaptureStatus::Failed;
                image.error = Some("image localization exceeded the total time limit".into());
            }
            article
                .diagnostics
                .warnings
                .push("Image localization exceeded the 45 second time limit.".into());
        }
        let article = self.persist_normalized_article(job_id, &source, article)?;
        Ok(IngestionResult { source, article })
    }

    async fn localize_images(&self, job_id: &str, article: &mut NormalizedArticle) {
        let mut total_bytes = 0;
        for (index, image) in article.images.iter_mut().enumerate() {
            if index >= MAX_IMAGE_COUNT {
                image.capture_status = ImageCaptureStatus::Failed;
                image.error = Some(format!("image limit of {MAX_IMAGE_COUNT} reached"));
                continue;
            }
            match self.fetch_image(&image.source_url).await {
                Ok((resolved_url, mime_type, bytes)) => {
                    if total_bytes + bytes.len() > MAX_TOTAL_IMAGE_BYTES {
                        image.capture_status = ImageCaptureStatus::Failed;
                        image.error = Some("total localized image byte limit reached".into());
                        continue;
                    }
                    image.source_url = resolved_url;
                    total_bytes += bytes.len();
                    if let Err(error) = self.persist_image_asset(job_id, image, &mime_type, &bytes)
                    {
                        image.capture_status = ImageCaptureStatus::Failed;
                        image.error = Some(error.to_string());
                    }
                }
                Err(error) => {
                    image.capture_status = ImageCaptureStatus::Failed;
                    image.error = Some(error.to_string());
                }
            }
        }
        let failed = article
            .images
            .iter()
            .filter(|image| image.capture_status == ImageCaptureStatus::Failed)
            .count();
        if failed > 0 {
            article
                .diagnostics
                .warnings
                .push(format!("{failed} image asset(s) could not be localized."));
        }
    }

    async fn fetch_image(
        &self,
        requested_url: &str,
    ) -> Result<(String, String, Vec<u8>), IngestionError> {
        let mut url = parse_public_url(requested_url)?;
        for redirect_count in 0..=MAX_REDIRECTS {
            validate_public_url(&url)?;
            let mut response = self.client.get(url.clone()).send().await?;
            if response.status().is_redirection() {
                if redirect_count == MAX_REDIRECTS {
                    return Err(IngestionError::InvalidUrl(
                        "image exceeded the redirect limit".into(),
                    ));
                }
                url = redirect_target(&url, &response)?;
                continue;
            }
            if !response.status().is_success() {
                return Err(IngestionError::InvalidUrl(format!(
                    "image returned HTTP {}",
                    response.status()
                )));
            }
            let mime_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.split(';').next())
                .map(str::trim)
                .filter(|value| SUPPORTED_IMAGE_TYPES.contains(value))
                .ok_or_else(|| {
                    IngestionError::Extraction("image MIME type is missing or unsupported".into())
                })?
                .to_owned();
            if response
                .content_length()
                .is_some_and(|length| length > MAX_IMAGE_BYTES as u64)
            {
                return Err(IngestionError::ImageTooLarge(MAX_IMAGE_BYTES));
            }
            let resolved_url = response.url().to_string();
            let mut bytes =
                Vec::with_capacity(response.content_length().unwrap_or_default() as usize);
            while let Some(chunk) = response.chunk().await? {
                if bytes.len() + chunk.len() > MAX_IMAGE_BYTES {
                    return Err(IngestionError::ImageTooLarge(MAX_IMAGE_BYTES));
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok((resolved_url, mime_type, bytes));
        }
        Err(IngestionError::InvalidUrl(
            "image exceeded the redirect limit".into(),
        ))
    }
}

fn parse_public_url(value: &str) -> Result<Url, IngestionError> {
    let url = Url::parse(value).map_err(|error| IngestionError::InvalidUrl(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(IngestionError::InvalidUrl(
            "only http and https URLs are supported".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IngestionError::InvalidUrl(
            "article URLs cannot contain credentials".into(),
        ));
    }
    validate_public_url(&url)?;
    Ok(url)
}

fn validate_public_url(url: &Url) -> Result<(), IngestionError> {
    let host = url
        .host_str()
        .ok_or_else(|| IngestionError::InvalidUrl("URL has no host".into()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| IngestionError::InvalidUrl("URL has no usable port".into()))?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| IngestionError::InvalidUrl(error.to_string()))?;
    let mut found = false;
    for address in addresses {
        found = true;
        if !is_public_ip(address.ip()) {
            return Err(IngestionError::NonPublicAddress);
        }
    }
    if !found {
        return Err(IngestionError::InvalidUrl(
            "host did not resolve to an address".into(),
        ));
    }
    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation())
        }
        IpAddr::V6(ip) => {
            !(ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

fn redirect_target(base: &Url, response: &Response) -> Result<Url, IngestionError> {
    let location = response
        .headers()
        .get(LOCATION)
        .ok_or_else(|| IngestionError::InvalidUrl("redirect omitted Location".into()))?
        .to_str()
        .map_err(|_| IngestionError::InvalidUrl("redirect Location was not text".into()))?;
    base.join(location)
        .map_err(|error| IngestionError::InvalidUrl(error.to_string()))
}

fn select_root(document: &Html) -> Result<ElementRef<'_>, IngestionError> {
    if let Some(root) = document
        .select(&selector("article,main")?)
        .max_by_key(|candidate| readability_score(*candidate))
    {
        return Ok(root);
    }
    if let Some(root) = document.select(&selector("body")?).next() {
        return Ok(root);
    }
    Err(IngestionError::Extraction(
        "document has no article, main, or body element".into(),
    ))
}

fn readability_score(element: ElementRef<'_>) -> usize {
    let paragraph_count = element
        .select(&Selector::parse("p").expect("valid paragraph selector"))
        .count();
    element_text(element).len() + paragraph_count * 80
}

fn article_block(
    element: ElementRef<'_>,
    id: String,
) -> Result<Option<ArticleBlock>, IngestionError> {
    let tag = element.value().name();
    if tag == "p"
        && element
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|ancestor| matches!(ancestor.value().name(), "blockquote" | "li"))
    {
        return Ok(None);
    }
    let text = element_text(element);
    if text.is_empty() {
        return Ok(None);
    }
    let block = match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => ArticleBlock::Heading {
            id,
            level: tag[1..].parse().unwrap_or(1),
            text,
        },
        "p" => ArticleBlock::Paragraph { id, text },
        "pre" => {
            let language = element
                .select(&selector("code")?)
                .next()
                .and_then(|code| code.value().attr("class"))
                .and_then(|class| {
                    class
                        .split_whitespace()
                        .find_map(|name| name.strip_prefix("language-"))
                })
                .map(str::to_owned);
            ArticleBlock::Code { id, language, text }
        }
        "ul" | "ol" => {
            let item_selector = selector(":scope > li")?;
            let items = element
                .select(&item_selector)
                .map(element_text)
                .filter(|item| !item.is_empty())
                .collect();
            ArticleBlock::List {
                id,
                ordered: tag == "ol",
                items,
            }
        }
        "blockquote" => ArticleBlock::Quote { id, text },
        _ => return Ok(None),
    };
    Ok(Some(block))
}

fn first_text(root: ElementRef<'_>, query: &str) -> Result<Option<String>, IngestionError> {
    Ok(root
        .select(&selector(query)?)
        .next()
        .map(element_text)
        .filter(|text| !text.is_empty()))
}

fn selector(value: &str) -> Result<Selector, IngestionError> {
    Selector::parse(value).map_err(|error| IngestionError::Extraction(error.to_string()))
}

fn element_text(element: ElementRef<'_>) -> String {
    element
        .text()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn numeric_attribute(element: ElementRef<'_>, name: &str) -> Option<u32> {
    element.value().attr(name)?.parse().ok()
}

fn image_caption(element: ElementRef<'_>) -> Option<String> {
    let figure = element
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|ancestor| ancestor.value().name() == "figure")?;
    figure
        .select(&Selector::parse("figcaption").expect("valid figcaption selector"))
        .next()
        .map(element_text)
        .filter(|caption| !caption.is_empty())
}

fn srcset_candidates(element: ElementRef<'_>, base_url: &Url) -> Vec<String> {
    element
        .value()
        .attr("srcset")
        .into_iter()
        .flat_map(|srcset| srcset.split(','))
        .filter_map(|candidate| candidate.split_whitespace().next())
        .filter_map(|candidate| base_url.join(candidate).ok())
        .map(|candidate| candidate.to_string())
        .collect()
}

fn detect_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(&bytes[8..12], b"avif" | b"avis")
    {
        Some("image/avif")
    } else {
        None
    }
}

fn extraction_diagnostics(
    title: Option<&str>,
    blocks: &[ArticleBlock],
    image_count: usize,
) -> ExtractionDiagnostics {
    let text = blocks
        .iter()
        .map(|block| match block {
            ArticleBlock::Heading { text, .. }
            | ArticleBlock::Paragraph { text, .. }
            | ArticleBlock::Code { text, .. }
            | ArticleBlock::Quote { text, .. } => text.as_str(),
            ArticleBlock::List { .. } => "",
        })
        .collect::<Vec<_>>()
        .join(" ");
    let word_count = text.split_whitespace().count();
    let mut confidence = 20_u8;
    if title.is_some() {
        confidence += 25;
    }
    confidence += (blocks.len().min(3) * 10) as u8;
    if text.len() >= 80 {
        confidence += 25;
    }
    let mut warnings = Vec::new();
    if title.is_none() {
        warnings.push("No article title was found.".into());
    }
    if word_count < 40 {
        warnings.push("Extracted article text is unusually short.".into());
    }
    ExtractionDiagnostics {
        confidence: confidence.min(100),
        word_count,
        block_count: blocks.len(),
        image_count,
        warnings,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
