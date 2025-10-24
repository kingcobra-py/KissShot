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
            tokio::runtime::Handle::current().block_on(get_logger("AccountInfoPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["id", "id@KissShotChkBot", "me", "me@KissShotChkBot", "status", "status@KissShotChkBot"])]
pub struct AccountInfoPlugin;
impl AccountInfoPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.account_info(bot, message, msg).await
    }
    pub async fn account_info(&self, bot: &Bot, message: &Message, msg: &str) {
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
                "<b>Account Information Fetching Failed ❌</b>\n\
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
                "<b>Account Information Fetching Failed ❌</b>\n\
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

        let (target_user_id, target_user) = if let Some(reply) = message.reply_to_message() {
            if let Some(reply_from) = reply.from.as_ref() {
                (reply_from.id.0 as i64, reply_from)
            } else {
                (user_id, message.from.as_ref().expect("Operation failed"))
            }
        } else {
            (user_id, message.from.as_ref().expect("Operation failed"))
        };

        let user_data = handle_database_error!(
            bot,
            message,
            safe_fetch_user(target_user_id).await,
            "Database Operation"
        );

        let full_name = target_user.full_name();

        let username = target_user
            .username
            .as_deref()
            .map(|u| format!("@{}", u))
            .unwrap_or_else(|| "No Username".to_string());

        let current_user = handle_database_error!(
            bot,
            message,
            safe_fetch_user(user_id).await,
            "Database Operation"
        );
        let role = current_user
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
        let reply = if let Some(user) = user_data {
            format!(
                "<b>Account Information Fetching Successful ✅</b>\n\n\
                 <b>Name:</b> {}\n\
                 <b>Identification Number:</b> {}\n\
                 <b>Username:</b> {}\n\n\
                 <b>Status:</b> {}\n\
                 <b>Balance:</b> {}\n\
                 <b>Antispam:</b> {}'s\n\
                 <b>Registered:</b> {}\n\
                 <b>Expires:</b> {}\n\n\
                 {}",
                full_name,
                target_user_id,
                username,
                format!(
                    "{}{}",
                    &user.status[..1].to_uppercase(),
                    &user.status[1..].to_lowercase()
                ),
                user.balance,
                user.antispam,
                user.registered_at.format("%Y-%m-%d %H:%M:%S"),
                user.expires_at
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "Never".to_string()),
                end_text
            )
        } else {
            format!(
                "<b>Account Information Successful ✅</b>\n\n\
                 <b>Name:</b> {}\n\
                 <b>Identification Number:</b> {}\n\
                 <b>Username:</b> {}\n\n\
                 <b>Status:</b> Account Not Registered\n\
                 <b>Note:</b> User not found in database\n\n\
                 {}",
                full_name, target_user_id, username, end_text
            )
        };
        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }
}
