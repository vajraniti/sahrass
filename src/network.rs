//! Hybrid fetching engine with RSS, Telegram, and NewsData support.

use crate::consts::{headers, limits, selectors, Source, SourceType, Category};
use crate::utils::{clean_text, fibonacci_delay, truncate_text, is_junk};
use crate::translate::translate_text;
use reqwest::{Client};
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

        let mut items = match source.source_type {
            SourceType::TelegramHtml => self.fetch_telegram(source.url).await?,
            SourceType::Rss => self.fetch_rss(source.url).await?,
            SourceType::NewsData => self.fetch_newsdata(source.url).await?,
        };

        // Перевод (кроме RU источников)
        if source.language != "ru" {
            let mut tasks = Vec::new();
            for item in &items {
                let client = self.client.clone();
                let title = item.title.clone();
                let desc = item.description.clone();

                tasks.push(tokio::spawn(async move {
                    let t_title = translate_text(&client, &title, "ru").await.unwrap_or(title);
                    let t_desc = if let Some(d) = desc {
                        Some(translate_text(&client, &d, "ru").await.unwrap_or(d))
                    } else { None };
                    (t_title, t_desc)
                }));
            }

            let results = join_all(tasks).await;
            for (i, res) in results.into_iter().enumerate() {
                if let Ok((t, d)) = res {
                    if i < items.len() {
                        items[i].title = t;
                        items[i].description = d;
                    }
                }
            }
        }

        // Commodities Logic: Оставляем ТОЛЬКО ЦЕНУ
        if source.category == Category::Commodities {
            // Ищем паттерн: Число + Валюта (USD, $, RUB, ₽)
            let price_regex = Regex::new(r"(?i)(\d+[.,]?\d*\s*(\$|USD|RUB|руб|₽))").unwrap();

            let mut clean_items = Vec::new();
            for mut item in items {
                let content = format!("{} {}", item.title, item.description.as_deref().unwrap_or(""));

                if let Some(caps) = price_regex.find(&content) {
                    // Если нашли цену — перезаписываем заголовок на неё
                    item.title = caps.as_str().to_string();
                    item.description = None; // Убираем лишний текст
                    clean_items.push(item);
                }
            }
            items = clean_items;
        }

        if items.is_empty() { return Err(FetchError::Empty); }
        Ok(items)
    }

    async fn fetch_newsdata(&self, query: &str) -> Result<Vec<NewsItem>, FetchError> {
        let api_key = std::env::var("NEWSDATA_KEY").map_err(|_| FetchError::NoKey)?;
        // Используем endpoint latest и категорию business для отсева мусора
        let url = format!("https://newsdata.io/api/1/latest?apikey={}&q={}&category=business&language=en", api_key, query);

        let res = self.client.get(&url).send().await?;
        let data: serde_json::Value = res.json().await?;

        let mut items = Vec::new();
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            for entry in results.iter().take(limits::MAX_ITEMS_PER_SOURCE) {
                let title = entry["title"].as_str().unwrap_or("No Title").to_string();
                // NewsData: описание часто дублирует заголовок, берем если оно длиннее
                let desc = entry["description"].as_str().map(|s| clean_text(s));
                let link = entry["link"].as_str().map(|s| s.to_string());
                let date = entry["pubDate"].as_str().unwrap_or("--:--").to_string();

                if !is_junk(&title) {
                    items.push(NewsItem::new(title, date).with_desc(desc).with_link(link));
                }
            }
        }
        if items.is_empty() { return Err(FetchError::Empty); }
        Ok(items)
    }

    async fn fetch_rss(&self, url: &str) -> Result<Vec<NewsItem>, FetchError> {
        let res = self.client.get(url).send().await?;
        let bytes = res.bytes().await?;
        let feed = feed_rs::parser::parse(&bytes[..]).map_err(|_| FetchError::Empty)?;

        let items = feed.entries.into_iter()
            .take(limits::MAX_ITEMS_PER_SOURCE)
            .filter_map(|e| {
                let title = e.title.map(|t| t.content).unwrap_or_default();
                if is_junk(&title) { return None; }
                let desc = e.summary.map(|s| clean_text(&s.content))
                    .or_else(|| e.content.map(|c| clean_text(&c.body.unwrap_or_default())));
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
}

// ═══════════════════════════════════════════════════════════════════
// HTML FORMATTING
// ═══════════════════════════════════════════════════════════════════

pub fn format_results(source_name: &str, items: &[NewsItem]) -> String {
    // Используем HTML теги
    let mut output = format!("<b>🏴 {}</b>\n", escape_html(source_name));
    for item in items {
        let title_clean = truncate_text(&item.title, 150);

        // Для цен выделяем жирным
        output.push_str(&format!("\n▪️ <b>{}</b>", escape_html(&title_clean)));

        // Описание показываем, только если это не цена (т.к. для цены мы description обнулили)
        if let Some(ref d) = item.description {
            let desc_clean = truncate_text(d, 200);
            if !desc_clean.is_empty() && desc_clean != title_clean {
                output.push_str(&format!("\n   <i>{}</i>", escape_html(&desc_clean)));
            }
        }
        output.push_str(&format!("\n   └ <code>{}</code>", escape_html(&item.time_str)));

        // Компактная ссылка
        if let Some(link) = &item.link {
            output.push_str(&format!(" <a href=\"{}\">[Link]</a>", link));
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