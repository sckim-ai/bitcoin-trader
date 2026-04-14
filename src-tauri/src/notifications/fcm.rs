use reqwest::Client;
use serde_json::json;

pub struct FcmClient {
    server_key: String,
    client: Client,
}

impl FcmClient {
    pub fn new(server_key: String) -> Self {
        Self {
            server_key,
            client: Client::new(),
        }
    }

    pub async fn send(
        &self,
        device_token: &str,
        title: &str,
        body: &str,
        priority: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let payload = json!({
            "to": device_token,
            "notification": {
                "title": title,
                "body": body
            },
            "priority": priority
        });

        self.client
            .post("https://fcm.googleapis.com/fcm/send")
            .header("Authorization", format!("key={}", self.server_key))
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
    fn test_fcm_client_new() {
        let client = FcmClient::new("test_key".to_string());
        assert_eq!(client.server_key, "test_key");
    }
}
