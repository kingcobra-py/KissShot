use crate::config::get_config;
use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use regex::Regex;
use std::fs;
use teloxide::payloads::{EditMessageTextSetters, SendDocumentSetters, SendMessageSetters};
use teloxide::types::{InputFile, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("CCFilterPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["filter", "filter@KissShotChkBot"])]
pub struct CCFilterPlugin;
impl CCFilterPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.cc_filter(bot, message, msg).await
    }
    pub async fn cc_filter(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>Credit Card Filterization Failed ❌</b>\n\n\
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
                "<b>Credit Card Filterization Failed ❌</b>\n\n\
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
                "<b>Credit Card Filterization Failed ❌</b>\n\n\
                 <b>Reason:</b> No file or text found!\n\
                 <b>Usage:</b> /filter or reply to a message with CCs\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let cc_regex_pattern = &get_config()
            .expect("Operation failed")
            .config
            .regex
            .cc_regex;
        let cc_pattern = match Regex::new(cc_regex_pattern) {
            Ok(regex) => regex,
            Err(e) => {
                let _ = LOGGER.error(&format!("Invalid CC regex pattern: {}", e)).await;
                let reply = format!(
                    "<b>Credit Card Filterization Failed ❌</b>\n\n\
                     <b>Reason:</b> Invalid regex pattern in config.\n\
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
        let cc_list: Vec<&str> = cc_pattern.find_iter(&text).map(|m| m.as_str()).collect();
        if cc_list.is_empty() {
            let reply = format!(
                "<b>Credit Card Filterization Failed ❌</b>\n\n\
                 <b>Reason:</b> No Credit Card found!\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        bot.delete_message(message.chat.id, sent_msg.id)
            .await
            .unwrap();

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
        if cc_list.len() > 15 {
            let file_name = "filtered_cc.txt";
            let cc_content = cc_list.join("\n");
            if let Err(e) = fs::write(file_name, &cc_content) {
                let _ = LOGGER.error(&format!("Error writing file: {}", e)).await;
                let reply = format!(
                    "<b>Credit Card Filterization Failed ❌</b>\n\n\
                     <b>Reason:</b> Failed to create file.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.send_message(message.chat.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            let caption = format!(
                "<b>Credit Card Filterization Successful ✅</b>\n\n\
                 <b>Credit Card Count:</b> {}\n\n\
                 {}",
                cc_list.len(),
                end_text
            );

            if let Err(e) = bot
                .send_document(message.chat.id, InputFile::file(file_name))
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .await
            {
                let _ = LOGGER.error(&format!("Error sending document: {}", e)).await;
            }

            let _ = fs::remove_file(file_name);
        } else {
            let cc_text = cc_list.join("\n");
            let reply = format!(
                "<b>Credit Card Filterization Successful ✅</b>\n\n\
                 <b>Credit Card Count:</b> {}\n\n\
                 <code>{}</code>\n\n\
                 {}",
                cc_list.len(),
                cc_text,
                end_text
            );
            bot.send_message(message.chat.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
        }
    }
}
