use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use ccgen_rs::{Generate_Cards, GeneratorOptions};
use chrono::Utc;
use core::time;
use rand::Rng;
use std::time::Instant;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("CCGeneratorPlugin"))
        })
    };
}
#[TeloxidePlugin(commands = ["gen", "gen@KissShotChkBot", "gen_again:dynamic"])]
pub struct CCGeneratorPlugin;
impl CCGeneratorPlugin {
    async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        if msg.starts_with("gen_again:") {
            self.handle_callback(bot, message, msg).await;
        } else {
            self.cc_generator(bot, message, msg).await;
        }
    }
    async fn handle_callback(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let is_registered = check_registration(user_id).await;
        if !is_registered {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
                    <b>Reason:</b> You are not registered. Please register to generate cards.\n\
                    <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let banned = check_banned(user_id).await;
        if banned {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
                    <b>Reason:</b> You are banned from using this bot.\
                    <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        println!("🔄 handle_callback called with msg: {}", msg);
        let params: Vec<&str> = msg
            .strip_prefix("gen_again:")
            .expect("Operation failed")
            .split(':')
            .collect();
        println!("📊 Parsed params: {:?}", params);
        if params.len() >= 5 {
            let raw_bin = params[0];
            let raw_month = params[1];
            let raw_year = params[2];
            let raw_cvv = params[3];
            let amount = params[4];
            println!(
                "🔧 Extracted: bin={}, month={}, year={}, cvv={}, amount={}",
                raw_bin, raw_month, raw_year, raw_cvv, amount
            );
            self.cc_generator_edit(bot, message, raw_bin, raw_month, raw_year, raw_cvv, amount)
                .await;
        } else {
            println!("❌ Not enough parameters in callback data");
        }
    }
    async fn cc_generator_edit(
        &self,
        bot: &Bot,
        message: &Message,
        raw_bin: &str,
        raw_month: &str,
        raw_year: &str,
        raw_cvv: &str,
        amount: &str,
    ) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let is_registered = check_registration(user_id).await;
        if !is_registered {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
                    <b>Reason:</b> You are not registered. Please register to generate cards.\n\
                    <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let banned = check_banned(user_id).await;
        if banned {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
                    <b>Reason:</b> You are banned from using this bot.\
                    <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        if raw_bin.len() < 6 || raw_bin.len() > 16 {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> BIN must be 6-16 characters.\n\
<b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let bin_template = match normalize_bin(raw_bin) {
            Ok(t) => t,
            Err(e) => {
                let reply = format!(
                    "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> BIN Error: {}\n\
<b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, message.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };
        let month_template = normalize_month(raw_month).unwrap_or_default();
        let year_template = normalize_year(raw_year).unwrap_or_default();
        let cvv_template = normalize_cvv(raw_cvv).unwrap_or_default();
        let mut amount_value = amount.parse::<usize>().unwrap_or(10);
        amount_value = amount_value.clamp(1, 100);
        let mut bin_filled = bin_template.clone();
        while bin_filled.len() < 16 {
            bin_filled.push('x');
        }
        let mut cards = Vec::new();
        for _ in 0..amount_value {
            let month_value = if month_template.is_empty() || month_template.contains('x') {
                Some(rand::rng().random_range(1..=12))
            } else {
                Some(month_template.parse::<usize>().unwrap_or(1).clamp(1, 12))
            };
            let year_value = if year_template.is_empty() || year_template.contains('x') {
                Some(rand::rng().random_range(2025..=2050))
            } else {
                Some(
                    year_template
                        .parse::<usize>()
                        .unwrap_or(2025)
                        .clamp(2025, 2050),
                )
            };
            let cvv_value = if cvv_template.is_empty() || cvv_template.contains('x') {
                Some(rand::rng().random_range(100..=999))
            } else {
                Some(cvv_template.parse::<usize>().unwrap_or(123).clamp(0, 999))
            };
            let options = GeneratorOptions {
                bin_pattern: &bin_filled,
                month: month_value,
                year: year_value,
                cvv: cvv_value,
                amount: Some(1),
            };
            let mut generated = Generate_Cards(&options);
            cards.append(&mut generated);
        }
        if !cards.is_empty() {
            let formatted = cards
                .iter()
                .map(|c| format!("{}|{}|{}|{}", c.number, c.month, c.year, c.cvv))
                .collect::<Vec<_>>()
                .join("\n");
            let bin_lookup = lookup_bin(&raw_bin.replace("x", "")[..6]).await.unwrap();
            let bin_lookup_formatted = format!(
                "<b>Information:</b> {} - {} - {}\n\
            <b>Bank:</b> {}\n\
            <b>Country:</b> {} {}",
                bin_lookup.btype,
                bin_lookup.level,
                bin_lookup.vendor,
                bin_lookup.bank,
                bin_lookup.country,
                bin_lookup.flag
            );
            let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
            let user = handle_database_error!(
                bot,
                message,
                safe_fetch_user(user_id).await,
                "Database Operation"
            );
            let role = user
                .as_ref()
                .map(|u| u.status.clone())
                .unwrap_or_else(|| "FREE".to_string());
            let first_name = message
                .from
                .as_ref()
                .map(|user| user.first_name.clone())
                .unwrap_or_else(|| "User".to_string());
            let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
            let username = message
                .from
                .as_ref()
                .and_then(|u| u.username.clone())
                .unwrap_or_else(|| first_name.clone());
            let end_text = format!(
                "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
                <b>Timestamp:</b> {}",
                user_id,
                first_name,
                format!("{}{}", &role[..1].to_uppercase(), &role[1..].to_lowercase()),
                timestamp
            );
            let reply = format!(
                "<b>💳 Credit Card Generation Successful ✅</b>\n\n\
                    <b>Bank Identification Number:</b> {}\n\
                    {}\n\n\
                    <b>Generation Amount:</b> {}\n\n\
                    <code>{}</code>\n\n\
                    {}",
                bin_lookup.bin, bin_lookup_formatted, amount_value, formatted, end_text
            );
            let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                "🔄 Generate Again",
                format!(
                    "gen_again:{}:{}:{}:{}:{}",
                    raw_bin, raw_month, raw_year, raw_cvv, amount
                ),
            )]]);
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await
                .unwrap();
        } else {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> No card could be generated. Check your BIN and other fields.\n\
<b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, message.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
        }
    }
    async fn cc_generator(&self, bot: &Bot, message: &Message, msg: &str) {
        let sent_msg = bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        let text = if msg.len() > 8 {
            msg
        } else if let Some(reply) = message.reply_to_message() {
            if let Some(reply_text) = reply.text() {
                reply_text
            } else {
                msg
            }
        } else {
            msg
        };
        let mut parts: Vec<&str> = text.split('|').collect();
        if parts.len() <= 1 {
            parts = text.split_whitespace().collect();
        }
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let raw_bin = parts.get(0).copied().unwrap_or("").trim();
        let raw_month = parts.get(1).copied().unwrap_or("").trim();
        let raw_year = parts.get(2).copied().unwrap_or("").trim();
        let raw_cvv = parts.get(3).copied().unwrap_or("").trim();
        let raw_amount = if parts.len() > 1 {
            let last = parts.last().expect("Operation failed").trim();
            if last.chars().all(|c| c.is_ascii_digit()) && last != raw_bin {
                last
            } else {
                "10"
            }
        } else {
            "10"
        };
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        if raw_bin.len() < 6 || raw_bin.len() > 16 {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> BIN must be 6-16 characters.\n\
<b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let bin_template = match normalize_bin(raw_bin) {
            Ok(t) => t,
            Err(e) => {
                let reply = format!(
                    "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> BIN Error: {}\n\
<b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };
        let month_template = normalize_month(raw_month).unwrap_or_default();
        let year_template = normalize_year(raw_year).unwrap_or_default();
        let cvv_template = normalize_cvv(raw_cvv).unwrap_or_default();
        let mut amount_value = raw_amount.parse::<usize>().unwrap_or(10);
        amount_value = amount_value.clamp(1, 100);
        let mut bin_filled = bin_template.clone();
        while bin_filled.len() < 16 {
            bin_filled.push('x');
        }
        let mut cards = Vec::new();
        for _ in 0..amount_value {
            let month_value = if month_template.is_empty() || month_template.contains('x') {
                Some(rand::rng().random_range(1..=12))
            } else {
                Some(month_template.parse::<usize>().unwrap_or(1).clamp(1, 12))
            };
            let year_value = if year_template.is_empty() || year_template.contains('x') {
                Some(rand::rng().random_range(2025..=2050))
            } else {
                Some(
                    year_template
                        .parse::<usize>()
                        .unwrap_or(2025)
                        .clamp(2025, 2050),
                )
            };
            let cvv_value = if cvv_template.is_empty() || cvv_template.contains('x') {
                Some(rand::rng().random_range(100..=999))
            } else {
                Some(cvv_template.parse::<usize>().unwrap_or(123).clamp(0, 999))
            };
            let options = GeneratorOptions {
                bin_pattern: &bin_filled,
                month: month_value,
                year: year_value,
                cvv: cvv_value,
                amount: Some(1),
            };
            let mut generated = Generate_Cards(&options);
            cards.append(&mut generated);
        }
        if !cards.is_empty() {
            let formatted = cards
                .iter()
                .map(|c| format!("{}|{}|{}|{}", c.number, c.month, c.year, c.cvv))
                .collect::<Vec<_>>()
                .join("\n");
            let bin_lookup = lookup_bin(&raw_bin.replace("x", "")[..6]).await.unwrap();
            let bin_lookup_formatted = format!(
                "<b>Information:</b> {} - {} - {}\n\
            <b>Bank:</b> {}\n\
            <b>Country:</b> {} {}",
                bin_lookup.btype,
                bin_lookup.level,
                bin_lookup.vendor,
                bin_lookup.bank,
                bin_lookup.country,
                bin_lookup.flag
            );
            let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
            let user = handle_database_error!(
                bot,
                message,
                safe_fetch_user(user_id).await,
                "Database Operation"
            );
            let role = user
                .as_ref()
                .map(|u| u.status.clone())
                .unwrap_or_else(|| "user".to_string());
            let is_registered = check_registration(user_id).await;
            let first_name = message
                .from
                .as_ref()
                .map(|user| user.first_name.clone())
                .unwrap_or_else(|| "User".to_string());
            let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
            let username = message
                .from
                .as_ref()
                .and_then(|u| u.username.clone())
                .unwrap_or_else(|| first_name.clone());
            let end_text = format!(
                "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
                <b>Timestamp:</b> {}",
                user_id,
                first_name,
                format!("{}{}", &role[..1].to_uppercase(), &role[1..].to_lowercase()),
                timestamp
            );
            let reply = format!(
                "<b>💳 Credit Card Generation Successful ✅</b>\n\n\
                    <b>Bank Identification Number:</b> {}\n\
                    {}\n\n\
                    <b>Generation Amount:</b> {}\n\n\
                    <code>{}</code>\n\n\
                    {}",
                bin_lookup.bin, bin_lookup_formatted, amount_value, formatted, end_text
            );
            let keyboard = InlineKeyboardMarkup::new(vec![vec![InlineKeyboardButton::callback(
                "🔄 Generate Again",
                format!(
                    "gen_again:{}:{}:{}:{}:{}",
                    raw_bin, raw_month, raw_year, raw_cvv, amount_value
                ),
            )]]);
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .reply_markup(keyboard)
                .await
                .unwrap();
        } else {
            let reply = format!(
                "<b>Credit Card Generation Failed ❌</b>\n\
<b>Reason:</b> No card could be generated. Check your BIN and other fields.\n\
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
