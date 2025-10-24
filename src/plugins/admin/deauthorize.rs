use crate::logging::{get_logger, LoggerHandle};
use crate::plugins::helpers::*;
use chrono::Utc;
use teloxide::prelude::*;
use teloxide::types::{Message, ParseMode, ReplyParameters};
use teloxide::Bot;
use teloxide_plugin::TeloxidePlugin;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("DeauthorizeChatPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["deauthorize", "deauthorize@KissShotChkBot"])]
pub struct DeauthorizeChatPlugin;
impl DeauthorizeChatPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, _msg: &str) {
        self.deauthorize_chat(bot, message).await;
    }
    async fn deauthorize_chat(&self, bot: &Bot, message: &Message) {
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
        let chat_id = message.chat.id;
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let chat_title = message.chat.title().unwrap_or_default();
        let issuer = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        if !check_admin(user_id).await {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Deauthorization Failed ❌</b>\n\n\
                             <b>Reason:</b> Only administrators can use this command.\n\
                             <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let file_path = "src/resources/auth/groups.txt";
        let contents = fs::read_to_string(file_path).await.unwrap_or_default();
        if !contents.contains(&chat_id.0.to_string()) {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Deauthorization Failed ❌</b>\n\n\
                         <b>Reason:</b> This group is not authorized.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let new_contents: String = contents
            .lines()
            .filter(|line| line.trim() != chat_id.0.to_string())
            .map(|line| format!("{}\n", line))
            .collect();
        let mut file = match OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(file_path)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                LOGGER.error(&format!("Failed to open file: {}", e)).await;
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Deauthorization Failed ❌</b>\n\n\
                             <b>Reason:</b> Failed to open authorization file.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        if let Err(e) = file.write_all(new_contents.as_bytes()).await {
            LOGGER.error(&format!("Failed to write file: {}", e)).await;
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Deauthorization Failed ❌</b>\n\n\
                         <b>Reason:</b> Failed to update authorization file.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let success_text = format!(
            "<b>Deauthorization Approved ✅</b>\n\n\
             <b>Chat Title:</b> {}\n\
             <b>Chat ID:</b> {}\n\
             <b>Issuer:</b> {} [Admin]\n\
             <b>Timestamp:</b> {}",
            chat_title, chat_id, issuer, timestamp
        );
        let _ = bot
            .edit_message_text(wait_msg.chat.id, wait_msg.id, success_text)
            .parse_mode(ParseMode::Html)
            .await;
        LOGGER
            .info(&format!(
                "Group chat {} has been deauthorized by user {}",
                chat_id.0,
                message.from.as_ref().expect("Operation failed").id
            ))
            .await;
    }
}
