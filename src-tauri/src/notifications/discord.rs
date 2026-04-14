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

        self.client
            .post(&self.webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await?;

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
