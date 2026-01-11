//! Static source configuration.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType { Rss, TelegramHtml, NewsData }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category { Global, War, Market, Commodities }

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Global => write!(f, "🖤 Global"),
            Category::War => write!(f, "🤍 War"),
            Category::Market => write!(f, "🏴 Market"),
            Category::Commodities => write!(f, "✟ ANCIENT DUST"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub name: &'static str,
    pub url: &'static str,
    pub source_type: SourceType,
    pub category: Category,
    pub language: &'static str,
}

impl Source {
    pub const fn new(
        name: &'static str,
        url: &'static str,
        source_type: SourceType,
        category: Category,
        language: &'static str,
    ) -> Self {
        Self { name, url, source_type, category, language }
    }
}

pub static SOURCES: &[Source] = &[
    // Global - Reuters (NewsData)
    Source::new("Reuters", "reuters", SourceType::NewsData, Category::Global, "en"),
    Source::new("Kommersant", "https://t.me/s/kommersant", SourceType::TelegramHtml, Category::Global, "ru"),
    Source::new("AlJazeera", "https://www.aljazeera.com/xml/rss/all.xml", SourceType::Rss, Category::Global, "en"),

    // War
    Source::new("DeepState", "https://t.me/s/DeepStateUA", SourceType::TelegramHtml, Category::War, "ru"),
    Source::new("TASS", "https://t.me/s/tass_agency", SourceType::TelegramHtml, Category::War, "ru"),
    Source::new("Liveuamap", "https://t.me/s/liveuamap", SourceType::TelegramHtml, Category::War, "en"),

    // Market
    Source::new("Bloomberg", "https://t.me/s/bbbreaking", SourceType::TelegramHtml, Category::Market, "en"),
    Source::new("MarketTwits", "https://t.me/s/markettwits", SourceType::TelegramHtml, Category::Market, "ru"),
    Source::new("Tree", "https://t.me/s/TreeNewsFeed", SourceType::TelegramHtml, Category::Market, "en"),

    // Commodities (Цены - пробуем вытащить цифры через RSS)
    // Золото: Ищем точное совпадение цены
    Source::new("Gold", "https://news.google.com/rss/search?q=XAU+USD+price+when:1d&hl=en-US&gl=US&ceid=US:en", SourceType::Rss, Category::Commodities, "en"),
    // Лукойл/Роснефть: Ищем цену акций
    Source::new("Oil", "https://news.google.com/rss/search?q=Lukoil+Rosneft+share+price+when:1d&hl=en-US&gl=US&ceid=US:en", SourceType::Rss, Category::Commodities, "en"),
];

// ... (find_source, headers, limits остаются как были, скопируй если нужно, но они есть в старом коде)
// Обязательно добавь эти функции если заменяешь файл целиком:
#[inline]
pub fn find_source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

#[inline]
pub fn sources_by_category(category: Category) -> impl Iterator<Item = &'static Source> {
    SOURCES.iter().filter(move |s| s.category == category)
}

pub mod headers {
    pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
}

pub mod selectors {
    pub const TG_MESSAGE_WRAP: &str = ".tgme_widget_message_wrap";
    pub const TG_MESSAGE_TEXT: &str = ".tgme_widget_message_text";
    pub const TG_MESSAGE_DATE: &str = ".tgme_widget_message_date";
}

pub mod limits {
    pub const MAX_ITEMS_PER_SOURCE: usize = 5;
    pub const MAX_TEXT_LENGTH: usize = 280;
    pub const REQUEST_TIMEOUT_SECS: u64 = 15;
    pub const BASE_DELAY_MS: u64 = 500;
}