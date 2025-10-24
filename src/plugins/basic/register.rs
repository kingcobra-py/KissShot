use crate::config::get_config;
use crate::database::sql::{self, fetch_user, register_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::basic::keyboards::*;
use crate::plugins::helpers::check_registration;
use crate::safe_database_operation;
use chrono::Utc;
use lazy_static::lazy_static;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::prelude::Requester;
use teloxide::types::{Message, ParseMode};
use teloxide::Bot;
use teloxide_plugin::TeloxidePlugin;
lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("RegisterPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["register", "register@KissShotChkBot"])]
pub struct RegisterPlugin;
impl RegisterPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, _msg: &str) {
        self.register_plugin(bot, message).await;
    }
    async fn register_plugin(&self, bot: &Bot, message: &Message) {
        let sent_msg = bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_id = message.from.as_ref().map(|u| u.id.0 as i64).unwrap_or(0);
        let user_name = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        if !check_registration(user_id).await {
            let user = crate::database::sql::User {
                user_id,
                username: user_name.clone(),
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
            let _ = register_user(&user).await;
            let user = handle_database_error!(
                bot,
                message,
                fetch_user(user_id).await,
                "User Registration"
            )
            .unwrap();
            let balance = user.balance;
            let status = user.status;
            let antispam = user.antispam;
            let registered_at = user.registered_at;
            let reply = format!(
                "<b>Registration Approved ✅</b>\n\n\
                 <b>Username:</b> {}\n\
                 <b>Balance:</b> {}\n\
                 <b>Status:</b> {}\n\
                 <b>Antispam:</b> {}\n\
                 <b>Registered At:</b> {}\n\n<b>Welcome to KissShot! 🌟</b>",
                user_name, balance, status, antispam, registered_at
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
        } else {
            let reply = format!(
                "<b>Registration Failed ❌</b>\n\n\
                 <b>Reason:</b> You are already registered.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
        }
    }
}
