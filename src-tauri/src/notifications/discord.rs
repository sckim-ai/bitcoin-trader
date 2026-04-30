use reqwest::Client;
use serde_json::json;

pub struct DiscordClient {
    webhook_url: String,
    client: Client,
}

impl DiscordClient {
    pub fn new(webhook_url: String) -> Self {
        Self {
            webhook_url,
            client: Client::new(),
        }
    }

    pub async fn send(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let payload = json!({ "content": message });

        let resp = self.client
            .post(&self.webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

        // Discord webhook returns 204 No Content on success; 200~299 covers
        // any future variant. Anything else is an error we want to surface
        // (404 = bad URL, 401 = revoked, 429 = rate-limit, etc.).
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Discord webhook returned {}: {}", status, body).into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_client_new() {
        let client = DiscordClient::new("https://discord.com/api/webhooks/test".to_string());
        assert_eq!(client.webhook_url, "https://discord.com/api/webhooks/test");
    }
}
