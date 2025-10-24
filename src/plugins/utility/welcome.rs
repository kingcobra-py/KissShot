use crate::database::sql::{self};
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::basic::keyboards::get_start_keyboard;
use crate::plugins::basic::*;
use chrono::Utc;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("WelcomePlugin"))
        })
    };
}
pub async fn handle_chat_member_updated(bot: &Bot, update: &teloxide::types::ChatMemberUpdated) {
    let bot_clone = bot.clone();
    let update_clone = update.clone();
    process_welcome_message(&bot_clone, &update_clone).await;
}
async fn process_welcome_message(bot: &Bot, update: &teloxide::types::ChatMemberUpdated) {
    let old_member = &update.old_chat_member;
    let new_member = &update.new_chat_member;

    let was_left = matches!(old_member.status(), teloxide::types::ChatMemberStatus::Left);
    let is_member = matches!(
        new_member.status(),
        teloxide::types::ChatMemberStatus::Member
    );
    if was_left && is_member {
        let chat = &update.chat;
        let user = &new_member.user;

        if user.is_bot {
            LOGGER.debug("Skipping welcome for bot user").await;
            return;
        }

        let is_group_chat = matches!(chat.kind, teloxide::types::ChatKind::Public(_));
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        if is_group_chat {
            let welcome_text = format!(
                "👋 Hello, <b>{}</b>!\n\
                     🤖 I'm <b>KissShot</b> — A Telegram <b>CC Checker Bot</b>\n\n\
                     <b>Phase:</b> Super Alpha\n\
                     <b>Version:</b> 1.0.0\n\
                     <b>Build:</b> Rust 1.83.0\n\
                     <b>Branch:</b> Master\n\
                     <b>Timestamp:</b> {}\n\n\
                     <b>⚡ Ready to execute commands and assist you at full capacity.</b>",
                user.full_name(),
                timestamp
            );
            match bot
                .send_message(chat.id, welcome_text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .reply_markup(get_start_keyboard().await)
                .await
            {
                Err(e) => {
                    LOGGER
                        .error(&format!("❌ Failed to send welcome message: {}", e))
                        .await;
                }
                _ => {
                    LOGGER
                        .info(&format!(
                            "✅ Welcome message sent to user {} in chat {}",
                            user.id.0, chat.id.0
                        ))
                        .await;
                }
            }
        }
    }
}
