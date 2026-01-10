//! Mathematical utilities and stealth timing functions.
//! Implements Fibonacci/Golden Ratio based delay patterns.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Golden Ratio (φ) - the divine proportion
const PHI: f64 = 1.618033988749895;

/// Inverse Golden Ratio (1/φ) = φ - 1
const PHI_INVERSE: f64 = 0.6180339887498949;

/// Minimum delay floor (prevents zero/negative delays)
const MIN_DELAY_MS: u64 = 100;

/// Maximum delay ceiling (prevents excessive waits)
const MAX_DELAY_MS: u64 = 5000;

/// Fibonacci-inspired delay with golden ratio noise.
///
/// Formula: `delay = base * (1 + random_noise * φ⁻¹)`
///
/// This creates organic, non-predictable timing that mimics human behavior
/// and helps avoid rate limiting / IP bans.
///
/// # Arguments
/// * `base_ms` - Base delay in milliseconds
///
/// # Properties
/// - Non-deterministic (random noise factor)
/// - Bounded (MIN_DELAY_MS..MAX_DELAY_MS)
/// - Golden ratio based for natural feel
#[inline]
pub async fn fibonacci_delay(base_ms: u64) {
    let delay = compute_golden_delay(base_ms);
    sleep(Duration::from_millis(delay)).await;
}

/// Compute delay without sleeping (for testing/logging)
#[inline]
pub fn compute_golden_delay(base_ms: u64) -> u64 {
    let mut rng = rand::thread_rng();

    // Noise factor in range [-1.0, 1.0]
    let noise: f64 = rng.gen_range(-1.0..=1.0);

    // Apply golden ratio modulation
    let multiplier = 1.0 + noise * PHI_INVERSE;
    let delay = (base_ms as f64) * multiplier;

    // Clamp to valid range
    (delay as u64).clamp(MIN_DELAY_MS, MAX_DELAY_MS)
}

/// Progressive delay using Fibonacci sequence approximation.
/// Delay increases exponentially with attempt number.
///
/// Formula: `delay = base * φ^attempt`
#[inline]
pub async fn progressive_delay(base_ms: u64, attempt: u32) {
    let multiplier = PHI.powi(attempt as i32);
    let delay = ((base_ms as f64) * multiplier) as u64;
    let clamped = delay.clamp(MIN_DELAY_MS, MAX_DELAY_MS * 2);
    sleep(Duration::from_millis(clamped)).await;
}

/// Truncate text to max length with ellipsis.
/// Handles UTF-8 safely by finding char boundaries.
#[inline]
pub fn truncate_text(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();

    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }

    // Find safe truncation point (char boundary)
    let mut end = max_len.saturating_sub(3);
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }

    format!("{}...", &trimmed[..end])
}

/// Clean and normalize text content
/// - Removes excessive whitespace
/// - Strips HTML artifacts
/// - Normalizes line breaks
pub fn clean_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
}

/// Format news item for Telegram display
#[inline]
pub fn format_news_item(index: usize, text: &str, max_len: usize) -> String {
    let cleaned = clean_text(text);
    let truncated = truncate_text(&cleaned, max_len);
    format!("{}. {}", index + 1, truncated)
}

/// Calculate jitter for retry backoff
#[inline]
pub fn jitter_ms(base: u64) -> u64 {
    let mut rng = rand::thread_rng();
    let jitter: f64 = rng.gen_range(0.5..1.5);
    ((base as f64) * jitter) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_golden_delay_bounded() {
        for _ in 0..1000 {
            let delay = compute_golden_delay(500);
            assert!(delay >= MIN_DELAY_MS);
            assert!(delay <= MAX_DELAY_MS);
        }
    }

    #[test]
    fn test_truncate_handles_utf8() {
        let russian = "Привет мир это тест очень длинного текста";
        let truncated = truncate_text(russian, 20);
        assert!(truncated.len() <= 23); // 20 + "..."
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn test_clean_text() {
        let dirty = "  Hello   &amp;  World  \n\n  Test  ";
        let clean = clean_text(dirty);
        assert_eq!(clean, "Hello & World Test");
    }
}