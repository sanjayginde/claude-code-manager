pub async fn generate_title(first_message: &str) -> Option<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;

    let truncated: String = first_message.chars().take(300).collect();
    let prompt = format!(
        "Give a concise 4-6 word title for a conversation starting with: \"{}\". Reply with only the title, no quotes or punctuation.",
        truncated
    );

    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 20,
        "messages": [{ "role": "user", "content": prompt }]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .ok()?;

    let json: serde_json::Value = resp.json().await.ok()?;
    let title = json["content"][0]["text"].as_str()?.trim().to_string();

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}
