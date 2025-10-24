use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Message};
use teloxide::Bot;
#[derive(Debug)]
pub struct PluginResponse {
    pub text: String,
    pub keyboard: Option<InlineKeyboardMarkup>,
}
impl PluginResponse {
    pub fn text_only(text: String) -> Self {
        Self {
            text,
            keyboard: None,
        }
    }
    pub fn with_keyboard(text: String, keyboard: InlineKeyboardMarkup) -> Self {
        Self {
            text,
            keyboard: Some(keyboard),
        }
    }
}
#[async_trait::async_trait]
pub trait Plugin {
    fn name(&self) -> &'static str;
    fn commands(&self) -> &'static [&'static str];
    async fn handle_message_async(&self, bot: &Bot, message: &Message, msg: &str) {}
    async fn handle_callback_async(
        &self,
        bot: &Bot,
        message: &Message,
        msg: &str,
        user_id: u64,
        callback_query: &teloxide::types::CallbackQuery,
    ) {
        self.handle_message_async(bot, message, msg).await;
    }
}
