//! LOGOS - High-performance Telegram News Aggregator
//!
//! Architecture: Modular async design with Arc-based shared state
//! Runtime: Tokio multi-threaded
//! Bot Framework: Teloxide

mod consts;
mod logic;
mod network;
mod utils;

use crate::logic::{build_help_message, build_summary, fetch_target, routes, Target};
use crate::network::NewsEngine;
use std::sync::Arc;
use std::env;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;

/// Bot commands enumeration with automatic parsing
#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Show help message")]
    Start,
    #[command(description = "Show help message")]
    Help,

    // Category commands
    #[command(description = "🖤 Global news")]
    Global,
    #[command(description = "🤍 War updates")]
    War,
    #[command(description = "🏴 Market news")]
    Market,

    // Individual source commands - Global
    #[command(description = "RBC feed")]
    Rbc,
    #[command(description = "Kommersant feed")]
    Kommersant,
    #[command(description = "AlJazeera feed")]
    Aljazeera,

    // Individual source commands - War
    #[command(description = "DeepState updates")]
    Deepstate,
    #[command(description = "TASS feed")]
    Tass,
    #[command(description = "Monitor updates")]
    Monitor,

    // Individual source commands - Market
    #[command(description = "Bloomberg breaking")]
    Bloomberg,
    #[command(description = "MarketTwits feed")]
    Markettwits,
    #[command(description = "Tree of Alpha feed")]
    Tree,
}

impl Command {
    /// Convert command to fetch target
    fn to_target(&self) -> Option<Target> {
        let cmd_str = match self {
            Command::Start | Command::Help => return None,
            Command::Global => "global",
            Command::War => "war",
            Command::Market => "market",
            Command::Rbc => "rbc",
            Command::Kommersant => "kommersant",
            Command::Aljazeera => "aljazeera",
            Command::Deepstate => "deepstate",
            Command::Tass => "tass",
            Command::Monitor => "monitor",
            Command::Bloomberg => "bloomberg",
            Command::Markettwits => "markettwits",
            Command::Tree => "tree",
        };
        routes::resolve_command(cmd_str)
    }
}

/// Application entry point
#[tokio::main]
async fn main() {
    // Load .env (TELOXIDE_TOKEN=...) from project root if present
    dotenvy::dotenv().ok();

    // Initialize logging
    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .filter_module("teloxide", log::LevelFilter::Warn)
        .filter_module("reqwest", log::LevelFilter::Warn)
        .init();

    log::info!("═══════════════════════════════════════════");
    log::info!("  LOGOS News Aggregator v0.1.0");
    log::info!("  Initializing...");
    log::info!("═══════════════════════════════════════════");

    // Check for bot token
    let token = match env::var("TELOXIDE_TOKEN") {
        Ok(t) => t,
        Err(_) => {
            log::error!("Token not found!");
            std::process::exit(1);
        }
    };

    // Initialize bot with token
    let bot = Bot::new(token);

    // Initialize shared news engine (Arc for cheap cloning)
    let engine = NewsEngine::new();

    log::info!("Bot initialized, starting command handler...");

    // Command handler with move closure for ownership
    Command::repl(bot, move |bot: Bot, msg: Message, cmd: Command| {
        // Clone Arc (cheap reference count increment)
        let engine = Arc::clone(&engine);

        async move {
            handle_command(bot, msg, cmd, engine).await
        }
    })
        .await;
}

/// Handle incoming command
async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    engine: Arc<NewsEngine>,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;

    // Handle help commands
    if matches!(cmd, Command::Start | Command::Help) {
        bot.send_message(chat_id, build_help_message())
            .parse_mode(ParseMode::Markdown)
            .await?;
        return Ok(());
    }

    // Resolve target
    let target = match cmd.to_target() {
        Some(t) => t,
        None => {
            bot.send_message(chat_id, "🕷 Unknown command")
                .await?;
            return Ok(());
        }
    };

    // Send loading indicator
    let loading_msg = bot
        .send_message(chat_id, format!("⏳ Fetching {}...", target.display_name()))
        .await?;

    // Fetch news
    let result = fetch_target(engine, target).await;

    // Build response
    let mut response = format!("*{}*\n\n{}", result.header, result.content);
    response.push_str(&build_summary(&result));

    // Delete loading message
    let _ = bot.delete_message(chat_id, loading_msg.id).await;

    // Send results (split if too long)
    if response.len() > 4000 {
        // Split into chunks for long messages
        for chunk in split_message(&response, 4000) {
            bot.send_message(chat_id, chunk)
                .parse_mode(ParseMode::Markdown)
                .disable_web_page_preview(true)
                .await?;
        }
    } else {
        bot.send_message(chat_id, response)
            .parse_mode(ParseMode::Markdown)
            .disable_web_page_preview(true)
            .await?;
    }

    Ok(())
}

/// Split message into chunks at line boundaries
fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + max_len).min(text.len());

        // Find last newline before end
        let chunk_end = if end == text.len() {
            end
        } else {
            text[start..end]
                .rfind('\n')
                .map(|pos| start + pos + 1)
                .unwrap_or(end)
        };

        chunks.push(&text[start..chunk_end]);
        start = chunk_end;
    }

    chunks
}