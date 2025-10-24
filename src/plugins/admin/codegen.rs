use crate::config::get_config;
use crate::database::kvs::{self, get_kvs};
use crate::database::sql::{self, get_all_users};
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use chrono::Utc;
use lazy_static::lazy_static;
use rand::Rng;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{ChatKind, Message, ParseMode, ReplyParameters, UserId};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("CodeGenerationPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["codegen", "codegen@KissShotChkBot"])]
pub struct CodeGenerationPlugin;
impl CodeGenerationPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.codegen_plugin(bot, message, msg).await;
    }
    async fn codegen_plugin(&self, bot: &Bot, message: &Message, msg: &str) {
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
                            "<b>Code Generation Failed ❌</b>\n\n\
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
                        "<b>Code Generation Failed ❌</b>\n\n\
                         <b>Reason:</b> Only administrators can use this command.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let args: Vec<&str> = msg.trim().split_whitespace().collect();
        if args.len() != 2 {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Code Generation Failed ❌</b>\n\n\
                         <b>Reason:</b> Usage: /codegen &lt;total_codes&gt; &lt;credits&gt;\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let total_codes = match args[0].parse::<i64>() {
            Ok(n) => n,
            Err(_) => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Code Generation Failed ❌</b>\n\n\
                             <b>Reason:</b> Invalid number of codes.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let credits = match args[1].parse::<i64>() {
            Ok(n) => n,
            Err(_) => {
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Code Generation Failed ❌</b>\n\n\
                             <b>Reason:</b> Invalid credits amount.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        if total_codes <= 0 || total_codes > 1000 {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Code Generation Failed ❌</b>\n\n\
                         <b>Reason:</b> Total codes must be between 1 and 1000.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        if credits <= 0 || credits > 1000000 {
            let _ = bot
                .edit_message_text(
                    wait_msg.chat.id,
                    wait_msg.id,
                    &format!(
                        "<b>Code Generation Failed ❌</b>\n\n\
                         <b>Reason:</b> Credits must be between 1 and 1,000,000.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    ),
                )
                .parse_mode(ParseMode::Html)
                .await;
            return;
        }
        let config = match get_config() {
            Ok(cfg) => cfg,
            Err(e) => {
                LOGGER.error(&format!("Failed to get config: {}", e)).await;
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Code Generation Failed ❌</b>\n\n\
                             <b>Reason:</b> Configuration error.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        };
        let gift_code_config = &config.config.gift_code;
        let prefix = &gift_code_config.prefix;
        let mut generated_codes = Vec::new();
        for _ in 0..total_codes {
            let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".chars().collect();
            let mut random_part = String::new();
            for _ in 0..20 {
                let random_index = (rand::rng().random::<u32>() as usize) % chars.len();
                random_part.push(chars[random_index]);
            }
            let code = format!("{}{}", prefix, random_part);
            generated_codes.push(code);
        }
        let mut stored_count = 0;
        let mut failed_codes = Vec::new();
        match kvs::get_kvs().await {
            Ok(_) => {
                for code in &generated_codes {
                    match kvs::register_gift_code(code, Some(credits), false, None).await {
                        Ok(true) => stored_count += 1,
                        Ok(false) => {
                            failed_codes.push(code.clone());
                            LOGGER
                                .warning(&format!("Failed to store gift code: {}", code))
                                .await;
                        }
                        Err(e) => {
                            failed_codes.push(code.clone());
                            LOGGER
                                .error(&format!("Error storing gift code {}: {}", code, e))
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                LOGGER.error(&format!("KVS not available: {}", e)).await;
                let _ = bot
                    .edit_message_text(
                        wait_msg.chat.id,
                        wait_msg.id,
                        &format!(
                            "<b>Code Generation Failed ❌</b>\n\n\
                             <b>Reason:</b> Database not available. Codes generated but not stored.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        ),
                    )
                    .parse_mode(ParseMode::Html)
                    .await;
                return;
            }
        }
        let codes_text = generated_codes.join("\n");
        let mut response = format!(
            "<b>Code Generation Approved ✅</b>\n\n\
             <b>Total Generated:</b> <code>{}</code>\n\
             <b>Credits (Per Code):</b> <code>{}</code>\n\
             <b>Timestamp:</b> {}",
            total_codes, credits, timestamp
        );
        if !failed_codes.is_empty() {
            response.push_str(&format!(
                "\n<b>⚠️ Failed to store:</b> <code>{}</code> codes",
                failed_codes.len()
            ));
        }
        response.push_str(&format!("\n\n<code>{}</code>", codes_text));
        let _ = bot
            .edit_message_text(wait_msg.chat.id, wait_msg.id, response)
            .parse_mode(ParseMode::Html)
            .await;
        LOGGER
            .info(&format!(
                "Generated {} gift codes with {} credits each by admin {}",
                total_codes, credits, user_id
            ))
            .await;
    }
}
