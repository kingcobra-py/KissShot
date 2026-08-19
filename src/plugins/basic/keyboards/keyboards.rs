use reqwest::Url;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup};
pub async fn get_start_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
        "⚡ Command Palette",
        InlineKeyboardButtonKind::CallbackData("command_palette".to_string()),
    )]])
}
pub async fn get_command_palette_keyboard() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::new(
                "🌐 Gateways",
                InlineKeyboardButtonKind::CallbackData("gateways".to_string()),
            ),
            InlineKeyboardButton::new(
                "🛠️ Utility",
                InlineKeyboardButtonKind::CallbackData("utility".to_string()),
            ),
        ],
        vec![
            InlineKeyboardButton::new(
                "📊 Dashboard",
                InlineKeyboardButtonKind::CallbackData("dashboard".to_string()),
            ),
            InlineKeyboardButton::new(
                "🔌 Extensions",
                InlineKeyboardButtonKind::CallbackData("extensions".to_string()),
            ),
        ],
        vec![InlineKeyboardButton::new(
            "🔑 SK Session",
            InlineKeyboardButtonKind::CallbackData("folder:utility:SKSession".to_string()),
        )],
        vec![
            InlineKeyboardButton::new(
                "📢 Channel",
                InlineKeyboardButtonKind::Url(
                    Url::parse("https://t.me/heckervault").expect("Operation failed"),
                ),
            ),
            InlineKeyboardButton::new(
                "💬 Group",
                InlineKeyboardButtonKind::Url(
                    Url::parse("https://t.me/heckervaultchat").expect("Operation failed"),
                ),
            ),
        ],
        vec![InlineKeyboardButton::new(
            "🔙 Back",
            InlineKeyboardButtonKind::CallbackData("back".to_string()),
        )],
    ])
}
pub async fn get_back_keyboard2() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
        "🔙 Back",
        InlineKeyboardButtonKind::CallbackData("back".to_string()),
    )]])
}
