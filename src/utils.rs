use std::time::Duration;
use tokio::time::sleep;

pub async fn fibonacci_delay(base_ms: u64) {
    sleep(Duration::from_millis(base_ms)).await;
}

pub fn clean_text(text: &str) -> String {
    let no_html = text.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace("<b>", "*").replace("</b>", "*") // Оставляем жирный
        .replace("<strong>", "*").replace("</strong>", "*");

    no_html.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Фильтр мусора: Шоу, Спорт, Криминал (если это не новости войны)
pub fn is_junk(text: &str) -> bool {
    let t = text.trim().to_lowercase();

    // 1. Системные сообщения Telegram
    if t.contains("channel created") || t.contains("account created") { return true; }

    // 2. Развлекательный мусор (фильтруем шоу, сериалы, спорт)
    let junk_keywords = [
        "football", "soccer", "sport", "match", "premier league",
        "netflix", "series", "season", "episode", "show", "star", "celebrity",
        "футбол", "спорт", "сериал", "шоу", "звезда", "эпизод"
    ];

    if junk_keywords.iter().any(|&k| t.contains(k)) {
        return true;
    }

    // 3. Ссылки без текста
    if (t.starts_with("http") && !t.contains(' ')) || (t.contains("youtu.be") && t.len() < 60) {
        return true;
    }

    false
}

pub fn truncate_text(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars { return s.to_string(); }
    s.chars().take(max_chars).collect::<String>() + "..."
}