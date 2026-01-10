//! Hybrid fetching engine with RSS and Telegram HTML support.
//! Implements stealth mode with browser-like headers.

use crate::consts::{headers, limits, selectors, Source, SourceType};
use crate::utils::{clean_text, fibonacci_delay, progressive_delay, truncate_text};
use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

/// Network operation errors
#[derive(Error, Debug)]
pub enum FetchError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Rate limited (429)")]
    RateLimited,

    #[error("Forbidden (403)")]
    Forbidden,

    #[error("Not found (404)")]
    NotFound,

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Empty response")]
    Empty,
}

/// News item from any source
#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub link: Option<String>,
    pub timestamp: Option<String>,
}

impl NewsItem {
    fn new(title: String) -> Self {
        Self {
            title,
            link: None,
            timestamp: None,
        }
    }

    fn with_link(mut self, link: Option<String>) -> Self {
        self.link = link;
        self
    }
}

/// High-performance news fetching engine
pub struct NewsEngine {
    client: Client,
    tg_wrap_selector: Selector,
    tg_text_selector: Selector,
    tg_date_selector: Selector,
}

impl NewsEngine {
    /// Create new engine with optimized HTTP client
    pub fn new() -> Arc<Self> {
        let client = Client::builder()
            .user_agent(headers::USER_AGENT)
            .timeout(Duration::from_secs(limits::REQUEST_TIMEOUT_SECS))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(5)
            .gzip(true)
            .brotli(true)
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .expect("Failed to build HTTP client");

        Arc::new(Self {
            client,
            tg_wrap_selector: Selector::parse(selectors::TG_MESSAGE_WRAP)
                .expect("Invalid TG wrap selector"),
            tg_text_selector: Selector::parse(selectors::TG_MESSAGE_TEXT)
                .expect("Invalid TG text selector"),
            tg_date_selector: Selector::parse(selectors::TG_MESSAGE_DATE)
                .expect("Invalid TG date selector"),
        })
    }

    /// Fetch news from source with automatic type detection
    pub async fn fetch(&self, source: &Source) -> Result<Vec<NewsItem>, FetchError> {
        // Apply stealth delay
        fibonacci_delay(limits::BASE_DELAY_MS).await;

        match source.source_type {
            SourceType::TelegramHtml => self.fetch_telegram(source.url).await,
            SourceType::Rss => self.fetch_rss(source.url).await,
        }
    }

    /// Fetch with retry logic
    pub async fn fetch_with_retry(
        &self,
        source: &Source,
        max_attempts: u32,
    ) -> Result<Vec<NewsItem>, FetchError> {
        let mut last_error = FetchError::Empty;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                log::warn!("Retry {} for {}", attempt, source.name);
                progressive_delay(limits::BASE_DELAY_MS, attempt).await;
            }

