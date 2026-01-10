//! Static source configuration with zero-allocation design.
//! All strings are &'static str to avoid lifetime complexity.

use std::fmt;

/// Source type discriminator for hybrid fetching engine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// Standard RSS/XML feed
    Rss,
    /// Telegram web mirror (t.me/s/...)
    TelegramHtml,
}

/// News category groupings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Global,
    War,
    Market,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Global => write!(f, "🌍 Global"),
            Category::War => write!(f, "⚔️ War"),
            Category::Market => write!(f, "📈 Market"),
        }
    }
}

/// News source definition with static lifetime
#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub name: &'static str,
    pub url: &'static str,
    pub source_type: SourceType,
    pub category: Category,
}

impl Source {
    const fn new(
        name: &'static str,
        url: &'static str,
        source_type: SourceType,
        category: Category,
    ) -> Self {
        Self { name, url, source_type, category }
    }
}

/// Static source registry - compile-time constant, zero heap allocation
pub static SOURCES: &[Source] = &[
    // ═══════════════════════════════════════════════════════════════════
    // GLOBAL NEWS
    // ═══════════════════════════════════════════════════════════════════
    Source::new(
        "Reuters",
        "https://feeds.reuters.com/reuters/topNews",
        SourceType::Rss,
        Category::Global,
    ),
    Source::new(
        "Kommersant",
        "https://www.kommersant.ru/RSS/news.xml",
        SourceType::Rss,
        Category::Global,
    ),
    Source::new(
        "AlJazeera",
        "https://www.aljazeera.com/xml/rss/all.xml",
        SourceType::Rss,
        Category::Global,
    ),

    // ═══════════════════════════════════════════════════════════════════
    // WAR / GEOPOLITICS
    // ═══════════════════════════════════════════════════════════════════
    Source::new(
        "DeepState",
        "https://t.me/s/DeepStateUA",
        SourceType::TelegramHtml,
        Category::War,
    ),
    Source::new(
        "TASS",
        "https://tass.com/rss/v2.xml",
        SourceType::Rss,
        Category::War,
    ),
    Source::new(
        "Monitor",
        "https://t.me/s/ukraine_monitor",
        SourceType::TelegramHtml,
        Category::War,
    ),

    // ═══════════════════════════════════════════════════════════════════
    // MARKET / FINANCE
    // ═══════════════════════════════════════════════════════════════════
    Source::new(
        "Bloomberg",
        "https://t.me/s/bbbreaking",
        SourceType::TelegramHtml,
        Category::Market,
    ),
    Source::new(
        "ProFinance",
        "https://www.profinance.ru/rss/news.xml",
        SourceType::Rss,
        Category::Market,
    ),
    Source::new(
        "TreeOfAlpha",
        "https://t.me/s/TreeNewsFeed",
        SourceType::TelegramHtml,
        Category::Market,
    ),
];

/// Lookup source by name (case-insensitive match)
#[inline]
pub fn find_source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

/// Get all sources in a category
#[inline]
pub fn sources_by_category(category: Category) -> impl Iterator<Item = &'static Source> {
    SOURCES.iter().filter(move |s| s.category == category)
}

/// HTTP headers for stealth mode
pub mod headers {
    pub const USER_AGENT: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    pub const ACCEPT_HTML: &str =
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";
    pub const ACCEPT_RSS: &str =
        "application/rss+xml,application/xml,text/xml;q=0.9,*/*;q=0.8";
    pub const ACCEPT_LANG: &str = "en-US,en;q=0.9,ru;q=0.8";
    pub const ACCEPT_ENCODING: &str = "gzip, deflate, br";
}

/// CSS selectors for Telegram HTML parsing
pub mod selectors {
    pub const TG_MESSAGE_TEXT: &str = ".tgme_widget_message_text";
    pub const TG_MESSAGE_DATE: &str = ".tgme_widget_message_date time";
}

/// Limits and thresholds
pub mod limits {
    pub const MAX_ITEMS_PER_SOURCE: usize = 5;
    pub const MAX_TEXT_LENGTH: usize = 280;
    pub const REQUEST_TIMEOUT_SECS: u64 = 15;
    pub const BASE_DELAY_MS: u64 = 500;
}