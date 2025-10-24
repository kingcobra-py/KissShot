use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use rand::distr::Alphanumeric;
use rand::Rng;
use serde_json;
use std::fs::File;
use std::io::Write;
use teloxide::payloads::SendDocumentSetters;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::InputFile;
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("SKGeneratorPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["skgen", "skgen@KissShotChkBot"])]
pub struct SKGeneratorPlugin;
impl SKGeneratorPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.sk_generator(bot, message, msg).await
    }
    pub async fn sk_generator(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>Stripe Key Generation Failed ❌</b>\n\
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
                "<b>Stripe Key Generation Failed ❌</b>\n\
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
        let parts = msg.split_whitespace().collect::<Vec<&str>>();
        let requested_amount = parts
            .get(0)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(10);
        let prefix = parts.get(1).unwrap_or(&"sk_live_").to_string();
        let length = parts
            .get(2)
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(24);
        let output = parts.get(3).unwrap_or(&"stdout").to_string();
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
        let max_limit = match role.to_lowercase().as_str() {
            "admin" => 5001,
            "premium" => 5001,
            _ => 10001,
        };
        if requested_amount > max_limit {
            let reason = match role.to_lowercase().as_str() {
                "admin" => "Admin maximum is 5001 Stripe Keys at once.",
                "premium" => "Premium maximum is 5001 Stripe Keys at once.",
                _ => "Free users can generate up to 10001 Stripe Keys at once. Upgrade your account for higher limits.",
            };
            let reply = format!(
                "<b>Stripe Key Generation Failed ❌</b>\n\
                 <b>Reason:</b> {}\n\
                 <b>Requested Amount:</b> {}\n\
                 <b>Timestamp:</b> {}",
                reason, requested_amount, timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let mut keys = Vec::new();
        for _ in 0..requested_amount {
            let random_part: String = rand::rng()
                .sample_iter(&Alphanumeric)
                .take(length)
                .map(char::from)
                .collect();
            keys.push(format!("{}{}", prefix, random_part));
        }
        let formatted_keys = keys.join("\n");
        let end_text = format!(
            "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
            <b>Timestamp:</b> {}",
            user_id,
            first_name,
            format!("{}{}", &role[..1].to_uppercase(), &role[1..].to_lowercase()),
            timestamp
        );
        if requested_amount > 30 {
            let file_name = format!("x{}_Stripe_Keys.txt", requested_amount);
            let mut file = File::create(&file_name).unwrap();
            writeln!(file, "{}", formatted_keys).unwrap();
            bot.send_document(message.chat.id, InputFile::file(file_name.clone()))
                .caption(format!(
                    "<b>Stripe Key Generation Successful ✅</b>\n\n\
                     <b>Prefix:</b> {}\n\
                     <b>Length:</b> {}\n\n\
                     <b>Generated Amount:</b> {}\n\n\
                {}
                     ",
                    prefix, length, requested_amount, end_text
                ))
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            std::fs::remove_file(file_name).unwrap();
            bot.delete_message(message.chat.id, sent_msg.id)
                .await
                .unwrap();
        } else {
            let reply = format!(
                "<b>Stripe Key Generation Successful ✅</b>\n\n\
                 <b>Prefix:</b> {}\n\
                 <b>Length:</b> {}\n\n\
                 <b>Generated Amount:</b> {}\n\n\
                 <code>{}</code>\n\n{}",
                prefix, length, requested_amount, formatted_keys, end_text
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
        }
    }
}
