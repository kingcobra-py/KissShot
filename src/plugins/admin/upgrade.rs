use crate::database::sql::{self, UserUpdate};
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
            tokio::runtime::Handle::current().block_on(get_logger("UpgradeChatPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["upgrade", "upgrade@KissShotChkBot"])]
pub struct UpgradeChatPlugin;
impl UpgradeChatPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.upgrade_chat(bot, message, msg).await;
    }
    async fn upgrade_chat(&self, bot: &Bot, message: &Message, msg: &str) {
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
        let issuer = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        let user_id = match message.from.as_ref() {
            Some(user) => user.id.0 as i64,
            None => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Upgradation Failed ❌</b>\n\n\
                             <b>Reason:</b> Could not identify issuer.\n\
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
                        "<b>Upgradation Failed ❌</b>\n\n\
                         <b>Reason:</b> Only administrators can perform this action.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        if check_banned(user_id).await {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Upgradation Failed ❌</b>\n\n\
                         <b>Reason:</b> Banned users cannot be upgraded.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let target_id: Option<i64> = if let Ok(id) = msg.trim().parse::<i64>() {
            Some(id)
        } else if let Some(reply_to) = message.reply_to_message() {
            reply_to.from.as_ref().map(|u| u.id.0 as i64)
        } else {
            None
        };
        let target_id = match target_id {
            Some(id) => id,
            None => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Upgradation Failed ❌</b>\n\n\
                             <b>Reason:</b> Could not identify target user.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let update_user = UserUpdate {
            status: Some("PREMIUM".to_string()),
            ..Default::default()
        };
        if sql::update_user(target_id, &update_user).await.is_ok() {
            let success_text = format!(
                "<b>Upgradation Approved ✅</b>\n\n\
                 <b>Target User ID:</b> {}\n\
                 <b>Issuer:</b> {}\n\
                 <b>Timestamp:</b> {}",
                target_id, issuer, timestamp
            );
            let _ = bot
                .edit_message_text(wait_msg.chat.id, wait_msg.id, success_text)
                .parse_mode(ParseMode::Html)
                .await;
            LOGGER
                .info(&format!("User upgraded successfully: {}", target_id))
                .await;
        } else {
            LOGGER
                .error(&format!("Failed to upgrade user: {}", target_id))
                .await;
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Upgradation Failed ❌</b>\n\n\
                         <b>Reason:</b> Something went wrong. Please try again later.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
        }
    }
}