            match self.fetch(source).await {
                Ok(items) => return Ok(items),
                Err(FetchError::RateLimited) => {
                    log::warn!("Rate limited on {}, backing off", source.name);
                    progressive_delay(2000, attempt + 1).await;
                    last_error = FetchError::RateLimited;
                }
                Err(e) => {
                    last_error = e;
                }
            }
        }

        Err(last_error)
    }

    /// Fetch Telegram web mirror (HTML scraping)
    async fn fetch_telegram(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let response = self.client
            .get(url)
            .header("Accept", headers::ACCEPT_HTML)
            .header("Accept-Language", headers::ACCEPT_LANG)
            .header("Accept-Encoding", headers::ACCEPT_ENCODING)
            .header("Cache-Control", "no-cache")
            .header("Pragma", "no-cache")
            .header("Sec-Fetch-Dest", "document")
            .header("Sec-Fetch-Mode", "navigate")
            .header("Sec-Fetch-Site", "none")
            .header("Sec-Fetch-User", "?1")
            .header("Upgrade-Insecure-Requests", "1")
            .send()
            .await?;

        self.check_status(&response.status())?;

        let html = response.text().await?;
        self.parse_telegram_html(&html)
    }

    /// Parse Telegram HTML structure
    fn parse_telegram_html(&self, html: &str) -> Result<Vec<NewsItem>, FetchError> {
        let document = Html::parse_document(html);
        let mut items = Vec::new();

        for element in document.select(&self.tg_wrap_selector).take(limits::MAX_ITEMS_PER_SOURCE) {
            // Extract text
            let text = if let Some(text_el) = element.select(&self.tg_text_selector).next() {
                text_el.text().collect::<String>()
            } else {
                continue;
            };

            let cleaned = clean_text(&text);
            if cleaned.is_empty() {
                continue;
            }

            // Extract link from date element
            let link = element
                .select(&self.tg_date_selector)
                .next()
                .and_then(|el| el.value().attr("href"))
                .map(|s| s.to_string());

            items.push(NewsItem::new(cleaned).with_link(link));
        }

        if items.is_empty() {
            return Err(FetchError::Empty);
        }

        Ok(items)
    }

    /// Fetch RSS/XML feed
    async fn fetch_rss(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let response = self.client
            .get(url)
            .header("Accept", headers::ACCEPT_RSS)
            .header("Accept-Language", headers::ACCEPT_LANG)
            .header("Accept-Encoding", headers::ACCEPT_ENCODING)
            .send()
            .await?;

        self.check_status(&response.status())?;

        let bytes = response.bytes().await?;
        self.parse_rss(&bytes)
    }

    /// Parse RSS/Atom feed
    fn parse_rss(&self, bytes: &[u8]) -> Result<Vec<NewsItem>, FetchError> {
        let feed = feed_rs::parser::parse(bytes)
            .map_err(|e| FetchError::Parse(e.to_string()))?;

        let items: Vec<NewsItem> = feed
            .entries
            .into_iter()
            .take(limits::MAX_ITEMS_PER_SOURCE)
            .filter_map(|entry| {
                let title = entry.title?;
                let cleaned = clean_text(&title.content);

                if cleaned.is_empty() {
                    return None;
                }

                let link = entry.links.first().map(|l| l.href.clone());
                Some(NewsItem::new(cleaned).with_link(link))
            })
            .collect();

        if items.is_empty() {
            return Err(FetchError::Empty);
        }

        Ok(items)
    }

    /// Check HTTP status and convert to error
    #[inline]
    fn check_status(&self, status: &StatusCode) -> Result<(), FetchError> {
        match *status {
            StatusCode::OK => Ok(()),
            StatusCode::TOO_MANY_REQUESTS => Err(FetchError::RateLimited),
            StatusCode::FORBIDDEN => Err(FetchError::Forbidden),
            StatusCode::NOT_FOUND => Err(FetchError::NotFound),
            _ => Ok(()), // Let other statuses through
        }
    }
}

/// Format fetch results for Telegram message (Gothic Style)
pub fn format_results(source_name: &str, items: &[NewsItem]) -> String {
    // Gothic header
    let mut output = format!("🏴 *{}*\n", escape_markdown(source_name));

    for item in items.iter() {
        let text = truncate_text(&item.title, limits::MAX_TEXT_LENGTH);
        output.push_str(&format!("▪️ {}", escape_markdown(&text)));

        // Add link if available
        if let Some(link) = &item.link {
            output.push_str(&format!(" [⛓️]({})", link));
        }
        output.push('\n');
    }

    output
}

/// Format error for Telegram message
pub fn format_error(source_name: &str, error: &FetchError) -> String {
    format!("🕸 *{}*: {}\n", escape_markdown(source_name), error)
}

/// Escape special Markdown characters for Telegram
fn escape_markdown(text: &str) -> String {
    text.replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('`', "\\`")
        .replace('(', "\\(") // Link parens
        .replace(')', "\\)")
}

/// Convenience function to create Arc-wrapped engine
pub fn create_engine() -> Arc<NewsEngine> {
    NewsEngine::new()
}