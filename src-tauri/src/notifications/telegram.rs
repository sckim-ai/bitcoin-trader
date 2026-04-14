use reqwest::Client;
use serde_json::json;

pub struct TelegramClient {
    bot_token: String,
    chat_id: String,
    client: Client,
}

impl TelegramClient {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id,
            client: Client::new(),
        }
    }

    pub async fn send(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        let payload = json!({
            "chat_id": self.chat_id,
            "text": message,
            "parse_mode": "HTML"
        });

        self.client
            .post(&url)
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
    fn test_telegram_client_new() {
        let client = TelegramClient::new("bot_token_123".to_string(), "chat_456".to_string());
        assert_eq!(client.bot_token, "bot_token_123");
        assert_eq!(client.chat_id, "chat_456");
    }
}
