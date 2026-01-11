use std::time::Duration;
use tokio::time::sleep;

/// Fibonacci delay sequence for stealth
pub async fn fibonacci_delay(base_ms: u64) {
    // Simple fixed delay for now to keep it responsive but safe
    sleep(Duration::from_millis(base_ms)).await;
}

/// Progressive delay for retries
pub async fn progressive_delay(base_ms: u64, attempt: u32) {
    sleep(Duration::from_millis(base_ms * 2u64.pow(attempt))).await;
}

/// Clean text from HTML tags and excess whitespace
pub fn clean_text(text: &str) -> String {
    let no_html = text.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ");

    no_html.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if the text is useless system message or just a link
/// Check if the text is useless system message, link, or unwanted topic (sports)
pub fn is_junk(text: &str) -> bool {
    let t = text.trim();
    // Приводим к нижнему регистру для поиска ключевиков
    let lower = t.to_lowercase();

    // 1. System messages
    if t.eq_ignore_ascii_case("Channel created")
        || t.eq_ignore_ascii_case("Account created")
        || t.contains("Channel photo updated") {
        return true;
    }

    // 2. Content Filters (Sports & Entertainment Noise)
    // Добавляй сюда любые слова, которые тебе не нужны.
    let noise_keywords = [
        "football", "soccer", "fifa", "uefa", "sport",
        "champions league", "premier league", "cricket", "tennis",
        "футбол", "спорт", "матч", "чемпионат"// на случай русских источников
    ];

    if noise_keywords.iter().any(|&k| lower.contains(k)) {
        return true;
    }

    // 3. Bare links (start with http, no spaces, short-ish)
    // or specifically YouTube links without context
    if (t.starts_with("http") && !t.contains(' '))
        || (t.contains("youtu.be") && t.len() < 60) {
        return true;
    }

    false
}

/// Truncate text respecting UTF-8 boundaries
pub fn truncate_text(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut s: String = s.chars().take(max_chars).collect();
    s.push_str("...");
    s
}