#![allow(dead_code)]

use async_trait::async_trait;
use serde::Deserialize;

use super::{ChannelType, NotificationMessage, NotificationProvider, SendResult};

#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
}

pub struct TelegramProvider {
    bot_token: String,
    client: reqwest::Client,
}

impl TelegramProvider {
    pub fn from_config(config: &serde_json::Value) -> Result<Self, String> {
        let telegram_config: TelegramConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Invalid Telegram config: {}", e))?;

        Ok(Self {
            bot_token: telegram_config.bot_token,
            client: reqwest::Client::new(),
        })
    }

    async fn send_message(&self, chat_id: &str, text: &str) -> SendResult {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let payload = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "parse_mode": "HTML"
        });

        match self.client.post(&url).json(&payload).send().await {
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
                        error: Some(format!("Telegram API error: {}", error_text)),
                    }
                }
            }
            Err(e) => SendResult {
                success: false,
                error: Some(format!("Failed to send Telegram message: {}", e)),
            },
        }
    }
}

#[async_trait]
impl NotificationProvider for TelegramProvider {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    async fn send(&self, message: &NotificationMessage) -> SendResult {
        let severity_emoji = match message.severity {
            super::NotificationSeverity::Info => "ℹ️",
            super::NotificationSeverity::Warning => "⚠️",
            super::NotificationSeverity::Critical => "🚨",
        };

        let text = format!(
            "{} <b>{}</b>\n\n{}",
            severity_emoji,
            html_escape(&message.title),
            html_escape(&message.body)
        );

        self.send_message(&message.recipient, &text).await
    }

    async fn test(&self, destination: &str) -> SendResult {
        let text = "✅ <b>Kubarr Test Notification</b>\n\nThis is a test notification from Kubarr.\n\nIf you received this message, your Telegram notifications are configured correctly!";
        self.send_message(destination, text).await
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
