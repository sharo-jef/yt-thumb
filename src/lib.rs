use anyhow::{anyhow, bail, Context, Result};
use reqwest::StatusCode;
use url::Url;

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// YouTube thumbnail resolution variants, ordered from highest to lowest.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    MaxRes,
    Sd,
    High,
    Medium,
    Default,
}

impl Resolution {
    pub fn filename(&self) -> &str {
        match self {
            Resolution::MaxRes => "maxresdefault.jpg",
            Resolution::Sd => "sddefault.jpg",
            Resolution::High => "hqdefault.jpg",
            Resolution::Medium => "mqdefault.jpg",
            Resolution::Default => "default.jpg",
        }
    }

    /// All resolutions in descending quality order.
    pub fn all() -> Vec<Resolution> {
        vec![
            Resolution::MaxRes,
            Resolution::Sd,
            Resolution::High,
            Resolution::Medium,
            Resolution::Default,
        ]
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Resolution::MaxRes => "maxres (1280×720)",
            Resolution::Sd => "sd (640×480)",
            Resolution::High => "hq (480×360)",
            Resolution::Medium => "mq (320×180)",
            Resolution::Default => "default (120×90)",
        };
        write!(f, "{}", label)
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Build the thumbnail URL for a given video ID and resolution.
pub fn thumbnail_url(base_url: &str, video_id: &str, resolution: &Resolution) -> String {
    format!(
        "{}/vi/{}/{}",
        base_url.trim_end_matches('/'),
        video_id,
        resolution.filename()
    )
}

/// Returns `true` if `s` is a syntactically valid YouTube video ID
/// (exactly 11 characters from `[A-Za-z0-9_-]`).
pub fn is_valid_video_id(s: &str) -> bool {
    s.len() == 11 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Extract a YouTube video ID from a URL or a bare video ID string.
///
/// Supported formats:
/// - Bare video ID (`dQw4w9WgXcQ`)
/// - `https://www.youtube.com/watch?v=ID`
/// - `https://m.youtube.com/watch?v=ID`
/// - `https://youtu.be/ID`
/// - `https://www.youtube.com/embed/ID`
/// - `https://www.youtube.com/v/ID`
/// - `https://www.youtube.com/shorts/ID`
pub fn extract_video_id(input: &str) -> Result<String> {
    let trimmed = input.trim();

    if is_valid_video_id(trimmed) {
        return Ok(trimmed.to_string());
    }

    let url = Url::parse(trimmed)
        .with_context(|| format!("Not a valid URL or video ID: {}", trimmed))?;

    let host = url.host_str().unwrap_or("");

    if host.ends_with("youtube.com") {
        extract_from_youtube_com(&url, trimmed)
    } else if host == "youtu.be" {
        extract_from_youtu_be(&url, trimmed)
    } else {
        Err(anyhow!("Not a YouTube URL: {}", trimmed))
    }
}

fn extract_from_youtube_com(url: &Url, original: &str) -> Result<String> {
    let path = url.path();

    if path == "/watch" || path.starts_with("/watch/") {
        url.query_pairs()
            .find(|(k, _)| k == "v")
            .map(|(_, v)| v.to_string())
            .ok_or_else(|| anyhow!("Missing `v` query parameter in URL: {}", original))
    } else {
        // /embed/ID, /v/ID, /shorts/ID
        let mut segments = url.path_segments().unwrap_or_else(|| "".split('/'));
        let first = segments.next().unwrap_or("");
        let id = segments.next().unwrap_or("");

        if matches!(first, "embed" | "v" | "shorts") && !id.is_empty() {
            Ok(id.to_string())
        } else {
            Err(anyhow!("Unsupported youtube.com URL format: {}", original))
        }
    }
}

fn extract_from_youtu_be(url: &Url, original: &str) -> Result<String> {
    url.path_segments()
        .and_then(|mut s| s.next())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No video ID in youtu.be URL: {}", original))
}

// ---------------------------------------------------------------------------
// Downloader
// ---------------------------------------------------------------------------

/// Minimum byte size below which a response is treated as a placeholder image.
const MIN_THUMBNAIL_BYTES: usize = 2_000;

pub struct ThumbnailDownloader {
    client: reqwest::blocking::Client,
    base_url: String,
}

impl Default for ThumbnailDownloader {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbnailDownloader {
    pub fn new() -> Self {
        Self::with_base_url("https://img.youtube.com")
    }

    /// Create a downloader that fetches from `base_url`. Useful for testing.
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Download the highest available thumbnail for `video_id`.
    ///
    /// Tries each resolution in descending order. A `404` response means that
    /// resolution is not available and the next one is tried. Any other non-2xx
    /// response is returned as an error immediately (to surface rate limits,
    /// auth issues, etc.).
    pub fn download(&self, video_id: &str) -> Result<(Vec<u8>, Resolution)> {
        for resolution in Resolution::all() {
            let url = thumbnail_url(&self.base_url, video_id, &resolution);

            let response = self
                .client
                .get(&url)
                .send()
                .with_context(|| format!("Request failed: {}", url))?;

            let status = response.status();

            if status == StatusCode::NOT_FOUND {
                continue;
            }

            if !status.is_success() {
                bail!("HTTP {} from {}", status, url);
            }

            let bytes = response
                .bytes()
                .with_context(|| format!("Failed to read body from {}", url))?
                .to_vec();

            // Tiny responses are YouTube's placeholder "image not available" image.
            if bytes.len() < MIN_THUMBNAIL_BYTES {
                continue;
            }

            return Ok((bytes, resolution));
        }

        Err(anyhow!("No thumbnail available for video: {}", video_id))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_valid_video_id ---

    #[test]
    fn valid_video_id_alphanumeric() {
        assert!(is_valid_video_id("dQw4w9WgXcQ"));
    }

    #[test]
    fn valid_video_id_with_underscore_and_dash() {
        assert!(is_valid_video_id("abc_def-gh1")); // exactly 11 chars
    }

    #[test]
    fn invalid_video_id_too_short() {
        assert!(!is_valid_video_id("dQw4w9WgXc"));
    }

    #[test]
    fn invalid_video_id_too_long() {
        assert!(!is_valid_video_id("dQw4w9WgXcQQ"));
    }

    #[test]
    fn invalid_video_id_special_char() {
        assert!(!is_valid_video_id("dQw4w9WgX!Q"));
    }

    // --- extract_video_id ---

    #[test]
    fn extract_bare_video_id() {
        assert_eq!(
            extract_video_id("dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_watch_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_watch_url_with_extra_params() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=42&list=PL123").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_mobile_watch_url() {
        assert_eq!(
            extract_video_id("https://m.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_youtu_be() {
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_youtu_be_with_params() {
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ?t=30").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_embed_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/embed/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_v_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/v/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_from_shorts_url() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/shorts/dQw4w9WgXcQ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_trims_whitespace() {
        assert_eq!(
            extract_video_id("  dQw4w9WgXcQ  ").unwrap(),
            "dQw4w9WgXcQ"
        );
    }

    #[test]
    fn extract_error_non_youtube_url() {
        assert!(extract_video_id("https://example.com/watch?v=dQw4w9WgXcQ").is_err());
    }

    #[test]
    fn extract_error_invalid_string() {
        assert!(extract_video_id("not-a-url-or-id!!").is_err());
    }

    #[test]
    fn extract_error_watch_without_v_param() {
        assert!(extract_video_id("https://www.youtube.com/watch").is_err());
    }

    // --- thumbnail_url ---

    #[test]
    fn thumbnail_url_maxres() {
        assert_eq!(
            thumbnail_url("https://img.youtube.com", "dQw4w9WgXcQ", &Resolution::MaxRes),
            "https://img.youtube.com/vi/dQw4w9WgXcQ/maxresdefault.jpg"
        );
    }

    #[test]
    fn thumbnail_url_base_url_trailing_slash() {
        assert_eq!(
            thumbnail_url("https://img.youtube.com/", "abc", &Resolution::Default),
            "https://img.youtube.com/vi/abc/default.jpg"
        );
    }

    // --- ThumbnailDownloader (mocked HTTP) ---

    #[test]
    fn download_uses_maxres_when_available() {
        let mut server = mockito::Server::new();
        let large_body = vec![0xffu8; 50_000];

        let _m = server
            .mock("GET", "/vi/dQw4w9WgXcQ/maxresdefault.jpg")
            .with_status(200)
            .with_body(&large_body)
            .create();

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        let (data, resolution) = downloader.download("dQw4w9WgXcQ").unwrap();

        assert_eq!(resolution, Resolution::MaxRes);
        assert_eq!(data.len(), 50_000);
    }

    #[test]
    fn download_falls_back_when_maxres_is_404() {
        let mut server = mockito::Server::new();
        let large_body = vec![0xffu8; 50_000];

        let _m404 = server
            .mock("GET", "/vi/dQw4w9WgXcQ/maxresdefault.jpg")
            .with_status(404)
            .create();
        let _m200 = server
            .mock("GET", "/vi/dQw4w9WgXcQ/sddefault.jpg")
            .with_status(200)
            .with_body(&large_body)
            .create();

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        let (_, resolution) = downloader.download("dQw4w9WgXcQ").unwrap();

        assert_eq!(resolution, Resolution::Sd);
    }

    #[test]
    fn download_skips_placeholder_and_falls_back() {
        let mut server = mockito::Server::new();
        let placeholder = vec![0u8; 500]; // tiny = placeholder
        let real = vec![0xffu8; 50_000];

        let _m1 = server
            .mock("GET", "/vi/dQw4w9WgXcQ/maxresdefault.jpg")
            .with_status(200)
            .with_body(&placeholder)
            .create();
        let _m2 = server
            .mock("GET", "/vi/dQw4w9WgXcQ/sddefault.jpg")
            .with_status(404)
            .create();
        let _m3 = server
            .mock("GET", "/vi/dQw4w9WgXcQ/hqdefault.jpg")
            .with_status(200)
            .with_body(&real)
            .create();

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        let (_, resolution) = downloader.download("dQw4w9WgXcQ").unwrap();

        assert_eq!(resolution, Resolution::High);
    }

    #[test]
    fn download_errors_when_no_thumbnail_available() {
        let mut server = mockito::Server::new();

        for filename in &[
            "maxresdefault.jpg",
            "sddefault.jpg",
            "hqdefault.jpg",
            "mqdefault.jpg",
            "default.jpg",
        ] {
            server
                .mock("GET", format!("/vi/dQw4w9WgXcQ/{}", filename).as_str())
                .with_status(404)
                .create();
        }

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        assert!(downloader.download("dQw4w9WgXcQ").is_err());
    }

    #[test]
    fn download_errors_on_server_error() {
        let mut server = mockito::Server::new();

        let _m = server
            .mock("GET", "/vi/dQw4w9WgXcQ/maxresdefault.jpg")
            .with_status(500)
            .create();

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        let err = downloader.download("dQw4w9WgXcQ").unwrap_err();
        assert!(err.to_string().contains("500"), "Expected HTTP 500 error");
    }

    #[test]
    fn download_errors_on_rate_limit() {
        let mut server = mockito::Server::new();

        let _m = server
            .mock("GET", "/vi/dQw4w9WgXcQ/maxresdefault.jpg")
            .with_status(429)
            .create();

        let downloader = ThumbnailDownloader::with_base_url(server.url());
        let err = downloader.download("dQw4w9WgXcQ").unwrap_err();
        assert!(err.to_string().contains("429"), "Expected HTTP 429 error");
    }
}
