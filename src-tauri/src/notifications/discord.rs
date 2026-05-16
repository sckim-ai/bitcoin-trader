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
        self.post_payload(&payload).await
    }

    /// Send a fully-formed Discord webhook payload (e.g. `{embeds: [...]}` ).
    /// Allows callers to assemble rich embeds without re-implementing the
    /// transport. Embeds give us titled/coloured/field-structured messages
    /// that plain `content` can't express — direct port of the legacy
    /// DiscordNotificationService formatting.
    pub async fn send_payload(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.post_payload(payload).await
    }

    async fn post_payload(
        &self,
        payload: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self.client
            .post(&self.webhook_url)
            .header("Content-Type", "application/json")
            .json(payload)
            .send()
            .await?;
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
