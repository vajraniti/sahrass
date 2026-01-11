//! Hybrid fetching engine with RSS and Telegram HTML support.

use crate::consts::{headers, limits, selectors, Source, SourceType};
use crate::utils::{clean_text, fibonacci_delay, progressive_delay, truncate_text, is_junk};
use crate::translate::translate_text;
use reqwest::{Client, StatusCode};
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use chrono::Local;
use futures::future::join_all;

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

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub link: Option<String>,
    pub time_str: String,
}

impl NewsItem {
    fn new(title: String, time_str: String) -> Self {
        Self { title, link: None, time_str }
    }
    fn with_link(mut self, link: Option<String>) -> Self {
        self.link = link;
        self
    }
}

pub struct NewsEngine {
    client: Client,
    tg_wrap_selector: Selector,
    tg_text_selector: Selector,
    tg_date_selector: Selector,
}

impl NewsEngine {
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
            tg_wrap_selector: Selector::parse(selectors::TG_MESSAGE_WRAP).expect("Invalid selector"),
            tg_text_selector: Selector::parse(selectors::TG_MESSAGE_TEXT).expect("Invalid selector"),
            tg_date_selector: Selector::parse(selectors::TG_MESSAGE_DATE).expect("Invalid selector"),
        })
    }

    pub async fn fetch(&self, source: &Source) -> Result<Vec<NewsItem>, FetchError> {
        fibonacci_delay(limits::BASE_DELAY_MS).await;

        let mut items = match source.source_type {
            SourceType::TelegramHtml => self.fetch_telegram(source.url).await?,
            SourceType::Rss => self.fetch_rss(source.url).await?,
        };

        // Parallel Translation
        let mut tasks = Vec::new();
        for item in &items {
            let client = self.client.clone();
            let text = item.title.clone();

            tasks.push(tokio::spawn(async move {
                // Translate to Russian
                match translate_text(&client, &text, "ru").await {
                    Ok(t) => t,
                    Err(_) => text, // Fallback to original
                }
            }));
        }

        let results = join_all(tasks).await;

        for (i, res) in results.into_iter().enumerate() {
            if let Ok(translated) = res {
                items[i].title = translated;
            }
        }

        Ok(items)
    }

    pub async fn fetch_with_retry(&self, source: &Source, max_attempts: u32) -> Result<Vec<NewsItem>, FetchError> {
        let mut last_error = FetchError::Empty;
        for attempt in 0..max_attempts {
            if attempt > 0 { progressive_delay(limits::BASE_DELAY_MS, attempt).await; }
            match self.fetch(source).await {
                Ok(items) => return Ok(items),
                Err(FetchError::RateLimited) => {
                    progressive_delay(2000, attempt + 1).await;
                    last_error = FetchError::RateLimited;
                }
                Err(e) => last_error = e,
            }
        }
        Err(last_error)
    }

    async fn fetch_telegram(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let response = self.client.get(url)
            .header("Accept", headers::ACCEPT_HTML)
            .header("Accept-Language", headers::ACCEPT_LANG)
            .header("Sec-Fetch-User", "?1")
            .header("Upgrade-Insecure-Requests", "1")
            .send().await?;

        self.check_status(&response.status())?;
        let html = response.text().await?;
        self.parse_telegram_html(&html)
    }

    fn parse_telegram_html(&self, html: &str) -> Result<Vec<NewsItem>, FetchError> {
        let document = Html::parse_document(html);
        let mut items = Vec::new();

        // Select ALL message wraps available on the page
        let elements: Vec<_> = document.select(&self.tg_wrap_selector).collect();

        // Iterate backwards (from Newest to Oldest)
        // We keep looking until we fill our buffer (MAX_ITEMS_PER_SOURCE)
        // This solves the problem where the last 10 posts are junk/links
        for element in elements.into_iter().rev() {
            if items.len() >= limits::MAX_ITEMS_PER_SOURCE {
                break;
            }

            let text = if let Some(text_el) = element.select(&self.tg_text_selector).next() {
                text_el.text().collect::<String>()
            } else {
                continue;
            };

            let cleaned = clean_text(&text);

            // Apply Filters
            if cleaned.is_empty() || is_junk(&cleaned) {
                continue;
            }

            let mut link = None;
            let mut time_str = "--:--".to_string();

            if let Some(date_el) = element.select(&self.tg_date_selector).next() {
                link = date_el.value().attr("href").map(|s| s.to_string());
                let raw_time = date_el.text().collect::<String>();
                if !raw_time.is_empty() {
                    time_str = raw_time;
                }
            }
            items.push(NewsItem::new(cleaned, time_str).with_link(link));
        }

        if items.is_empty() {
            // Log that we found elements but filtered them all, or found none
            // This helps distinguish between "Banned" (0 elements) and "All Junk"
            return Err(FetchError::Empty);
        }

        // We collected [Newest, 2nd Newest, ...].
        // Reverse to get [Oldest, ..., Newest] for the chat
        items.reverse();

        Ok(items)
    }

    async fn fetch_rss(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let response = self.client.get(url).header("Accept", headers::ACCEPT_RSS).send().await?;
        self.check_status(&response.status())?;
        let bytes = response.bytes().await?;
        self.parse_rss(&bytes)
    }

    fn parse_rss(&self, bytes: &[u8]) -> Result<Vec<NewsItem>, FetchError> {
        let feed = feed_rs::parser::parse(bytes).map_err(|e| FetchError::Parse(e.to_string()))?;

        let mut items: Vec<NewsItem> = feed.entries.into_iter()
            .filter_map(|entry| {
                let title = entry.title?;
                let cleaned = clean_text(&title.content);
                if cleaned.is_empty() || is_junk(&cleaned) { return None; }

                let link = entry.links.first().map(|l| l.href.clone());
                let time_str = entry.published.or(entry.updated)
                    .map(|dt| dt.with_timezone(&Local).format("%H:%M").to_string())
                    .unwrap_or_else(|| "--:--".to_string());

                Some(NewsItem::new(cleaned, time_str).with_link(link))
            })
            .take(limits::MAX_ITEMS_PER_SOURCE)
            .collect();

        if items.is_empty() { return Err(FetchError::Empty); }
        items.reverse();
        Ok(items)
    }

    fn check_status(&self, status: &StatusCode) -> Result<(), FetchError> {
        match *status {
            StatusCode::OK => Ok(()),
            StatusCode::TOO_MANY_REQUESTS => Err(FetchError::RateLimited),
            StatusCode::FORBIDDEN => Err(FetchError::Forbidden),
            StatusCode::NOT_FOUND => Err(FetchError::NotFound),
            _ => Ok(()),
        }
    }
}

pub fn format_results(source_name: &str, items: &[NewsItem]) -> String {
    let mut output = format!("🏴 *{}*\n", escape_markdown(source_name));
    for item in items.iter() {
        let text = truncate_text(&item.title, limits::MAX_TEXT_LENGTH);

        output.push_str(&format!("\n▪️ {}\n", escape_markdown(&text)));
        output.push_str("   └ 🕷 `");
        output.push_str(&escape_markdown(&item.time_str));
        output.push_str("`");

        if let Some(link) = &item.link {
            output.push_str(&format!("  ⛓️ [Link]({})", link));
        }
        output.push('\n');
    }
    output
}

pub fn format_error(source_name: &str, error: &FetchError) -> String {
    format!("🕸 *{}*: {}\n", escape_markdown(source_name), error)
}

fn escape_markdown(text: &str) -> String {
    text.replace('*', "\\*").replace('_', "\\_").replace('[', "\\[")
        .replace(']', "\\]").replace('`', "\\`").replace('(', "\\(").replace(')', "\\)")
}

pub fn create_engine() -> Arc<NewsEngine> { NewsEngine::new() }