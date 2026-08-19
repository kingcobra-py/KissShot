use crate::database::fetch_user;
use crate::database::sql::{self, User};
use crate::handle_database_error;
use crate::plugins::basic::keyboards::*;
use crate::safe_database_operation;
use chrono::Utc;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::prelude::Requester;
use teloxide::types::{
    InlineKeyboardButton, InlineKeyboardButtonKind, InlineKeyboardMarkup, Message, ParseMode,
};
use teloxide::Bot;
use teloxide_plugin::TeloxidePlugin;
struct SystemInfo {
    platform: String,
    architecture: String,
    hostname: String,
    kernel: String,
    load_average: String,
}
struct CpuInfo {
    cores: String,
    usage: f64,
    model: String,
    frequency: String,
}
struct MemoryInfo {
    total_gb: f64,
    used_gb: f64,
    available_gb: f64,
    free_gb: f64,
    buffers_gb: f64,
    cached_gb: f64,
    swap_total_gb: f64,
    swap_used_gb: f64,
    percent: f64,
    swap_percent: f64,
}
struct DiskInfo {
    total_gb: f64,
    used_gb: f64,
    free_gb: f64,
    percent: f64,
    read_mb_s: f64,
    write_mb_s: f64,
}
struct NetworkInfo {
    bytes_sent_mb: f64,
    bytes_recv_mb: f64,
    packets_sent: String,
    packets_recv: String,
    errors: String,
}
struct ProcessInfo {
    pid: u32,
    memory_percent: f64,
    cpu_percent: f64,
    threads: usize,
    virtual_memory_mb: f64,
    resident_memory_mb: f64,
}
struct UptimeInfo {
    boot_time: String,
    uptime: String,
    process_runtime: String,
}
#[TeloxidePlugin(callback_data = [
    "command_palette",
    "gateways",
    "utility",
    "dashboard",
    "personal_dashboard",
    "system_dashboard",
    "extensions",
    "back",
    "skmenu:dynamic",
    "folder:dynamic",
    "page:dynamic",
])]
pub struct KeyboardHandlerPlugin;
impl KeyboardHandlerPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, callback_data: &str) {
        println!(
            "🔍 KeyboardHandlerPlugin.handle called with callback_data: {}",
            callback_data
        );
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
        self.handle_with_user_id(bot, message, callback_data, user_id)
            .await;
    }
    pub async fn handle_with_user_id(
        &self,
        bot: &Bot,
        message: &Message,
        callback_data: &str,
        user_id: u64,
    ) {
        if callback_data.starts_with("page:") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            if parts.len() == 2 {
                Self::handle_extensions(bot, message, callback_data).await;
                return;
            }
        }
        if callback_data.starts_with("folder:") || callback_data.starts_with("page:") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            let category = if parts.len() >= 5 && parts[0] == "page" {
                parts[1]
            } else if parts.len() > 1 && (parts[1] == "utility" || parts[1] == "gateways") {
                parts[1]
            } else {
                "gateways"
            };
            Self::handle_folder(bot, message, callback_data, category).await;
            return;
        }
        if callback_data.starts_with("skmenu:") {
            Self::handle_sk_menu_hint(bot, message, callback_data).await;
            return;
        }
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
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
        match callback_data {
            "command_palette" => {
                let keyboard = get_command_palette_keyboard().await;
                bot.edit_message_text(message.chat.id, message.id, start_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await
                    .ok();
            }
            "dashboard" => {
                Self::show_dashboard_menu(bot, message).await;
            }
            "personal_dashboard" => {
                Self::handle_personal_dashboard(bot, message, user_id).await;
            }
            "system_dashboard" => {
                Self::handle_system_dashboard(bot, message).await;
            }
            "gateways" => {
                Self::handle_folder(bot, message, "page:0", "gateways").await;
            }
            "utility" => {
                Self::handle_folder(bot, message, "page:0", "utility").await;
            }
            "extensions" => {
                Self::handle_extensions(bot, message, "page:0").await;
            }
            "back" => {
                let keyboard = get_command_palette_keyboard().await;
                bot.edit_message_text(message.chat.id, message.id, start_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await
                    .ok();
            }
            _ => {}
        }
    }
    async fn handle_sk_menu_hint(bot: &Bot, message: &Message, callback_data: &str) {
        let cmd = callback_data.strip_prefix("skmenu:").unwrap_or("");
        let hint = match cmd {
            "setsk" => "<b>/setsk</b>\n\nValidate and save your Stripe SK.\n\n<b>Usage:</b>\n<code>/setsk sk_live_xxxxx</code>",
            "setproxy" => "<b>/setproxy</b>\n\nSet proxy for your SK session (requires SK set first).\n\n<b>Usage:</b>\n<code>/setproxy http://user:pass@host:port</code>",
            "skchk" => "<b>/skchk</b>\n\nCheck cards using your saved SK.\n\n<b>Usage:</b>\n<code>/skchk 4111111111111111|12|26|123</code>",
            "skstatus" => "<b>/skstatus</b>\n\nView your saved SK session, proxy, and balance.",
            "sk" => "<b>/sk</b>\n\nFull SK check (PM + balance + radar).\n\n<b>Usage:</b>\n<code>/sk sk_live_xxxxx</code>",
            "skbase" => "<b>/skbase</b>\n\nBase SK check — no PM, bypasses rate limit.\n\n<b>Usage:</b>\n<code>/skbase sk_live_xxxxx</code>",
            _ => "<b>Unknown SK command</b>",
        };
        let keyboard = InlineKeyboardMarkup::new(vec![vec![
            InlineKeyboardButton::new(
                "🔙 SK Session",
                InlineKeyboardButtonKind::CallbackData("folder:utility:SKSession".to_string()),
            ),
            InlineKeyboardButton::new(
                "🏠 Menu",
                InlineKeyboardButtonKind::CallbackData("command_palette".to_string()),
            ),
        ]]);
        bot.edit_message_text(message.chat.id, message.id, hint)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }
    async fn show_dashboard_menu(bot: &Bot, message: &Message) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
        let user_opt = handle_database_error!(
            bot,
            message,
            fetch_user(user_id as i64).await,
            "Database Operation"
        );
        let user = match user_opt {
            Some(u) => u,
            None => {
                let error_text = format!(
                    "❌ <b>User not found</b>\n\n\
    <b>User ID:</b> {}\n\n\
    You are not registered! Please",
                    user_id as i64
                );
                let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
                    "Back",
                    InlineKeyboardButtonKind::CallbackData("dashboard".to_string()),
                )]]);
                bot.edit_message_text(message.chat.id, message.id, error_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await
                    .ok();
                return;
            }
        };
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_name = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        let menu_text = format!(
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
        let keyboard = InlineKeyboardMarkup::new(vec![
            vec![
                InlineKeyboardButton::new(
                    "Personal Dashboard",
                    InlineKeyboardButtonKind::CallbackData("personal_dashboard".to_string()),
                ),
                InlineKeyboardButton::new(
                    "System Dashboard",
                    InlineKeyboardButtonKind::CallbackData("system_dashboard".to_string()),
                ),
            ],
            vec![InlineKeyboardButton::new(
                "Back",
                InlineKeyboardButtonKind::CallbackData("back".to_string()),
            )],
        ]);
        bot.edit_message_text(message.chat.id, message.id, menu_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }
    async fn handle_personal_dashboard(bot: &Bot, message: &Message, user_id: u64) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let user_name = message
            .from
            .as_ref()
            .map(|u| u.full_name())
            .unwrap_or_else(|| "Unknown".to_string());
        println!("Callback query user_id: {}", user_id);
        let user_opt = handle_database_error!(
            bot,
            message,
            fetch_user(user_id as i64).await,
            "Database Operation"
        );
        let user = match user_opt {
            Some(u) => u,
            None => {
                let error_text = format!(
                    "❌ <b>User not found</b>\n\n\
    <b>User ID:</b> {}\n\n\
    You are not registered! Please",
                    user_id as i64
                );
                let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
                    "Back",
                    InlineKeyboardButtonKind::CallbackData("dashboard".to_string()),
                )]]);
                bot.edit_message_text(message.chat.id, message.id, error_text)
                    .parse_mode(ParseMode::Html)
                    .reply_markup(keyboard)
                    .await
                    .ok();
                return;
            }
        };
        let start_text = format!(
            "<b>📊 Personal Dashboard</b>\n\n\
    <b><u>User ID</u></b>: {}\n\
    <b><u>Username</u></b>: {}\n\
    <b><u>Balance</u></b>: {}\n\n\
    <b><u>Status</u></b>: {}\n\
    <b><u>Antispam</u></b>: {}s\n\
    <b><u>Registered At</u></b>: {}\n\n\
    <b><u>Timestamp</u></b>: {}",
            user.user_id,
            user.username,
            user.balance,
            user.status,
            user.antispam,
            user.registered_at.format("%Y-%m-%d %H:%M:%S UTC"),
            timestamp
        );
        let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
            "Back",
            InlineKeyboardButtonKind::CallbackData("dashboard".to_string()),
        )]]);
        bot.edit_message_text(message.chat.id, message.id, start_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }
    async fn handle_folder(bot: &Bot, message: &Message, callback_data: &str, category: &str) {
        println!(
            "🌐 handle_folder called with: {} (category={})",
            callback_data, category
        );
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
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
        let base_path = match category {
            "utility" => "src/resources/utility",
            _ => "src/resources/gateways",
        };
        let mut folder_name = String::new();
        let mut page_index = 0;
        let mut data_page = 0;
        let parts: Vec<&str> = callback_data.split(':').collect();
        if callback_data.starts_with("folder:") {
            if parts.len() == 3 {
                folder_name = parts[2].to_string();
            }
        } else if callback_data.starts_with("page:") {
            match parts.len() {
                2 => page_index = parts[1].parse::<usize>().unwrap_or(0),
                3 => {
                    if parts[1] == "utility" || parts[1] == "gateways" {
                        page_index = parts[2].parse::<usize>().unwrap_or(0);
                    } else {
                        folder_name = parts[1].to_string();
                        page_index = parts[2].parse::<usize>().unwrap_or(0);
                    }
                }
                4 => {
                    folder_name = parts[1].to_string();
                    data_page = parts[2].parse::<usize>().unwrap_or(0);
                    page_index = parts[3].parse::<usize>().unwrap_or(0);
                }
                5 => {
                    let callback_category = parts[1];
                    folder_name = parts[2].to_string();
                    data_page = parts[3].parse::<usize>().unwrap_or(0);
                    page_index = parts[4].parse::<usize>().unwrap_or(0);
                    if callback_category == "utility" || callback_category == "gateways" {}
                }
                _ => {}
            }
        }
        let folder_path = if folder_name.is_empty() {
            base_path.to_string()
        } else {
            format!("{}/{}", base_path, folder_name)
        };
        if !Path::new(&folder_path).exists() {
            let error_text = if folder_name.is_empty() {
                format!(
                    "❌ <b>Category folder not found</b>\n\n\
                    <b>Expected path:</b> <code>{}</code>\n\n\
                    Please ensure the {} category folder exists.",
                    folder_path, category
                )
            } else {
                format!(
                    "❌ <b>Folder not found</b>\n\n\
                    <b>Category:</b> {}\n\
                    <b>Folder:</b> {}\n\
                    <b>Expected path:</b> <code>{}</code>\n\n\
                    Please ensure the folder exists in the {} category.",
                    category, folder_name, folder_path, category
                )
            };
            bot.edit_message_text(message.chat.id, message.id, error_text)
                .parse_mode(ParseMode::Html)
                .await
                .ok();
            return;
        }
        let data_file = format!("{}/data.txt", folder_path);
        println!(
            "🔍 Checking data file: {} (exists: {}, folder_name: '{}')",
            data_file,
            Path::new(&data_file).exists(),
            folder_name
        );
        if Path::new(&data_file).exists() && !folder_name.is_empty() {
            let content = match fs::read_to_string(&data_file) {
                Ok(content) => {
                    println!(
                        "🔍 Successfully read data file content ({} chars): {}",
                        content.len(),
                        content
                    );
                    content
                }
                Err(e) => {
                    let error_text = format!(
                        "❌ <b>Failed to read folder data</b>\n\n\
                        <b>Error:</b> <code>{}</code>\n\
                        <b>File:</b> <code>{}</code>\n\n\
                        Please check file permissions and try again.",
                        e, data_file
                    );
                    bot.edit_message_text(message.chat.id, message.id, error_text)
                        .parse_mode(ParseMode::Html)
                        .await
                        .ok();
                    return;
                }
            };
            let lines: Vec<&str> = content.lines().collect();
            let lines_per_page = 30;
            let total_pages = (lines.len() + lines_per_page - 1) / lines_per_page;
            let start = data_page * lines_per_page;
            let end = ((data_page + 1) * lines_per_page).min(lines.len());
            let page_content = lines[start..end].join("\n");
            let mut keyboard_rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
            let mut page_buttons = Vec::new();
            if data_page > 0 {
                page_buttons.push(InlineKeyboardButton::new(
                    "⬅️ Prev",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "page:{}:{}:{}:{}",
                        category,
                        folder_name,
                        data_page - 1,
                        page_index
                    )),
                ));
            }
            if data_page + 1 < total_pages {
                page_buttons.push(InlineKeyboardButton::new(
                    "➡️ Next",
                    InlineKeyboardButtonKind::CallbackData(format!(
                        "page:{}:{}:{}:{}",
                        category,
                        folder_name,
                        data_page + 1,
                        page_index
                    )),
                ));
            }
            if !page_buttons.is_empty() {
                keyboard_rows.push(page_buttons);
            }
            if folder_name == "SKSession" {
                keyboard_rows.push(vec![
                    InlineKeyboardButton::new(
                        "1️⃣ /setsk",
                        InlineKeyboardButtonKind::CallbackData("skmenu:setsk".to_string()),
                    ),
                    InlineKeyboardButton::new(
                        "2️⃣ /setproxy",
                        InlineKeyboardButtonKind::CallbackData("skmenu:setproxy".to_string()),
                    ),
                ]);
                keyboard_rows.push(vec![
                    InlineKeyboardButton::new(
                        "3️⃣ /skchk",
                        InlineKeyboardButtonKind::CallbackData("skmenu:skchk".to_string()),
                    ),
                    InlineKeyboardButton::new(
                        "📋 /skstatus",
                        InlineKeyboardButtonKind::CallbackData("skmenu:skstatus".to_string()),
                    ),
                ]);
                keyboard_rows.push(vec![
                    InlineKeyboardButton::new(
                        "🔍 /sk",
                        InlineKeyboardButtonKind::CallbackData("skmenu:sk".to_string()),
                    ),
                    InlineKeyboardButton::new(
                        "⚡ /skbase",
                        InlineKeyboardButtonKind::CallbackData("skmenu:skbase".to_string()),
                    ),
                ]);
            }
            keyboard_rows.push(vec![InlineKeyboardButton::new(
                "🔙 Back",
                InlineKeyboardButtonKind::CallbackData(category.to_string()),
            )]);
            let keyboard = InlineKeyboardMarkup::new(keyboard_rows);
            println!("🔍 Sending content to Telegram: {}", page_content);
            let result = bot
                .edit_message_text(message.chat.id, message.id, page_content)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await;
            match result {
                Ok(_) => println!("✅ Successfully sent content to Telegram"),
                Err(e) => println!("❌ Failed to send content to Telegram: {}", e),
            }
            return;
        }
        let entries: Vec<String> = fs::read_dir(&folder_path)
            .expect("Operation failed")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().into_string().expect("Operation failed"))
            .collect();
        let entries_per_page = 5;
        let total_pages = (entries.len() + entries_per_page - 1) / entries_per_page;
        let start = page_index * entries_per_page;
        let end = ((page_index + 1) * entries_per_page).min(entries.len());
        let mut keyboard_rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
        let mut row = Vec::new();
        for (i, entry) in entries[start..end].iter().enumerate() {
            row.push(InlineKeyboardButton::new(
                entry,
                InlineKeyboardButtonKind::CallbackData(format!("folder:{}:{}", category, entry)),
            ));
            if i % 2 == 1 {
                keyboard_rows.push(row);
                row = Vec::new();
            }
        }
        if !row.is_empty() {
            keyboard_rows.push(row);
        }
        let mut page_buttons = Vec::new();
        if page_index > 0 {
            page_buttons.push(InlineKeyboardButton::new(
                "⬅️ Prev",
                InlineKeyboardButtonKind::CallbackData(format!(
                    "page:{}:{}",
                    category,
                    page_index - 1
                )),
            ));
        }
        if page_index + 1 < total_pages {
            page_buttons.push(InlineKeyboardButton::new(
                "➡️ Next",
                InlineKeyboardButtonKind::CallbackData(format!(
                    "page:{}:{}",
                    category,
                    page_index + 1
                )),
            ));
        }
        if !page_buttons.is_empty() {
            keyboard_rows.push(page_buttons);
        }
        keyboard_rows.push(vec![InlineKeyboardButton::new(
            "🔙 Back",
            InlineKeyboardButtonKind::CallbackData("back".to_string()),
        )]);
        let keyboard = InlineKeyboardMarkup::new(keyboard_rows);
        let text = if folder_name.is_empty() {
            start_text
        } else {
            "<b>No data found in this folder.</b>".to_string()
        };
        bot.edit_message_text(message.chat.id, message.id, text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }
    async fn handle_extensions(bot: &Bot, message: &Message, callback_data: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
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
        let mut page_index = 0;
        if callback_data.starts_with("page:") {
            let parts: Vec<&str> = callback_data.split(':').collect();
            if parts.len() >= 2 {
                page_index = parts[1].parse::<usize>().unwrap_or(0);
            }
        }
        let data_file = "src/resources/extensions/data.txt";
        if !Path::new(data_file).exists() {
            let error_text = format!(
                "❌ <b>Extensions data file not found</b>\n\n\
                <b>Expected path:</b> <code>{}</code>\n\n\
                Please ensure the extensions data file exists.",
                data_file
            );
            bot.edit_message_text(message.chat.id, message.id, error_text)
                .parse_mode(ParseMode::Html)
                .await
                .ok();
            return;
        }
        let content = match fs::read_to_string(data_file) {
            Ok(content) => content,
            Err(e) => {
                let error_text = format!(
                    "❌ <b>Failed to read extensions data</b>\n\n\
                    <b>Error:</b> <code>{}</code>\n\
                    <b>File:</b> <code>{}</code>\n\n\
                    Please check file permissions and try again.",
                    e, data_file
                );
                bot.edit_message_text(message.chat.id, message.id, error_text)
                    .parse_mode(ParseMode::Html)
                    .await
                    .ok();
                return;
            }
        };
        let lines: Vec<&str> = content.lines().collect();
        let lines_per_page = 10;
        let total_pages = (lines.len() + lines_per_page - 1) / lines_per_page;
        let start = page_index * lines_per_page;
        let end = ((page_index + 1) * lines_per_page).min(lines.len());
        let page_content = if lines.is_empty() {
            "<b>No extensions available.</b>".to_string()
        } else {
            let mut display_text = format!(
                "<b>🔌 Extensions (Page {}/{})</b>\n\n",
                page_index + 1,
                total_pages
            );
            for (i, line) in lines[start..end].iter().enumerate() {
                let extension_num = start + i + 1;
                display_text.push_str(&format!("<b>{}</b>\n", line));
                if i < lines[start..end].len() - 1 {
                    display_text.push_str("\n");
                }
            }
            display_text
        };
        let mut keyboard_rows: Vec<Vec<InlineKeyboardButton>> = Vec::new();
        let mut page_buttons = Vec::new();
        if page_index > 0 {
            page_buttons.push(InlineKeyboardButton::new(
                "⬅️ Prev",
                InlineKeyboardButtonKind::CallbackData(format!("page:{}", page_index - 1)),
            ));
        }
        if page_index + 1 < total_pages {
            page_buttons.push(InlineKeyboardButton::new(
                "➡️ Next",
                InlineKeyboardButtonKind::CallbackData(format!("page:{}", page_index + 1)),
            ));
        }
        if !page_buttons.is_empty() {
            keyboard_rows.push(page_buttons);
        }
        keyboard_rows.push(vec![InlineKeyboardButton::new(
            "🔙 Back",
            InlineKeyboardButtonKind::CallbackData("back".to_string()),
        )]);
        let keyboard = InlineKeyboardMarkup::new(keyboard_rows);
        bot.edit_message_text(message.chat.id, message.id, page_content)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }
    async fn handle_system_dashboard(bot: &Bot, message: &Message) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
        let system_info = Self::get_system_info().await;
        let cpu_info = Self::get_cpu_info().await;
        let memory_info = Self::get_memory_info().await;
        let disk_info = Self::get_disk_info().await;
        let network_info = Self::get_network_info().await;
        let user_stats = Self::get_user_stats().await;
        let process_info = Self::get_process_info().await;
        let uptime_info = Self::get_uptime_info().await;
        let dashboard_text = format!(
            "<b>System Information</b>\n\
            <b><u>Platform:</u></b> <code>{}</code>\n\
            <b><u>Architecture:</u></b> <code>{}</code>\n\
            <b><u>Hostname:</u></b> <code>{}</code>\n\
            <b><u>Kernel:</u></b> <code>{}</code>\n\
            <b><u>Load Average:</u></b> <code>{}</code>\n\n\
            <b>CPU Information</b>\n\
            <b><u>CPU Cores:</u></b> <code>{}</code>\n\
            <b><u>CPU Usage:</u></b> <code>{}%</code>\n\
            <b><u>CPU Model:</u></b> <code>{}</code>\n\
            <b><u>CPU Frequency:</u></b> <code>{}</code>\n\n\
            <b>Memory Information</b>\n\
            <b><u>Total RAM:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Used RAM:</u></b> <code>{:.2} GB ({:.1}%)</code>\n\
            <b><u>Available RAM:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Free RAM:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Buffers:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Cached:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Swap Total:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Swap Used:</u></b> <code>{:.2} GB ({:.1}%)</code>\n\n\
            <b>Disk Information</b>\n\
            <b><u>Total Disk:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Used Disk:</u></b> <code>{:.2} GB ({:.1}%)</code>\n\
            <b><u>Free Disk:</u></b> <code>{:.2} GB</code>\n\
            <b><u>Disk I/O Read:</u></b> <code>{:.2} MB/s</code>\n\
            <b><u>Disk I/O Write:</u></b> <code>{:.2} MB/s</code>\n\n\
            <b>Network Information</b>\n\
            <b><u>Bytes Sent:</u></b> <code>{:.2} MB</code>\n\
            <b><u>Bytes Received:</u></b> <code>{:.2} MB</code>\n\
            <b><u>Packets Sent:</u></b> <code>{}</code>\n\
            <b><u>Packets Received:</u></b> <code>{}</code>\n\
            <b><u>Network Errors:</u></b> <code>{}</code>\n\n\
            <b><u>Timestamp:</u></b> <code>{}</code>",
            system_info.platform,
            system_info.architecture,
            system_info.hostname,
            system_info.kernel,
            system_info.load_average,
            cpu_info.cores,
            cpu_info.usage,
            cpu_info.model,
            cpu_info.frequency,
            memory_info.total_gb,
            memory_info.used_gb,
            memory_info.percent,
            memory_info.available_gb,
            memory_info.free_gb,
            memory_info.buffers_gb,
            memory_info.cached_gb,
            memory_info.swap_total_gb,
            memory_info.swap_used_gb,
            memory_info.swap_percent,
            disk_info.total_gb,
            disk_info.used_gb,
            disk_info.percent,
            disk_info.free_gb,
            disk_info.read_mb_s,
            disk_info.write_mb_s,
            network_info.bytes_sent_mb,
            network_info.bytes_recv_mb,
            network_info.packets_sent,
            network_info.packets_recv,
            network_info.errors,
            timestamp
        );
        let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::new(
            "Back",
            InlineKeyboardButtonKind::CallbackData("dashboard".to_string()),
        )]]);
        bot.edit_message_text(message.chat.id, message.id, dashboard_text)
            .parse_mode(ParseMode::Html)
            .reply_markup(keyboard)
            .await
            .ok();
    }

    async fn get_system_info() -> SystemInfo {
        let platform = std::env::consts::OS.to_string();
        let architecture = std::env::consts::ARCH.to_string();
        let hostname = hostname::get()
            .unwrap_or_else(|_| "unknown".into())
            .to_string_lossy()
            .to_string();
        let kernel_output = Command::new("uname").arg("-r").output();
        let kernel = match kernel_output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "Unknown".to_string(),
        };
        let load_output = Command::new("sh")
            .arg("-c")
            .arg("uptime | awk -F'load average:' '{print $2}' | awk '{print $1}'")
            .output();
        let load_average = match load_output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "Unknown".to_string(),
        };
        SystemInfo {
            platform,
            architecture,
            hostname,
            kernel,
            load_average,
        }
    }
    async fn get_cpu_info() -> CpuInfo {
        let cores = num_cpus::get();
        let usage = Self::get_cpu_usage().await;
        let model_output = Command::new("sh")
            .arg("-c")
            .arg("cat /proc/cpuinfo | grep 'model name' | head -1 | cut -d':' -f2 | xargs")
            .output();
        let model = match model_output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "Unknown".to_string(),
        };
        let freq_output = Command::new("sh")
            .arg("-c")
            .arg("cat /proc/cpuinfo | grep 'cpu MHz' | head -1 | cut -d':' -f2 | xargs")
            .output();
        let frequency = match freq_output {
            Ok(output) => {
                let freq_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !freq_str.is_empty() {
                    format!("{} MHz", freq_str)
                } else {
                    "Unknown".to_string()
                }
            }
            Err(_) => "Unknown".to_string(),
        };
        CpuInfo {
            cores: format!("{} Physical, {} Logical", cores, num_cpus::get_physical()),
            usage,
            model,
            frequency,
        }
    }
    async fn get_cpu_usage() -> f64 {
        let output = Command::new("sh")
            .arg("-c")
            .arg("grep 'cpu ' /proc/stat | awk '{usage=($2+$4)*100/($2+$3+$4+$5)} END {print usage}'")
            .output();
        match output {
            Ok(output) => {
                let usage_str = String::from_utf8_lossy(&output.stdout);
                usage_str.trim().parse().unwrap_or(0.0)
            }
            Err(_) => 0.0,
        }
    }
    async fn get_memory_info() -> MemoryInfo {
        let output = Command::new("sh")
            .arg("-c")
            .arg("free -m | grep -E '^Mem:|^Swap:' | awk '{print $2, $3, $4, $5, $6, $7}'")
            .output();
        match output {
            Ok(output) => {
                let info_str = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = info_str.trim().lines().collect();
                if lines.len() >= 2 {
                    let mem_parts: Vec<&str> = lines[0].split_whitespace().collect();
                    let swap_parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if mem_parts.len() >= 6 {
                        let total_mb: f64 = mem_parts[0].parse().unwrap_or(0.0);
                        let used_mb: f64 = mem_parts[1].parse().unwrap_or(0.0);
                        let free_mb: f64 = mem_parts[2].parse().unwrap_or(0.0);
                        let shared_mb: f64 = mem_parts[3].parse().unwrap_or(0.0);
                        let buffers_mb: f64 = mem_parts[4].parse().unwrap_or(0.0);
                        let cached_mb: f64 = mem_parts[5].parse().unwrap_or(0.0);
                        let total_gb = total_mb / 1024.0;
                        let used_gb = used_mb / 1024.0;
                        let free_gb = free_mb / 1024.0;
                        let available_gb = (free_mb + buffers_mb + cached_mb) / 1024.0;
                        let buffers_gb = buffers_mb / 1024.0;
                        let cached_gb = cached_mb / 1024.0;
                        let percent = (used_mb / total_mb) * 100.0;
                        let swap_total_mb: f64 = if swap_parts.len() >= 2 {
                            swap_parts[0].parse().unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        let swap_used_mb: f64 = if swap_parts.len() >= 2 {
                            swap_parts[1].parse().unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        let swap_total_gb = swap_total_mb / 1024.0;
                        let swap_used_gb = swap_used_mb / 1024.0;
                        let swap_percent = if swap_total_mb > 0.0 {
                            (swap_used_mb / swap_total_mb) * 100.0
                        } else {
                            0.0
                        };
                        MemoryInfo {
                            total_gb,
                            used_gb,
                            available_gb,
                            free_gb,
                            buffers_gb,
                            cached_gb,
                            swap_total_gb,
                            swap_used_gb,
                            percent,
                            swap_percent,
                        }
                    } else {
                        MemoryInfo {
                            total_gb: 0.0,
                            used_gb: 0.0,
                            available_gb: 0.0,
                            free_gb: 0.0,
                            buffers_gb: 0.0,
                            cached_gb: 0.0,
                            swap_total_gb: 0.0,
                            swap_used_gb: 0.0,
                            percent: 0.0,
                            swap_percent: 0.0,
                        }
                    }
                } else {
                    MemoryInfo {
                        total_gb: 0.0,
                        used_gb: 0.0,
                        available_gb: 0.0,
                        free_gb: 0.0,
                        buffers_gb: 0.0,
                        cached_gb: 0.0,
                        swap_total_gb: 0.0,
                        swap_used_gb: 0.0,
                        percent: 0.0,
                        swap_percent: 0.0,
                    }
                }
            }
            Err(_) => MemoryInfo {
                total_gb: 0.0,
                used_gb: 0.0,
                available_gb: 0.0,
                free_gb: 0.0,
                buffers_gb: 0.0,
                cached_gb: 0.0,
                swap_total_gb: 0.0,
                swap_used_gb: 0.0,
                percent: 0.0,
                swap_percent: 0.0,
            },
        }
    }
    async fn get_disk_info() -> DiskInfo {
        let output = Command::new("sh")
            .arg("-c")
            .arg("df -h / | tail -1 | awk '{print $2, $3, $4, $5}'")
            .output();
        match output {
            Ok(output) => {
                let info_str = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = info_str.trim().split_whitespace().collect();
                if parts.len() >= 4 {
                    let total_str = parts[0].replace("G", "").replace("T", "000");
                    let used_str = parts[1].replace("G", "").replace("T", "000");
                    let free_str = parts[2].replace("G", "").replace("T", "000");
                    let percent_str = parts[3].replace("%", "");
                    let total_gb: f64 = total_str.parse().unwrap_or(0.0);
                    let used_gb: f64 = used_str.parse().unwrap_or(0.0);
                    let free_gb: f64 = free_str.parse().unwrap_or(0.0);
                    let percent: f64 = percent_str.parse().unwrap_or(0.0);
                    let io_output = Command::new("sh")
                        .arg("-c")
                        .arg("iostat -x 1 1 | grep -E '^[a-z]' | head -1 | awk '{print $4, $5}'")
                        .output();
                    let (read_mb_s, write_mb_s) = match io_output {
                        Ok(io_output) => {
                            let io_str = String::from_utf8_lossy(&io_output.stdout);
                            let io_parts: Vec<&str> = io_str.trim().split_whitespace().collect();
                            if io_parts.len() >= 2 {
                                let read: f64 = io_parts[0].parse().unwrap_or(0.0);
                                let write: f64 = io_parts[1].parse().unwrap_or(0.0);
                                (read, write)
                            } else {
                                (0.0, 0.0)
                            }
                        }
                        Err(_) => (0.0, 0.0),
                    };
                    DiskInfo {
                        total_gb,
                        used_gb,
                        free_gb,
                        percent,
                        read_mb_s,
                        write_mb_s,
                    }
                } else {
                    DiskInfo {
                        total_gb: 0.0,
                        used_gb: 0.0,
                        free_gb: 0.0,
                        percent: 0.0,
                        read_mb_s: 0.0,
                        write_mb_s: 0.0,
                    }
                }
            }
            Err(_) => DiskInfo {
                total_gb: 0.0,
                used_gb: 0.0,
                free_gb: 0.0,
                percent: 0.0,
                read_mb_s: 0.0,
                write_mb_s: 0.0,
            },
        }
    }
    async fn get_network_info() -> NetworkInfo {
        let output = Command::new("sh")
            .arg("-c")
            .arg("cat /proc/net/dev | grep -E 'eth0|wlan0|enp' | head -1 | awk '{print $2, $3, $10, $11, $4, $12}'")
            .output();
        match output {
            Ok(output) => {
                let info_str = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = info_str.trim().split_whitespace().collect();
                if parts.len() >= 6 {
                    let bytes_recv: f64 = parts[0].parse().unwrap_or(0.0);
                    let packets_recv: String = parts[1].to_string();
                    let bytes_sent: f64 = parts[2].parse().unwrap_or(0.0);
                    let packets_sent: String = parts[3].to_string();
                    let err_in: f64 = parts[4].parse().unwrap_or(0.0);
                    let err_out: f64 = parts[5].parse().unwrap_or(0.0);
                    NetworkInfo {
                        bytes_sent_mb: bytes_sent / (1024.0 * 1024.0),
                        bytes_recv_mb: bytes_recv / (1024.0 * 1024.0),
                        packets_sent,
                        packets_recv,
                        errors: format!("{}", (err_in + err_out) as i64),
                    }
                } else {
                    NetworkInfo {
                        bytes_sent_mb: 0.0,
                        bytes_recv_mb: 0.0,
                        packets_sent: "0".to_string(),
                        packets_recv: "0".to_string(),
                        errors: "0".to_string(),
                    }
                }
            }
            Err(_) => NetworkInfo {
                bytes_sent_mb: 0.0,
                bytes_recv_mb: 0.0,
                packets_sent: "0".to_string(),
                packets_recv: "0".to_string(),
                errors: "0".to_string(),
            },
        }
    }
    async fn get_user_stats() -> sql::UserStats {
        sql::get_user_stats()
            .await
            .unwrap_or_else(|_| sql::UserStats {
                total_users: 0,
                free_users: 0,
                banned_users: 0,
                total_balance: 0,
                avg_balance: 0.0,
            })
    }
    async fn get_process_info() -> ProcessInfo {
        let pid = std::process::id();
        let output = Command::new("sh")
            .arg("-c")
            .arg(&format!(
                "ps -p {} -o pid,pcpu,pmem,nlwp,vsz,rss --no-headers",
                pid
            ))
            .output();
        match output {
            Ok(output) => {
                let info_str = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = info_str.trim().split_whitespace().collect();
                if parts.len() >= 6 {
                    let cpu_percent: f64 = parts[1].parse().unwrap_or(0.0);
                    let memory_percent: f64 = parts[2].parse().unwrap_or(0.0);
                    let threads: usize = parts[3].parse().unwrap_or(1);
                    let virtual_memory_kb: f64 = parts[4].parse().unwrap_or(0.0);
                    let resident_memory_kb: f64 = parts[5].parse().unwrap_or(0.0);
                    ProcessInfo {
                        pid,
                        memory_percent,
                        cpu_percent,
                        threads,
                        virtual_memory_mb: virtual_memory_kb / 1024.0,
                        resident_memory_mb: resident_memory_kb / 1024.0,
                    }
                } else {
                    ProcessInfo {
                        pid,
                        memory_percent: 0.0,
                        cpu_percent: 0.0,
                        threads: 1,
                        virtual_memory_mb: 0.0,
                        resident_memory_mb: 0.0,
                    }
                }
            }
            Err(_) => ProcessInfo {
                pid,
                memory_percent: 0.0,
                cpu_percent: 0.0,
                threads: 1,
                virtual_memory_mb: 0.0,
                resident_memory_mb: 0.0,
            },
        }
    }
    async fn get_uptime_info() -> UptimeInfo {
        let boot_time_output = Command::new("sh").arg("-c").arg("uptime -s").output();
        let uptime_output = Command::new("sh").arg("-c").arg("uptime -p").output();
        let boot_time = match boot_time_output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "Unknown".to_string(),
        };
        let uptime = match uptime_output {
            Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
            Err(_) => "Unknown".to_string(),
        };

        let process_runtime = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(now) => {
                let start_time = now.as_secs() - 3600;
                let runtime_secs = now.as_secs() - start_time;
                let hours = runtime_secs / 3600;
                let minutes = (runtime_secs % 3600) / 60;
                let seconds = runtime_secs % 60;
                format!("{}h {}m {}s", hours, minutes, seconds)
            }
            Err(_) => "Unknown".to_string(),
        };
        UptimeInfo {
            boot_time,
            uptime,
            process_runtime,
        }
    }
}
