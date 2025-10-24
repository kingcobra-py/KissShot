use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use chrono::Utc;
use std::path::Path;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::sugar::request::RequestLinkPreviewExt;
use teloxide::types::ReplyParameters;
use teloxide::types::{ChatKind, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("AuthorizeChatPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["authorize", "authorize@KissShotChkBot"])]
pub struct AuthorizeChatPlugin;
impl AuthorizeChatPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.authorize_chat(bot, message, msg).await;
    }
    async fn authorize_chat(&self, bot: &Bot, message: &Message, _msg: &str) {
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
        let chat_type = &message.chat.kind;
        let is_group = matches!(chat_type, ChatKind::Public(_));
        if !is_group {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> This command can only be used in group chats.\n\
<b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let user_id = match message.from.as_ref() {
            Some(user) => user.id.0 as i64,
            None => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Unable to identify user.\n\
<b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let is_admin = check_admin(user_id).await;
        if !is_admin {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Only administrators can use this command.\n\
<b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let is_authorized = check_group(chat_id.0).await;
        if is_authorized {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> This group is already authorized.\n\
<b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let file_path = "resources/auth/groups.txt";
        let dir_path = Path::new(file_path).parent().unwrap_or(Path::new("."));
        if let Err(e) = tokio::fs::create_dir_all(dir_path).await {
            LOGGER
                .error(&format!("Failed to create directory: {}", e))
                .await;
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Failed to create authorization directory.\n\
<b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .await
        {
            Ok(file) => file,
            Err(e) => {
                LOGGER.error(&format!("Failed to open file: {}", e)).await;
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Failed to access authorization file.\n\
<b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let mut contents = String::new();
        if let Err(e) = file.read_to_string(&mut contents).await {
            LOGGER.error(&format!("Failed to read file: {}", e)).await;
        }
        let chat_id_str = chat_id.0.to_string();
        if !contents.contains(&chat_id_str) {
            let new_entry = format!("{}\n", chat_id_str);
            let auth_file_path = "src/resources/auth/groups.txt";
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(auth_file_path)
                .await
            {
                Ok(mut f) => {
                    if let Err(e) = f.write_all(new_entry.as_bytes()).await {
                        LOGGER
                            .error(&format!("Failed to write to file: {}", e))
                            .await;
                        let _ = bot
                            .edit_message_text(
                                wait_msg.chat.id,
                                wait_msg.id,
                                &format!(
                                    "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Failed to save authorization.\n\
<b>Timestamp:</b> {}",
                                    timestamp
                                ),
                            )
                            .parse_mode(ParseMode::Html)
                            .await;
                        return;
                    }
                }
                Err(e) => {
                    LOGGER.error(&format!("Failed to open file: {}", e)).await;
                    let _ = bot
                        .edit_message_text(
                            wait_msg.chat.id,
                            wait_msg.id,
                            &format!(
                                "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> Failed to access authorization file.\n\
<b>Timestamp:</b> {}",
                                timestamp
                            ),
                        )
                        .parse_mode(ParseMode::Html)
                        .await;
                    return;
                }
            }
        } else {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Authorization Failed ❌</b>\n\n\
<b>Reason:</b> This group is already in the authorization list.\n\
<b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let success_text = format!(
            "<b>Authorization Approved ✅</b>\n\n\
<b>Chat Title:</b> {}\n<b>Chat ID:</b> {}\n<b>Issuer:</b> {} [Admin]\n\
<b>Timestamp:</b> {}",
            message.chat.title().unwrap_or_default(),
            chat_id,
            message
                .from
                .as_ref()
                .map(|u| u.full_name())
                .unwrap_or_else(|| "Unknown".to_string()),
            timestamp
        );
        let _ = bot
            .edit_message_text(wait_msg.chat.id, wait_msg.id, success_text)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await;
        LOGGER
            .info(&format!(
                "Group chat {} has been authorized by user {}",
                chat_id.0, user_id
            ))
            .await;
    }
}
