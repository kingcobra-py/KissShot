use crate::database::sql::{self, get_all_users};
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use chrono::Utc;
use lazy_static::lazy_static;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{ChatKind, Message, ParseMode, ReplyParameters, UserId};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("BroadcastPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["broadcast", "broadcast@KissShotChkBot"])]
pub struct BroadcastPlugin;
impl BroadcastPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.broadcast_chat(bot, message, msg).await;
    }
    async fn broadcast_chat(&self, bot: &Bot, message: &Message, msg: &str) {
        let wait_msg = match bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .reply_parameters(ReplyParameters::new(message.id))
            .await
        {
            Ok(msg) => msg,
            Err(_) => return,
        };
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_id = match message.from.as_ref() {
            Some(user) => user.id.0 as i64,
            None => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Broadcast Failed ❌</b>\n\n\
                             <b>Reason:</b> Could not identify user.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        if !check_admin(user_id).await {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Broadcast Failed ❌</b>\n\n\
                         <b>Reason:</b> Only administrators can use this command.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let users = match get_all_users(1000, 0).await {
            Ok(u) => u,
            Err(e) => {
                LOGGER.error(&format!("Failed to fetch users: {}", e)).await;
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Broadcast Failed ❌</b>\n\n\
                             <b>Reason:</b> Could not fetch users.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let user_count = users.len();
        if let Some(reply_to) = message.reply_to_message() {
            for user in &users {
                let _ = bot
                    .forward_message(UserId(user.user_id as u64), message.chat.id, reply_to.id)
                    .await;
            }
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Broadcast Approved ✅</b>\n\n\
                         <b>Message forwarded to:</b> {} users\n\
                         <b>Timestamp:</b> {}",
                        user_count, timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
        } else if !msg.trim().is_empty() {
            for user in &users {
                let _ = bot
                    .send_message(UserId(user.user_id as u64), msg)
                    .parse_mode(ParseMode::Html)
                    .await;
            }
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Broadcast Approved ✅</b>\n\n\
                         <b>Message sent to:</b> {} users\n\
                         <b>Timestamp:</b> {}",
                        user_count, timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
        } else {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Broadcast Failed ❌</b>\n\n\
                         <b>Reason:</b> Please reply to a message or provide a message to broadcast.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
        }
    }
}
