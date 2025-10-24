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
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("IPLookupPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["ip", "ip@KissShotChkBot"])]
pub struct IPLookupPlugin;
impl IPLookupPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.ip_lookup(bot, message, msg).await
    }
    pub async fn ip_lookup(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>IP Lookup Failed ❌</b>\n\
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

        if !check_access(user_id).await {
            let reply = format!(
                "<b>IP Lookup Failed ❌</b>\n\
                 <b>Reason:</b> You do not have access to use this bot here. Free access: @heckervaultchat\n\
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
                "<b>IP Lookup Failed ❌</b>\n\
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

        let text = if msg.len() > 5 {
            msg
        } else if let Some(reply) = message.reply_to_message() {
            if let Some(reply_text) = reply.text() {
                reply_text
            } else {
                "No IP provided"
            }
        } else {
            "No IP provided"
        };
        if text == "No IP provided" {
            let reply = format!(
                "<b>IP Lookup Failed ❌</b>\n\
                 <b>Reason:</b> No IP address provided.\n\
                 <b>Usage:</b> /ip 1.1.1.1 or reply to a message with IP\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let ip_pattern = Regex::new(r"\b(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\b").unwrap();
        let ip_matches: Vec<&str> = ip_pattern.find_iter(text).map(|m| m.as_str()).collect();
        if ip_matches.is_empty() {
            let reply = format!(
                "<b>IP Lookup Failed ❌</b>\n\
                 <b>Reason:</b> IP Filter didn't work! No valid IP found.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let ip_address = ip_matches[0];

        let response =
            match reqwest::get(&format!("https://scamalytics.com/ip/{}", ip_address)).await {
                Ok(resp) => resp,
                Err(e) => {
                    let _ = LOGGER.error(&format!("Error fetching IP data: {}", e)).await;
                    let reply = format!(
                        "<b>IP Lookup Failed ❌</b>\n\
                     <b>Reason:</b> Failed to fetch IP data from external service.\n\
                     <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    return;
                }
            };
        let html_content = match response.text().await {
            Ok(content) => content,
            Err(e) => {
                let _ = LOGGER.error(&format!("Error reading response: {}", e)).await;
                let reply = format!(
                    "<b>IP Lookup Failed ❌</b>\n\
                     <b>Reason:</b> Failed to read response from external service.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let extract_field = |pattern: &str, default: &str| -> String {
            if let Ok(regex) = Regex::new(pattern) {
                if let Some(captures) = regex.captures(&html_content) {
                    if let Some(matched) = captures.get(1) {
                        return matched.as_str().trim().to_string();
                    }
                }
            }
            default.to_string()
        };
        let score = extract_field(r#""score":"(.+?)""#, "Not Found");
        let risk = extract_field(r#""risk":"(.+?)""#, "Not Found");
        let isp = extract_field(r#"<td><a\s+href="[^"]+">([^<]+)</a></td>"#, "Not Found");
        let asn = extract_field(r#"<td>(\d+)\s+-\s+[^<]+</td>"#, "Not Found");
        let country = extract_field(
            r#"<tr>\s*<th>Country Name</th>\s*<td>([^<]+)</td>"#,
            "Not Found",
        );
        let city = extract_field(r#"<tr>\s*<th>Region</th>\s*<td>([^<]+)</td>"#, "Not Found");
        let zip_code = extract_field(r#"<th>Postal Code</th>\s+<td>(\d+)</td>"#, "Not Found");
        // Get user info for the footer
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
        let reply = format!(
            "
            <b>Internet Protocol Lookup Sucessful ✅</b>\n\n\
            <b>Address: <code>{ip_address}</code></b>\n\
            <b>Score: {score}</b>; \
            <b>Risk: {risk}</b>\n\
            <b>ISP: {isp}</b>\n\
            <b>ASN: {asn}</b>\n\
            <b>Country: {country}</b>\n\
            <b>City: {city}</b>\n\
            <b>Zip: {zip_code}</b>\n\n\
    {end_text}
        "
        );
        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }
}
