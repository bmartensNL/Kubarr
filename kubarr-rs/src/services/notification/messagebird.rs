use async_trait::async_trait;
use serde::Deserialize;

use super::{ChannelType, NotificationMessage, NotificationProvider, SendResult};

#[derive(Debug, Deserialize)]
pub struct MessageBirdConfig {
    pub api_key: String,
    pub originator: Option<String>,
}

pub struct MessageBirdProvider {
    api_key: String,
    originator: String,
    client: reqwest::Client,
}

impl MessageBirdProvider {
    pub fn from_config(config: &serde_json::Value) -> Result<Self, String> {
        let mb_config: MessageBirdConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Invalid MessageBird config: {}", e))?;

        Ok(Self {
            api_key: mb_config.api_key,
            originator: mb_config.originator.unwrap_or_else(|| "Kubarr".to_string()),
            client: reqwest::Client::new(),
        })
    }

    async fn send_sms(&self, recipient: &str, body: &str) -> SendResult {
        let url = "https://rest.messagebird.com/messages";

        let payload = serde_json::json!({
            "originator": self.originator,
            "recipients": [recipient],
            "body": body
        });

        match self
            .client
            .post(url)
            .header("Authorization", format!("AccessKey {}", self.api_key))
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    SendResult {
                        success: true,
                        error: None,
                    }
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    SendResult {
                        success: false,
                        error: Some(format!("MessageBird API error: {}", error_text)),
                    }
                }
            }
            Err(e) => SendResult {
                success: false,
                error: Some(format!("Failed to send SMS: {}", e)),
            },
        }
    }
}

#[async_trait]
impl NotificationProvider for MessageBirdProvider {
    fn channel_type(&self) -> ChannelType {
        ChannelType::MessageBird
    }

    async fn send(&self, message: &NotificationMessage) -> SendResult {
        // SMS has character limits, so keep it concise
        let severity_prefix = match message.severity {
            super::NotificationSeverity::Info => "[INFO]",
            super::NotificationSeverity::Warning => "[WARN]",
            super::NotificationSeverity::Critical => "[ALERT]",
        };

        let text = format!(
            "{} {}: {}",
            severity_prefix,
            message.title,
            truncate(&message.body, 120)
        );

        self.send_sms(&message.recipient, &text).await
    }

    async fn test(&self, destination: &str) -> SendResult {
        self.send_sms(
            destination,
            "Kubarr Test: SMS notifications are configured correctly!",
        )
        .await
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
