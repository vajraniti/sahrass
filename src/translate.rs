use reqwest::Client;
use serde::Deserialize;
use std::error::Error;

/// Structure to parse Google Translate response
/// The response is a messy JSON array: [[["translated_text", "original", ...]], ...]
#[derive(Deserialize, Debug)]
struct TranslationResponse(Vec<Vec<Option<String>>>);
// We use a simplified structure or just raw parsing because the structure is dynamic.
// Actually, it's easier to parse as serde_json::Value for safety.

pub async fn translate_text(client: &Client, text: &str, target_lang: &str) -> Result<String, Box<dyn Error>> {
    // URL encoding is handled by reqwest query params
    let url = "https://translate.googleapis.com/translate_a/single";

    let params = [
        ("client", "gtx"),
        ("sl", "auto"),      // Source language: auto-detect
        ("tl", target_lang), // Target language
        ("dt", "t"),         // Return translation
        ("q", text),
    ];

    let response = client.get(url)
        .query(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(format!("Translation failed: {}", response.status()).into());
    }

    let raw_json: serde_json::Value = response.json().await?;

    // Extract text from the deep nested array structure: [[[ "Translated", ... ]]]
    let mut translated_text = String::new();

    if let Some(sentences) = raw_json.get(0).and_then(|v| v.as_array()) {
        for sentence in sentences {
            if let Some(s_arr) = sentence.as_array() {
                if let Some(text_val) = s_arr.get(0).and_then(|v| v.as_str()) {
                    translated_text.push_str(text_val);
                }
            }
        }
    }

    if translated_text.is_empty() {
        return Ok(text.to_string()); // Fallback to original
    }

    Ok(translated_text)
}