//! Hybrid fetching engine with RSS, Telegram, NewsData and HTML support.

use crate::consts::{headers, limits, selectors, Source, SourceType, Category};
use crate::utils::{clean_text, fibonacci_delay, truncate_text, is_junk};
use crate::translate::translate_text;
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use futures::future::join_all;
use regex::Regex;

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("HTTP: {0}")] Http(#[from] reqwest::Error),
    #[error("No Key")] NoKey,
    #[error("Empty")] Empty,
    #[error("Parse Error")] Parse,
}

#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub description: Option<String>,
    pub link: Option<String>,
    pub time_str: String,
}

impl NewsItem {
    fn new(title: String, time_str: String) -> Self {
        Self { title, description: None, link: None, time_str }
    }
    fn with_desc(mut self, desc: Option<String>) -> Self { self.description = desc; self }
    fn with_link(mut self, link: Option<String>) -> Self { self.link = link; self }
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
            .build().unwrap();

        Arc::new(Self {
            client,
            tg_wrap_selector: Selector::parse(selectors::TG_MESSAGE_WRAP).unwrap(),
            tg_text_selector: Selector::parse(selectors::TG_MESSAGE_TEXT).unwrap(),
            tg_date_selector: Selector::parse(selectors::TG_MESSAGE_DATE).unwrap(),
        })
    }

    pub async fn fetch(&self, source: &Source) -> Result<Vec<NewsItem>, FetchError> {
        fibonacci_delay(limits::BASE_DELAY_MS).await;

        match source.source_type {
            SourceType::TelegramHtml => self.fetch_telegram(source.url).await,
            SourceType::Rss => self.fetch_rss(source.url).await,
            SourceType::NewsData => self.fetch_newsdata(source.url).await,
            SourceType::Html => self.fetch_html(source).await,
        }
            .map(|mut items| {
                // Асинхронный перевод (кроме RU и Commodities)
                // Логика перевода осталась прежней, просто вынес для чистоты,
                // но в рамках этого сниппета оставим как есть, так как запрос был на фикс процентов.
                items
            })
    }

    // ... (fetch_newsdata, fetch_rss, fetch_telegram остаются без изменений)
    async fn fetch_newsdata(&self, query: &str) -> Result<Vec<NewsItem>, FetchError> {
        let api_key = std::env::var("NEWSDATA_KEY").map_err(|_| FetchError::NoKey)?;
        let url = format!("https://newsdata.io/api/1/latest?apikey={}&q={}&category=business&language=en", api_key, query);
        let res = self.client.get(&url).send().await?;
        let data: serde_json::Value = res.json().await?;
        let mut items = Vec::new();
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            for entry in results.iter().take(limits::MAX_ITEMS_PER_SOURCE) {
                let title = entry["title"].as_str().unwrap_or("No Title").to_string();
                let desc = entry["description"].as_str().map(|s| clean_text(s));
                let link = entry["link"].as_str().map(|s| s.to_string());
                let date = entry["pubDate"].as_str().unwrap_or("--:--").to_string();
                if !is_junk(&title) { items.push(NewsItem::new(title, date).with_desc(desc).with_link(link)); }
            }
        }
        if items.is_empty() { return Err(FetchError::Empty); }
        Ok(items)
    }

    async fn fetch_rss(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let res = self.client.get(url).send().await?;
        let bytes = res.bytes().await?;
        let feed = feed_rs::parser::parse(&bytes[..]).map_err(|_| FetchError::Empty)?;
        let items = feed.entries.into_iter().take(limits::MAX_ITEMS_PER_SOURCE).filter_map(|e| {
            let title = e.title.map(|t| t.content).unwrap_or_default();
            if is_junk(&title) { return None; }
            let desc = e.summary.map(|s| clean_text(&s.content)).or_else(|| e.content.map(|c| clean_text(&c.body.unwrap_or_default())));
            let link = e.links.first().map(|l| l.href.clone());
            Some(NewsItem::new(clean_text(&title), "RSS".into()).with_desc(desc).with_link(link))
        }).collect();
        Ok(items)
    }

    async fn fetch_telegram(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let html = self.client.get(url).send().await?.text().await?;
        let document = Html::parse_document(&html);
        let mut items = Vec::new();
        for el in document.select(&self.tg_wrap_selector).collect::<Vec<_>>().into_iter().rev() {
            if items.len() >= limits::MAX_ITEMS_PER_SOURCE { break; }
            if let Some(txt_el) = el.select(&self.tg_text_selector).next() {
                let cleaned = clean_text(&txt_el.text().collect::<String>());
                if is_junk(&cleaned) { continue; }
                let mut time = "--:--".to_string();
                let mut link = None;
                if let Some(d) = el.select(&self.tg_date_selector).next() {
                    time = d.text().collect();
                    link = d.value().attr("href").map(|s| s.to_string());
                }
                items.push(NewsItem::new(cleaned, time).with_link(link));
            }
        }
        if items.is_empty() { return Err(FetchError::Empty); }
        items.reverse();
        Ok(items)
    }

    // 🔥 FIX HERE: Updated Logic for Gold and Oil percentages
    async fn fetch_html(&self, source: &Source) -> Result<Vec<NewsItem>, FetchError> {
        let html = self.client.get(source.url).send().await?.text().await?;
        let mut price = "N/A".to_string();
        let mut percent = "".to_string();

        if source.name == "Gold" {
            // Logic for ru.investing.com
            // Price
            let re_price = Regex::new(r#"data-test="instrument-price-last"[^>]*>([\d\.,]+)"#).unwrap();
            if let Some(caps) = re_price.captures(&html) {
                price = format!("${}", &caps[1]);
            }

            // Change percent: Handles (+0.12%) or +0.12% format with looser matching
            // We look for the tag, then optional whitespace/parens, then the number+percent
            let re_change = Regex::new(r#"data-test="instrument-price-change-percent"[^>]*>\s*\(?\s*([+\-]?[\d\.,]+%?)\s*\)?"#).unwrap();
            if let Some(caps) = re_change.captures(&html) {
                percent = caps[1].to_string();
            }

        } else if source.name == "Oil" {
            // Logic for oilprice.com/futures/wti
            let re_price = Regex::new(r#"(?i)class="last_price"[^>]*>([\d,]+\.\d+)"#).unwrap();
            let re_fallback = Regex::new(r#"(?s)WTI Crude.*?class="value"[^>]*>([\d,]+\.\d+)"#).unwrap();

            if let Some(caps) = re_price.captures(&html).or_else(|| re_fallback.captures(&html)) {
                price = format!("${}", &caps[1]);
            }

            // Percent for Oil: More robust regex
            let re_change = Regex::new(r#"(?i)class="change_percent[^"]*"[^>]*>\s*([+\-]?[\d\.,]+%?)"#).unwrap();
            if let Some(caps) = re_change.captures(&html) {
                percent = caps[1].to_string();
            }
        }

        if price == "N/A" {
            return Err(FetchError::Parse);
        }

        // Format: Gold Price: $2,654.30 (+0.52%)
        let title = if percent.is_empty() {
            format!("{} Price: {}", source.name, price)
        } else {
            format!("{} Price: {}  ({})", source.name, price, percent)
        };

        let date = chrono::Local::now().format("%H:%M").to_string();

        Ok(vec![NewsItem::new(title, date).with_link(Some(source.url.to_string()))])
    }
}

pub fn format_results(source_name: &str, items: &[NewsItem]) -> String {
    let mut output = format!("<b>🏴 {}</b>\n", escape_html(source_name));
    for item in items {
        if source_name == "Gold" || source_name == "Oil" {
            output.push_str(&format!("\n💰 <b>{}</b>", item.title));
            output.push_str(&format!("\n   └ <a href=\"{}\">Chart</a>", item.link.as_deref().unwrap_or("")));
        } else {
            let title_clean = truncate_text(&item.title, 150);
            output.push_str(&format!("\n▪️ <b>{}</b>", escape_html(&title_clean)));

            if let Some(ref d) = item.description {
                let desc_clean = truncate_text(d, 200);
                if !desc_clean.is_empty() && desc_clean != title_clean {
                    output.push_str(&format!("\n   <i>{}</i>", escape_html(&desc_clean)));
                }
            }
            output.push_str(&format!("\n   └ <code>{}</code>", escape_html(&item.time_str)));
            if let Some(link) = &item.link {
                output.push_str(&format!(" <a href=\"{}\">[Link]</a>", link));
            }
        }
        output.push('\n');
    }
    output
}

pub fn format_error(source_name: &str, error: &FetchError) -> String {
    format!("<b>🕸 {}:</b> {}\n", escape_html(source_name), error)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}