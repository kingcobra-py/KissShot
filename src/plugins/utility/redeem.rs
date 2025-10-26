use crate::config::get_config;
use crate::database::kvs::{self, get_gift_code, update_gift_code};
use crate::database::sql::{self, fetch_user, register_user, update_user};
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use chrono::Utc;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("KUtilities_redeem"))
        })
    };
}
#[TeloxidePlugin(commands = ["redeem", "redeem@KissShotChkBot"])]
pub struct RedeemPlugin;
impl RedeemPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.redeem_code(bot, message, msg).await
    }
    pub async fn redeem_code(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
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
                "<b>Code Redemption Failed ❌</b>\n\n\
                 <b>Reason:</b> You are not registered.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(
                message.chat.id,
                sent_msg.id,
                "<b>You are not registered. Please, Wait while we automatically register you.</b>",
            )
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
            let user = crate::database::sql::User {
                user_id,
                username: first_name.clone(),
                balance: get_config()
                    .expect("Operation failed")
                    .config
                    .default_user_value
                    .balance as i64,
                status: get_config()
                    .expect("Operation failed")
                    .config
                    .default_user_value
                    .status
                    .clone(),
                antispam: get_config()
                    .expect("Operation failed")
                    .config
                    .default_user_value
                    .antispam as i32,
                registered_at: Utc::now(),
                expires_at: None,
            };
            if let Err(e) = register_user(&user).await {
                LOGGER
                    .error(&format!("❌ Failed to register user: {}", e))
                    .await;
                let reply =
                    "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Failed to register user.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        }

        if check_banned(user_id).await {
            let reply = format!(
                "<b>Code Redemption Failed ❌</b>\n\n\
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

        let mut parts = msg.split_whitespace();
        let code = match parts.next() {
            Some(first) => {
                if first.starts_with('/') {
                    match parts.next() {
                        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
                        _ => {
                            let reply = "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Please provide a valid redeem code.";
                            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                .parse_mode(ParseMode::Html)
                                .await
                                .unwrap();
                            return;
                        }
                    }
                } else if !first.trim().is_empty() {
                    first.trim().to_string()
                } else {
                    let reply = "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Please provide a valid redeem code.";
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    return;
                }
            }
            None => {
                let reply = "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Please provide a valid redeem code.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let user_data = match fetch_user(user_id).await {
            Ok(Some(user)) => user,
            Ok(None) => {
                let reply = "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> User not found in database.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            Err(e) => {
                LOGGER
                    .error(&format!("❌ Failed to fetch user data: {}", e))
                    .await;
                let reply =
                    "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Failed to fetch user data.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        if user_data.status == "ADMIN" {
            let reply =
                "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Admins cannot redeem codes.";
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        if user_data.status == "PREMIUM" {
            let reply =
                "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> You are already premium.";
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let gift_data = match get_gift_code(&code).await {
            Ok(Some(data)) => data,
            Ok(None) => {
                let reply =
                    "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Invalid redeem code.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            Err(e) => {
                LOGGER
                    .error(&format!("❌ Failed to fetch gift data: {}", e))
                    .await;
                let reply =
                    "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Failed to fetch gift data.";
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        if gift_data.used {
            let reply = "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Redeem code has already been used.";
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let gift_balance = gift_data.gift_balance;

        if let Err(e) = update_gift_code(&code, None, Some(true)).await {
            LOGGER
                .error(&format!("❌ Failed to mark code as used: {}", e))
                .await;
            let reply =
                "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Failed to process redeem code.";
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let new_balance = user_data.balance + gift_balance;
        let user_update = sql::UserUpdate {
            username: None,
            balance: Some(new_balance),
            status: Some("PREMIUM".to_string()),
            antispam: Some(10),
            registered_at: None,
            expires_at: None,
        };
        if let Err(e) = update_user(user_id, &user_update).await {
            LOGGER
                .error(&format!("❌ Failed to update user data: {}", e))
                .await;
            let reply =
                "<b>Code Redemption Failed ❌</b>\n\n<b>Reason:</b> Failed to update user data.";
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let user_id_for_link = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
        let role = "Premium";
        let end_text = format!(
            "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
            <b>Timestamp:</b> {}",
            user_id_for_link, first_name, role, timestamp
        );
        let success_msg = format!(
            "<b>Code Redemption Successful ✅</b>\n\n\
             <b>Code:</b> <code>{}</code>\n\
             <b>Status:</b> PREMIUM\n\
             <b>Credits Added:</b> {}\n\
             <b>New Balance:</b> {}\n\
             <b>Antispam:</b> 10's\n\n\
             <b>Welcome to Premium! 🌟</b>\n\n\
             {}",
            code, gift_balance, new_balance, end_text
        );
        bot.edit_message_text(message.chat.id, sent_msg.id, success_msg)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
        LOGGER
            .info(&format!(
                "✅ User {} ({}) redeemed code {} for {} credits",
                user_id, first_name, code, gift_balance
            ))
            .await;
    }
}
