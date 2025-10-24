use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use regex::Regex;
use reqwest;
use serde_json;
use std::fs;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{ChatKind, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("SKCheckerPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["sk", "sk@KissShotChkBot", "msk", "msk@KissShotChkBot"])]
pub struct SKCheckerPlugin;
impl SKCheckerPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.sk_checker(bot, message, msg).await
    }
    fn extract_sk(&self, text: &str) -> Vec<String> {
        let pattern = Regex::new(r"sk_live_\S+").unwrap();
        pattern
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }
    fn mask_sk(&self, sk: &str, is_group: bool) -> String {
        if is_group && sk.len() > 12 {
            let start = &sk[..14];
            let end = &sk[sk.len() - 6..];
            let middle = "x".repeat(sk.len() - 20);
            format!("{}{}{}", start, middle, end)
        } else {
            sk.to_string()
        }
    }
    async fn check_single_sk(&self, sk: &str, display_sk: &str) -> String {
        let client = reqwest::Client::new();

        let pm_response = match client
            .post("https://api.stripe.com/v1/payment_methods")
            .header("Authorization", format!("Bearer {}", sk))
            .form(&[
                ("type", "card"),
                ("card[number]", "4403934238397462"),
                ("card[exp_month]", "12"),
                ("card[exp_year]", "2026"),
                ("card[cvc]", "582"),
            ])
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let _ = LOGGER.error(&format!("Error in payment method request: {}", e)).await;
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌",
                    display_sk
                );
            }
        };

        let balance_response = match client
            .get("https://api.stripe.com/v1/balance")
            .header("Authorization", format!("Bearer {}", sk))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let _ = LOGGER.error(&format!("Error in balance request: {}", e)).await;
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌",
                    display_sk
                );
            }
        };

        let blocked_bins = match client
            .get("https://api.stripe.com/v1/radar/value_lists")
            .header("Authorization", format!("Bearer {}", sk))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(data) => data
                    .get("data")
                    .and_then(|d| d.get(8))
                    .and_then(|item| item.get("list_items"))
                    .and_then(|items| items.get("total_count"))
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0),
                Err(_) => 0,
            },
            Err(_) => 0,
        };
        let pm_text = match pm_response.text().await {
            Ok(text) => text,
            Err(_) => "".to_string(),
        };
        let balance_data = match balance_response.json::<serde_json::Value>().await {
            Ok(data) => data,
            Err(_) => serde_json::Value::Null,
        };
        if pm_text.contains("pm") {
            let balance = balance_data
                .get("available")
                .and_then(|a| a.get(0))
                .and_then(|b| b.get("amount"))
                .and_then(|a| a.as_u64())
                .unwrap_or(0) as f64;
            let currency = balance_data
                .get("available")
                .and_then(|a| a.get(0))
                .and_then(|b| b.get("currency"))
                .and_then(|c| c.as_str())
                .unwrap_or("usd")
                .to_uppercase();
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> Live Key ✅\n\
                 <b>Balance:</b> {:.2}\n\
                 <b>Currency:</b> {}\n\
                 <b>Blocked Bins:</b> {}",
                display_sk,
                balance / 100.0,
                currency,
                blocked_bins
            )
        } else if pm_text.contains("rate_limit") {
            let balance = balance_data
                .get("available")
                .and_then(|a| a.get(0))
                .and_then(|b| b.get("amount"))
                .and_then(|a| a.as_u64())
                .unwrap_or(0) as f64;
            let currency = balance_data
                .get("available")
                .and_then(|a| a.get(0))
                .and_then(|b| b.get("currency"))
                .and_then(|c| c.as_str())
                .unwrap_or("usd")
                .to_uppercase();
            format!(
                "Secret Key: {}\n\
                 Status: RATE LIMITED KEY ⚠️\n\
                 Balance: {:.2}\n\
                 Currency: {}\n\
                 Blocked Bins: {}",
                display_sk,
                balance / 100.0,
                currency,
                blocked_bins
            )
        } else {
            let error_msg = match serde_json::from_str::<serde_json::Value>(&pm_text) {
                Ok(data) => data
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
                    .to_string(),
                Err(_) => "Unknown error".to_string(),
            };
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> {}",
                display_sk, error_msg
            )
        }
    }
    pub async fn sk_checker(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let first_name = message
            .from
            .as_ref()
            .map(|user| user.first_name.clone())
            .unwrap_or_else(|| "User".to_string());
        let sent_msg = bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !check_registration(user_id).await {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> You are not registered.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        if check_banned(user_id).await {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> You are banned from using this bot.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let mut text = String::new();

        if let Some(document) = message.document() {
            let _ = document;
            let _ = LOGGER.info("Document attachment detected but not processed yet").await;
        }

        if msg.len() > 20 {
            text = msg.to_string();
        } else if let Some(reply) = message.reply_to_message() {
            if let Some(reply_text) = reply.text() {
                if reply_text.len() > 20 {
                    text = reply_text.to_string();
                }
            }
        }

        if text.is_empty() {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> No file or text found!\n\
                 <b>Usage:</b> /sk sk_live_... or reply to a message with SKs\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let sks = self.extract_sk(&text);
        if sks.is_empty() {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> No SK found!\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let max_sk = 20;
        if sks.len() > max_sk {
            let reply = format!(
                "<b>Secret Key Checking Information ℹ️</b>\n\n\
                 <b>Reason:</b> You can only check {} Secret Keys at once.\n\
                 <b>Found:</b> {}\n\
                 <b>Timestamp:</b> {}",
                max_sk,
                sks.len(),
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let is_group = matches!(message.chat.kind, ChatKind::Public(_));
        let mut results = Vec::new();
        let mut checked_count = 0;
        for sk in &sks {
            if checked_count >= max_sk {
                break;
            }
            checked_count += 1;
            let display_sk = self.mask_sk(sk, is_group);

            let progress_msg = format!(
                "<b>Secret Key Checking...</b>\n\n\
                 <b>Progress:</b> {}/{}\n\n\
                 {}",
                checked_count,
                sks.len(),
                results.join("\n\n")
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, &progress_msg)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            let result = self.check_single_sk(sk, &display_sk).await;
            results.push(result);
        }

        let user = handle_database_error!(
            bot,
            message,
            safe_fetch_user(user_id).await,
            "Database Operation"
        );
        let role = user
            .as_ref()
            .map(|u| u.status.clone())
            .unwrap_or_else(|| "Free".to_string());
        let user_id_for_link = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
        let end_text = format!(
            "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
            <b>Timestamp:</b> {}",
            user_id_for_link,
            first_name,
            format!("{}{}", &role[..1].to_uppercase(), &role[1..].to_lowercase()),
            timestamp
        );

        let final_msg = format!(
            "<b>Secret Key Checking Successful ✅</b>\n\n\
             <b>Checked:</b> {}\n\
             <b>Total:</b> {}\n\n\
             {}\n\n\
             {}",
            checked_count,
            sks.len(),
            results.join("\n\n"),
            end_text
        );
        bot.edit_message_text(message.chat.id, sent_msg.id, final_msg)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }
}
