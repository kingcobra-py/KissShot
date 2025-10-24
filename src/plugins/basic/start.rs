use crate::config::get_config;
use crate::database::sql::{self, register_user};
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::basic::keyboards::*;
use crate::plugins::helpers::check_registration;
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
            tokio::runtime::Handle::current().block_on(get_logger("StartPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["start", "start@KissShotChkBot", "help", "help@KissShotChkBot", "cmds", "cmds@KissShotChkBot"])]
pub struct StartPlugin;
impl StartPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, _msg: &str) {
        self.start_plugin(bot, message).await;
    }
    async fn start_plugin(&self, bot: &Bot, message: &Message) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let user_name = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        let start_text = format!(
            "👋 Hello, <b>{}</b>!\n\
             🤖 I'm <b>KissShot</b> — A Telegram <b>CC Checker Bot</b>\n\n\
             <b>Phase:</b> Super Alpha\n\
             <b>Version:</b> 1.0.0\n\
             <b>Build:</b> Rust 1.83.0\n\
             <b>Branch:</b> Master\n\
             <b>Timestamp:</b> {}\n\n\
             <b>⚡ Ready to execute commands and assist you at full capacity.</b>",
            user_name, timestamp
        );
        let _ = bot
            .send_message(message.chat.id, start_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(get_start_keyboard().await)
            .await;
        if !check_registration(user_id).await {
            let user = crate::database::sql::User {
                user_id,
                username: user_name,
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
        }
    }
}
