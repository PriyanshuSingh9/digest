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

const ARTICLE_SCHEMA_VERSION: &str = "1.0";
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Debug, Error)]
pub enum IngestionError {
    #[error("invalid article URL: {0}")]
    InvalidUrl(String),
    #[error("article URL must resolve to a public HTTP address")]
    NonPublicAddress,
    #[error("article response exceeded the {0} byte limit")]
    ResponseTooLarge(usize),
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
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    List {
        ordered: bool,
        items: Vec<String>,
    },
    Quote {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleImage {
    pub source_url: String,
    pub alt: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedArticle {
    pub schema_version: String,
    pub canonical_url: String,
    pub title: Option<String>,
    pub blocks: Vec<ArticleBlock>,
    pub images: Vec<ArticleImage>,
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
        let source_payload = serde_json::json!({
            "requestedUrl": response.requested_url,
            "finalUrl": response.final_url,
            "status": response.status,
            "contentType": response.content_type,
            "byteLength": response.body.len(),
            "redirectCount": response.redirect_count,
            "capturedAtMs": now_ms(),
        });
        let source = self.digest.persist_binary_artifact(
            job_id,
            ArtifactKind::SourceCapture,
            response.body,
            source_payload,
        )?;
        let mut article_payload = serde_json::to_value(article)?;
        article_payload
            .as_object_mut()
            .expect("normalized article serializes as an object")
            .insert(
                "sourceArtifactId".into(),
                serde_json::Value::String(source.artifact_id.clone()),
            );
        let article = self.digest.persist_json_artifact(
            job_id,
            ArtifactKind::NormalizedArticle,
            article_payload,
        )?;
        Ok(IngestionResult { source, article })
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
            if let Some(block) = article_block(element)? {
                blocks.push(block);
            }
        }
        let image_selector = selector("img[src]")?;
        let images = root
            .select(&image_selector)
            .filter_map(|element| {
                let source = element.value().attr("src")?;
                let source_url = base_url.join(source).ok()?.to_string();
                Some(ArticleImage {
                    source_url,
                    alt: optional_text(element.value().attr("alt")),
                    title: optional_text(element.value().attr("title")),
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
        Ok(NormalizedArticle {
            schema_version: ARTICLE_SCHEMA_VERSION.into(),
            canonical_url: base_url.to_string(),
            title,
            blocks,
            images,
        })
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
        self.persist_captured_response(
            job_id,
            CapturedResponse {
                requested_url,
                final_url: &final_url,
                status: status.as_u16(),
                content_type: content_type.as_deref(),
                body: &body,
                redirect_count,
            },
        )
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
    for query in ["article", "main", "body"] {
        if let Some(root) = document.select(&selector(query)?).next() {
            return Ok(root);
        }
    }
    Err(IngestionError::Extraction(
        "document has no article, main, or body element".into(),
    ))
}

fn article_block(element: ElementRef<'_>) -> Result<Option<ArticleBlock>, IngestionError> {
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
            level: tag[1..].parse().unwrap_or(1),
            text,
        },
        "p" => ArticleBlock::Paragraph { text },
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
            ArticleBlock::Code { language, text }
        }
        "ul" | "ol" => {
            let item_selector = selector(":scope > li")?;
            let items = element
                .select(&item_selector)
                .map(element_text)
                .filter(|item| !item.is_empty())
                .collect();
            ArticleBlock::List {
                ordered: tag == "ol",
                items,
            }
        }
        "blockquote" => ArticleBlock::Quote { text },
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
