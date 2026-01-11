//! LOGOS - High-performance Telegram News Aggregator

mod consts;
mod logic;
mod network;
mod utils;
mod translate;

use crate::logic::{build_help_message, build_summary, fetch_target, routes, Target};
use crate::network::NewsEngine;
use std::sync::Arc;
use std::env;
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::command::BotCommands;

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
    #[command(description = "✟ Ancient Dust")]
    Commodities,

    // Individual sources
    #[command(description = "Reuters NewsData")]
    Reuters,
    #[command(description = "Yahoo Politics")]
    Yahoo,
    #[command(description = "Gold price")]
    Gold,
    #[command(description = "Oil price")]
    Oil,
}

impl Command {
    fn to_target(&self) -> Option<Target> {
        let cmd_str = match self {
            Command::Start | Command::Help => return None,
            Command::Global => "global",
            Command::War => "war",
            Command::Market => "market",
            Command::Commodities => "commodities",
            Command::Reuters => "reuters",
            Command::Yahoo => "yahoopolitics", // Updated mapping
            Command::Gold => "gold",
            Command::Oil => "oil",
        };
        routes::resolve_command(cmd_str)
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("═══════════════════════════════════════════");
    log::info!("  LOGOS SYSTEM ONLINE. FILTERING AETHER...");
    log::info!("═══════════════════════════════════════════");

    let token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN not found!");
    let bot = Bot::new(token);
    let engine = NewsEngine::new();

    Command::repl(bot, move |bot: Bot, msg: Message, cmd: Command| {
        let engine = Arc::clone(&engine);
        async move {
            handle_command(bot, msg, cmd, engine).await
        }
    }).await;
}

async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    engine: Arc<NewsEngine>,
) -> ResponseResult<()> {
    let chat_id = msg.chat.id;

    if matches!(cmd, Command::Start | Command::Help) {
        bot.send_message(chat_id, build_help_message())
            .parse_mode(ParseMode::Markdown)
            .await?;
        return Ok(());
    }

    let target = match cmd.to_target() {
        Some(t) => t,
        None => return Ok(()),
    };

    let loading_msg = bot
        .send_message(chat_id, format!("⏳ Fetching {}...", target.display_name()))
        .await?;

    let result = fetch_target(engine, target).await;

    let mut response = format!("<b>{}</b>\n\n{}", result.header, result.content);
    response.push_str(&build_summary(&result));

    let _ = bot.delete_message(chat_id, loading_msg.id).await;

    if response.len() > 4000 {
        for chunk in split_message(&response, 4000) {
            bot.send_message(chat_id, chunk)
                .parse_mode(ParseMode::Html)
                .disable_web_page_preview(true)
                .await?;
        }
    } else {
        bot.send_message(chat_id, response)
            .parse_mode(ParseMode::Html)
            .disable_web_page_preview(true)
            .await?;
    }

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = start + max_len;
        if end >= text.len() {
            chunks.push(&text[start..]);
            break;
        }
        while !text.is_char_boundary(end) { end -= 1; }
        let search_range = &text[start..end];
        if let Some(last_newline) = search_range.rfind('\n') {
            let split_idx = start + last_newline + 1;
            if split_idx > start { end = split_idx; }
        }
        chunks.push(&text[start..end]);
        start = end;
    }
    chunks
}