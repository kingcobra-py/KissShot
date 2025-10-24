use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("BinLookupPlugin"))
        })
    };
}

#[TeloxidePlugin(commands = ["bin", "bin@KissShotChkBot"])]
pub struct BinLookupPlugin;

impl BinLookupPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.bin_lookup(bot, message, msg).await
    }

    pub async fn bin_lookup(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>BIN Lookup Failed ❌</b>\n\n\
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
                "<b>BIN Lookup Failed ❌</b>\n\n\
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

        let text = if msg.len() > 8 {
            msg
        } else if let Some(reply) = message.reply_to_message() {
            if let Some(reply_text) = reply.text() {
                if reply_text.len() > 6 {
                    reply_text
                } else {
                    msg
                }
            } else {
                msg
            }
        } else {
            msg
        };

        let parts: Vec<&str> = text.split_whitespace().collect();
        let raw_bin = parts.get(0).copied().unwrap_or("").trim();

        if raw_bin.len() < 6 || raw_bin.len() > 16 {
            let reply = format!(
                "<b>BIN Lookup Failed ❌</b>\n\n\
                 <b>Reason:</b> BIN must be 6-16 characters.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let bin_template = match normalize_bin(raw_bin) {
            Ok(t) => t,
            Err(e) => {
                let reply = format!(
                    "<b>BIN Lookup Failed ❌</b>\n\n\
                     <b>Reason:</b> BIN Error: {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let bin_for_lookup = &bin_template.replace("x", "")[..6.min(bin_template.len())];

        match lookup_bin(bin_for_lookup).await {
            Ok(bin_info) => {
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

                let bin_lookup_formatted = format!(
                    "<b>Information:</b> {} - {} - {}\n\
                    <b>Bank:</b> {}\n\
                    <b>Country:</b> {} {}",
                    bin_info.btype,
                    bin_info.level,
                    bin_info.vendor,
                    bin_info.bank,
                    bin_info.country,
                    bin_info.flag
                );

                let reply = format!(
                    "<b>BIN Lookup Successful ✅</b>\n\n\
                     <b>Bank Identification Number:</b> {}\n\
                     {}\n\n\
                     {}",
                    bin_info.bin, bin_lookup_formatted, end_text
                );

                bot.send_message(message.chat.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
            Err(e) => {
                let reply = format!(
                    "<b>BIN Lookup Failed ❌</b>\n\n\
                     <b>Reason:</b> Failed to lookup BIN: {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
        }
    }
}
