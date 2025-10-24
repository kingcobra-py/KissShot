use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use reqwest;
use std::fs;
use teloxide::payloads::{EditMessageTextSetters, SendDocumentSetters, SendMessageSetters};
use teloxide::types::{InputFile, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("ProxyFinderPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["proxy", "proxy@KissShotChkBot"])]
pub struct ProxyFinderPlugin;
impl ProxyFinderPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.proxy_find(bot, message, msg).await
    }
    pub async fn proxy_find(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>Proxy Fetching Failed ❌</b>\n\
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
                "<b>Proxy Fetching Failed ❌</b>\n\
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

        let command_parts: Vec<&str> = msg.split_whitespace().collect();
        let proxy_type = if command_parts.len() > 1 {
            command_parts[1]
        } else {
            "http"
        };

        let valid_types = ["http", "https", "socks4", "socks5"];
        if !valid_types.contains(&proxy_type.to_lowercase().as_str()) {
            let reply = format!(
                "<b>Proxy Fetching Failed ❌</b>\n\
                 <b>Reason:</b> Invalid proxy type! Use: http, https, socks4, or socks5\n\
                 <b>Usage:</b> /proxy http\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let api_url = format!(
            "https://api.proxyscrape.com/?request=getproxies&proxytype={}&timeout=10000&country=all&ssl=all&anonymity=all",
            proxy_type.to_lowercase()
        );
        let proxy_list = match reqwest::get(&api_url).await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(text) => text,
                        Err(e) => {
                            let _ = LOGGER.error(&format!("Error reading response: {}", e)).await;
                            let reply = format!(
                                "<b>Proxy Fetching Failed ❌</b>\n\
                                 <b>Reason:</b> Failed to read response from proxy API.\n\
                                 <b>Timestamp:</b> {}",
                                timestamp
                            );
                            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                .parse_mode(ParseMode::Html)
                                .await
                                .unwrap();
                            return;
                        }
                    }
                } else {
                    let _ = LOGGER.error(&format!("API returned error status: {}", response.status())).await;
                    let reply = format!(
                        "<b>Proxy Fetching Failed ❌</b>\n\
                         <b>Reason:</b> Proxy API returned an error.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    return;
                }
            }
            Err(e) => {
                let _ = LOGGER.error(&format!("Error fetching proxy data: {}", e)).await;
                let reply = format!(
                    "<b>Proxy Fetching Failed ❌</b>\n\
                     <b>Reason:</b> Failed to fetch proxy list from external service.\n\
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
        if proxy_list.trim().is_empty() {
            let reply = format!(
                "<b>Proxy Fetching Failed ❌</b>\n\
                 <b>Reason:</b> No proxies found!\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let proxy_lines: Vec<&str> = proxy_list
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        let proxy_count = proxy_lines.len();

        let temp_file = format!("{}_proxies.txt", proxy_type);
        if let Err(e) = fs::write(&temp_file, &proxy_list) {
            let _ = LOGGER.error(&format!("Error writing file: {}", e)).await;
            let reply = format!(
                "<b>Proxy Fetching Failed ❌</b>\n\
                 <b>Reason:</b> Failed to create proxy file.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
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
        let caption = format!(
            "<b>Proxy List Fetching Successful ✅</b>\n\n\
             <b>Proxy Type:</b> {}\n\
             <b>Total Proxies:</b> {}\n\
             <b>Proxy Format:</b> IP:PORT\n\n\
             {}",
            proxy_type.to_uppercase(),
            proxy_count,
            end_text
        );

        bot.delete_message(message.chat.id, sent_msg.id)
            .await
            .unwrap();

        if let Err(e) = bot
            .send_document(message.chat.id, InputFile::file(&temp_file))
            .caption(caption)
            .parse_mode(ParseMode::Html)
            .await
        {
            let _ = LOGGER.error(&format!("Error sending document: {}", e)).await;
        }

        let _ = fs::remove_file(&temp_file);
    }
}
