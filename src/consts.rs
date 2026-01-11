//! Static source configuration with zero-allocation design.

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
    Commodities,
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Category::Global => write!(f, "🖤 Global"),
            Category::War => write!(f, "🤍 War"),
            Category::Market => write!(f, "🏴 Market"),
            Category::Commodities => write!(f, "💀 Commodities"), // Replaced Cup with Skull
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

/// Static source registry
pub static SOURCES: &[Source] = &[
    // ═══════════════════════════════════════════════════════════════════
    // GLOBAL NEWS (🖤)
    // ═══════════════════════════════════════════════════════════════════
    // Switched RBC -> Reuters Agency Feed (Cleaner, reliable)
    Source::new("Reuters", "https://www.reutersagency.com/feed/?best-topics=political-general&post_type=best", SourceType::Rss, Category::Global),
    Source::new("Kommersant", "https://t.me/s/kommersant", SourceType::TelegramHtml, Category::Global),
    Source::new("AlJazeera", "https://www.aljazeera.com/xml/rss/all.xml", SourceType::Rss, Category::Global),

    // ═══════════════════════════════════════════════════════════════════
    // WAR / GEOPOLITICS (🤍)
    // ═══════════════════════════════════════════════════════════════════
    Source::new("DeepState", "https://t.me/s/DeepStateUA", SourceType::TelegramHtml, Category::War),
    Source::new("TASS", "https://t.me/s/tass_agency", SourceType::TelegramHtml, Category::War),
    // Switched UkraineNow -> Liveuamap
    Source::new("Liveuamap", "https://t.me/s/liveuamap", SourceType::TelegramHtml, Category::War),

    // ═══════════════════════════════════════════════════════════════════
    // MARKET / FINANCE (🏴)
    // ═══════════════════════════════════════════════════════════════════
    Source::new("Bloomberg", "https://t.me/s/bbbreaking", SourceType::TelegramHtml, Category::Market),
    Source::new("MarketTwits", "https://t.me/s/markettwits", SourceType::TelegramHtml, Category::Market),
    Source::new("TreeOfAlpha", "https://t.me/s/TreeNewsFeed", SourceType::TelegramHtml, Category::Market),

    // ═══════════════════════════════════════════════════════════════════
    // COMMODITIES / DEAD ASSETS (💀)
    // ═══════════════════════════════════════════════════════════════════
    // Using Google News RSS specific queries to get the latest "Rate" news
    Source::new("Gold", "https://news.google.com/rss/search?q=Gold+Price+USD&hl=en-US&gl=US&ceid=US:en", SourceType::Rss, Category::Commodities),
    Source::new("Oil", "https://news.google.com/rss/search?q=Brent+Crude+Oil+Price&hl=en-US&gl=US&ceid=US:en", SourceType::Rss, Category::Commodities),
];

#[inline]
pub fn find_source(name: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.name.eq_ignore_ascii_case(name))
}

#[inline]
pub fn sources_by_category(category: Category) -> impl Iterator<Item = &'static Source> {
    SOURCES.iter().filter(move |s| s.category == category)
}

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