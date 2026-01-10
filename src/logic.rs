//! Business logic layer - Target resolution and aggregation

use crate::consts::{find_source, limits, sources_by_category, Category, Source};
use crate::network::{format_error, format_results, NewsEngine};
use std::sync::Arc;

/// Fetch target - either a category or specific source
#[derive(Debug, Clone)]
pub enum Target {
    /// Fetch all sources in a category
    Category(Category),
    /// Fetch a specific source by name
    Source(&'static str),
}

impl Target {
    /// Resolve target to list of sources
    pub fn resolve(&self) -> Vec<&'static Source> {
        match self {
            Target::Category(cat) => sources_by_category(*cat).collect(),
            Target::Source(name) => {
                find_source(name).into_iter().collect()
            }
        }
    }

    /// Get display name for this target
    pub fn display_name(&self) -> String {
        match self {
            Target::Category(cat) => cat.to_string(),
            Target::Source(name) => format!("📰 {}", name),
        }
    }
}

/// Aggregated fetch result
pub struct AggregatedNews {
    pub header: String,
    pub content: String,
    pub success_count: usize,
    pub error_count: usize,
}

/// Fetch news for a target with aggregation
pub async fn fetch_target(engine: Arc<NewsEngine>, target: Target) -> AggregatedNews {
    let sources = target.resolve();
    let header = format!("{} News Feed", target.display_name());

    if sources.is_empty() {
        return AggregatedNews {
            header,
            content: "❌ No sources found".to_string(),
            success_count: 0,
            error_count: 1,
        };
    }

    let mut content = String::with_capacity(4096);
    let mut success_count = 0;
    let mut error_count = 0;

    for source in sources {
        match engine.fetch_with_retry(source, 2).await {
            Ok(items) => {
                content.push_str(&format_results(source.name, &items));
                content.push('\n');
                success_count += 1;
            }
            Err(e) => {
                log::error!("Failed to fetch {}: {}", source.name, e);
                content.push_str(&format_error(source.name, &e));
                error_count += 1;
            }
        }
    }

    AggregatedNews {
        header,
        content,
        success_count,
        error_count,
    }
}

/// Build help message
pub fn build_help_message() -> &'static str {
    r#"📰 *LOGOS News Aggregator*

*Category Commands:*
/global — 🌍 Global news (Reuters, Kommersant, AlJazeera)
/war — ⚔️ War updates (DeepState, TASS, Monitor)
/market — 📈 Market news (Bloomberg, ProFinance, TreeOfAlpha)

*Individual Source Commands:*
🌍 `/reuters` `/kommersant` `/aljazeera`
⚔️ `/deepstate` `/tass` `/monitor`
📈 `/bloomberg` `/profinance` `/tree`

*Other:*
/start, /help — Show this message

_Powered by Rust 🦀_"#
}

/// Build summary line
pub fn build_summary(result: &AggregatedNews) -> String {
    format!(
        "\n───────────────────\n✅ {} sources | ❌ {} failed",
        result.success_count, result.error_count
    )
}

/// Command routing table
pub mod routes {
    use super::*;
    use crate::consts::Category;

    /// Map command string to target
    pub fn resolve_command(cmd: &str) -> Option<Target> {
        match cmd.to_lowercase().as_str() {
            // Categories
            "global" => Some(Target::Category(Category::Global)),
            "war" => Some(Target::Category(Category::War)),
            "market" => Some(Target::Category(Category::Market)),

            // Individual sources - Global
            "reuters" => Some(Target::Source("Reuters")),
            "kommersant" => Some(Target::Source("Kommersant")),
            "aljazeera" => Some(Target::Source("AlJazeera")),

            // Individual sources - War
            "deepstate" => Some(Target::Source("DeepState")),
            "tass" => Some(Target::Source("TASS")),
            "monitor" => Some(Target::Source("Monitor")),

            // Individual sources - Market
            "bloomberg" => Some(Target::Source("Bloomberg")),
            "profinance" => Some(Target::Source("ProFinance")),
            "tree" => Some(Target::Source("TreeOfAlpha")),

            _ => None,
        }
    }
}