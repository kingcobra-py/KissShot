use crate::config::get_config;
use crate::database::sql::fetch_user;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::utils::sk_utils::{
    build_session_from_validation, check_cc_with_sk, clear_session, extract_proxy, extract_sk,
    load_session, mask_sk, normalize_proxy, save_session, validate_sk,
};
use crate::plugins::helpers::*;
use chrono::Utc;
use regex::Regex;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("SKSessionPlugin"))
        })
    };
}

#[TeloxidePlugin(commands = [
    "setsk",
    "setsk@KissShotChkBot",
    "setproxy",
    "setproxy@KissShotChkBot",
    "skchk",
    "skchk@KissShotChkBot",
    "skstatus",
    "skstatus@KissShotChkBot",
    "clearsk",
    "clearsk@KissShotChkBot"
])]
pub struct SKSessionPlugin;

impl SKSessionPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        let command = message
            .text()
            .and_then(|t| t.split_whitespace().next())
            .unwrap_or("/setsk")
            .split('@')
            .next()
            .unwrap_or("/setsk")
            .to_ascii_lowercase();

        match command.as_str() {
            "/setsk" => self.set_sk(bot, message, msg).await,
            "/setproxy" => self.set_proxy(bot, message, msg).await,
            "/skchk" => self.sk_check_cc(bot, message, msg).await,
            "/skstatus" => self.sk_status(bot, message).await,
            "/clearsk" => self.clear_sk(bot, message).await,
            _ => self.set_sk(bot, message, msg).await,
        }
    }

    async fn guard_user(&self, bot: &Bot, message: &Message, sent_id: teloxide::types::MessageId) -> bool {
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        if !check_registration(user_id).await {
            let reply = format!(
                "<b>Access Denied ❌</b>\n\n\
                 <b>Reason:</b> You are not registered.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .ok();
            return false;
        }

        if check_banned(user_id).await {
            let reply = format!(
                "<b>Access Denied ❌</b>\n\n\
                 <b>Reason:</b> You are banned.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .ok();
            return false;
        }

        true
    }

    async fn set_sk(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;

        let sent = bot
            .send_message(message.chat.id, "<b>Validating SK...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !self.guard_user(bot, message, sent.id).await {
            return;
        }

        let sk = extract_sk(msg).or_else(|| {
            message
                .reply_to_message()
                .and_then(|r| r.text())
                .and_then(|t| extract_sk(t))
        });

        let sk = match sk {
            Some(s) => s,
            None => {
                let reply = format!(
                    "<b>Set SK Failed ❌</b>\n\n\
                     <b>Usage:</b> <code>/setsk sk_live_...</code>\n\
                     Or reply to a message containing an SK.\n\n\
                     <b>Flow:</b>\n\
                     1. <code>/setsk</code> — validate & save SK\n\
                     2. <code>/setproxy http://host:port</code> — set proxy\n\
                     3. <code>/skchk cc|mm|yy|cvv</code> — check cards\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let existing = load_session(user_id).await.ok().flatten();
        let proxy_ref = existing.as_ref().and_then(|s| s.proxy.as_deref());

        let validation = validate_sk(&sk, proxy_ref).await;

        if !validation.live {
            let reply = format!(
                "<b>Set SK Failed ❌</b>\n\n\
                 <b>SK:</b> <code>{}</code>\n\
                 <b>Reason:</b> {}\n\
                 <b>Proxy:</b> {}\n\
                 <b>Timestamp:</b> {}",
                mask_sk(&sk),
                validation.message,
                if proxy_ref.is_some() { "On" } else { "Off" },
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let session = build_session_from_validation(sk, existing.and_then(|s| s.proxy), &validation);

        if let Err(e) = save_session(user_id, &session).await {
            let _ = LOGGER.error(&format!("Failed to save SK session: {}", e)).await;
            let reply = format!(
                "<b>Set SK Failed ❌</b>\n\n\
                 <b>Reason:</b> {}\n\
                 <b>Timestamp:</b> {}",
                e, timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let reply = format!(
            "<b>SK Set Successfully ✅</b>\n\n\
             <b>SK:</b> <code>{}</code>\n\
             <b>Status:</b> Live ✅\n\
             <b>Balance:</b> {:.2} {}\n\
             <b>Account:</b> {}\n\
             <b>Proxy:</b> {}\n\n\
             <b>Next:</b> <code>/setproxy http://host:port</code> (optional)\n\
             <b>Then:</b> <code>/skchk cc|mm|yy|cvv</code>\n\
             <b>Timestamp:</b> {}",
            mask_sk(&session.sk),
            session.balance,
            session.currency,
            session.account_id.as_deref().unwrap_or("N/A"),
            if session.proxy.is_some() {
                "On"
            } else {
                "Off — use /setproxy"
            },
            timestamp
        );
        bot.edit_message_text(message.chat.id, sent.id, reply)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }

    async fn set_proxy(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;

        let sent = bot
            .send_message(message.chat.id, "<b>Setting proxy...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !self.guard_user(bot, message, sent.id).await {
            return;
        }

        let proxy = extract_proxy(msg).or_else(|| normalize_proxy(msg.trim()).ok());
        let proxy = match proxy {
            Some(p) => p,
            None => {
                let reply = format!(
                    "<b>Set Proxy Failed ❌</b>\n\n\
                     <b>Usage:</b>\n\
                     • <code>/setproxy host:port:user:pass</code>\n\
                     • <code>/setproxy http://user:pass@host:port</code>\n\
                     • <code>/setproxy socks5://user:pass@host:port</code>\n\n\
                     <b>Example:</b>\n\
                     <code>/setproxy c72fda....novada.pro:7777:user:pass</code>\n\n\
                     <b>Note:</b> Set your SK first with <code>/setsk</code>\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let mut session = match load_session(user_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                let reply = format!(
                    "<b>Set Proxy Failed ❌</b>\n\n\
                     <b>Reason:</b> No SK configured. Use <code>/setsk</code> first.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            Err(e) => {
                let reply = format!(
                    "<b>Set Proxy Failed ❌</b>\n\n\
                     <b>Reason:</b> {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let validation = validate_sk(&session.sk, Some(proxy.as_str())).await;
        if !validation.live {
            let reason = if validation.message.contains("Invalid proxy") {
                format!("Could not parse or use proxy.\n<b>Detail:</b> {}", validation.message)
            } else {
                format!(
                    "Proxy reachable but SK check failed.\n<b>Detail:</b> {}",
                    validation.message
                )
            };
            let reply = format!(
                "<b>Set Proxy Failed ❌</b>\n\n\
                 <b>Reason:</b> {}\n\
                 <b>Timestamp:</b> {}",
                reason, timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        session.proxy = Some(proxy.clone());
        session.validated_at = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
        session.balance = validation.balance;
        session.currency = validation.currency;

        if let Err(e) = save_session(user_id, &session).await {
            let reply = format!(
                "<b>Set Proxy Failed ❌</b>\n\n\
                 <b>Reason:</b> {}\n\
                 <b>Timestamp:</b> {}",
                e, timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let reply = format!(
            "<b>Proxy Set Successfully ✅</b>\n\n\
             <b>SK:</b> <code>{}</code>\n\
             <b>Proxy:</b> <code>{}</code>\n\
             <b>SK Status:</b> Live ✅ (re-validated via proxy)\n\
             <b>Balance:</b> {:.2} {}\n\n\
             <b>Ready:</b> <code>/skchk cc|mm|yy|cvv</code>\n\
             <b>Timestamp:</b> {}",
            mask_sk(&session.sk),
            mask_proxy(&proxy),
            session.balance,
            session.currency,
            timestamp
        );
        bot.edit_message_text(message.chat.id, sent.id, reply)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }

    async fn sk_check_cc(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;

        let sent = bot
            .send_message(message.chat.id, "<b>Checking card...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !self.guard_user(bot, message, sent.id).await {
            return;
        }

        let session = match load_session(user_id).await {
            Ok(Some(s)) if s.live => s,
            Ok(Some(_)) | Ok(None) => {
                let reply = format!(
                    "<b>SK Check Failed ❌</b>\n\n\
                     <b>Reason:</b> No valid SK configured.\n\
                     Use <code>/setsk sk_live_...</code> first.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            Err(e) => {
                let reply = format!(
                    "<b>SK Check Failed ❌</b>\n\n\
                     <b>Reason:</b> {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
        };

        let cc_regex = get_config()
            .map(|c| c.config.regex.cc_regex.clone())
            .unwrap_or_else(|_| "[0-9]{16}[|][0-9]{1,2}[|][0-9]{2,4}[|][0-9]{3}".to_string());

        let mut input = msg.to_string();
        if input.len() < 16 {
            if let Some(reply) = message.reply_to_message() {
                if let Some(text) = reply.text() {
                    input = text.to_string();
                }
            }
        }

        let cc_re = Regex::new(&cc_regex).unwrap();
        let cards: Vec<&str> = cc_re
            .find_iter(&input)
            .map(|m| m.as_str())
            .collect();

        if cards.is_empty() {
            let reply = format!(
                "<b>SK Check Failed ❌</b>\n\n\
                 <b>Reason:</b> No cards found.\n\
                 <b>Usage:</b> <code>/skchk 4111111111111111|12|26|123</code>\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let max_cards = get_config()
            .map(|c| c.config.limits.max_cc_chk as usize)
            .unwrap_or(5);

        if cards.len() > max_cards {
            let reply = format!(
                "<b>SK Check Failed ❌</b>\n\n\
                 <b>Reason:</b> Max {} cards per request.\n\
                 <b>Found:</b> {}\n\
                 <b>Timestamp:</b> {}",
                max_cards,
                cards.len(),
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let proxy_ref = session.proxy.as_deref();
        let start = std::time::Instant::now();
        let mut results = Vec::new();

        for (i, cc) in cards.iter().enumerate() {
            let card_number = cc.split('|').next().unwrap_or(cc);
            if !luhn_check(card_number) {
                results.push(format!(
                    "<code>{}</code>\n<b>Status:</b> Invalid ❌",
                    cc
                ));
                continue;
            }

            bot.edit_message_text(
                message.chat.id,
                sent.id,
                format!("<b>Checking card {}/{}...</b>", i + 1, cards.len()),
            )
            .parse_mode(ParseMode::Html)
            .await
            .ok();

            match check_cc_with_sk(&session.sk, proxy_ref, cc).await {
                Ok(result) => {
                    results.push(format!(
                        "<code>{}</code>\n<b>Status:</b> {}\n<b>Response:</b> {}",
                        cc, result.status, result.response
                    ));
                }
                Err(e) => {
                    results.push(format!(
                        "<code>{}</code>\n<b>Status:</b> Error ❌\n<b>Response:</b> {}",
                        cc, e
                    ));
                }
            }

            if i + 1 < cards.len() {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }

        let elapsed = start.elapsed().as_secs_f64();
        let first_name = message
            .from
            .as_ref()
            .map(|u| u.first_name.clone())
            .unwrap_or_else(|| "User".to_string());
        let role = match fetch_user(user_id).await {
            Ok(Some(u)) => u.status,
            _ => "Free".to_string(),
        };

        let reply = format!(
            "<b>SK CC Check Complete ✅</b>\n\n\
             <b>SK:</b> <code>{}</code>\n\
             <b>Proxy:</b> {}\n\
             <b>Checked:</b> {}\n\
             <b>Time:</b> {:.2}s\n\n\
             {}\n\n\
             <b>Requested By:</b> {} [{}]\n\
             <b>Timestamp:</b> {}",
            mask_sk(&session.sk),
            if proxy_ref.is_some() { "On" } else { "Off" },
            cards.len(),
            elapsed,
            results.join("\n\n"),
            first_name,
            role,
            timestamp
        );

        bot.edit_message_text(message.chat.id, sent.id, reply)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }

    async fn sk_status(&self, bot: &Bot, message: &Message) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;

        let sent = bot
            .send_message(message.chat.id, "<b>Loading session...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !self.guard_user(bot, message, sent.id).await {
            return;
        }

        match load_session(user_id).await {
            Ok(Some(session)) => {
                let reply = format!(
                    "<b>SK Session Status ℹ️</b>\n\n\
                     <b>SK:</b> <code>{}</code>\n\
                     <b>Status:</b> {}\n\
                     <b>Balance:</b> {:.2} {}\n\
                     <b>Account:</b> {}\n\
                     <b>Proxy:</b> {}\n\
                     <b>Validated:</b> {}\n\n\
                     <b>Commands:</b>\n\
                     • <code>/setsk</code> — replace SK\n\
                     • <code>/setproxy</code> — set proxy\n\
                     • <code>/skchk</code> — check cards\n\
                     • <code>/clearsk</code> — clear session\n\
                     <b>Timestamp:</b> {}",
                    mask_sk(&session.sk),
                    if session.live { "Live ✅" } else { "Invalid ❌" },
                    session.balance,
                    session.currency,
                    session.account_id.as_deref().unwrap_or("N/A"),
                    session
                        .proxy
                        .as_ref()
                        .map(|p| mask_proxy(p))
                        .unwrap_or_else(|| "Not set".to_string()),
                    session.validated_at,
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
            Ok(None) => {
                let reply = format!(
                    "<b>SK Session Status ℹ️</b>\n\n\
                     <b>SK:</b> Not configured\n\
                     <b>Proxy:</b> Not configured\n\n\
                     <b>Start with:</b> <code>/setsk sk_live_...</code>\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
            Err(e) => {
                let reply = format!(
                    "<b>SK Session Status ❌</b>\n\n\
                     <b>Reason:</b> {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
        }
    }

    async fn clear_sk(&self, bot: &Bot, message: &Message) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;

        let sent = bot
            .send_message(message.chat.id, "<b>Clearing session...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !self.guard_user(bot, message, sent.id).await {
            return;
        }

        match clear_session(user_id).await {
            Ok(_) => {
                let reply = format!(
                    "<b>Session Cleared ✅</b>\n\n\
                     SK and proxy removed from your session.\n\
                     Use <code>/setsk</code> to configure again.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
            Err(e) => {
                let reply = format!(
                    "<b>Clear Failed ❌</b>\n\n\
                     <b>Reason:</b> {}\n\
                     <b>Timestamp:</b> {}",
                    e, timestamp
                );
                bot.edit_message_text(message.chat.id, sent.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
            }
        }
    }
}

fn mask_proxy(proxy: &str) -> String {
    if proxy.contains('@') {
        if let Some(host_part) = proxy.split('@').nth(1) {
            return format!("***@{}", host_part);
        }
    }
    if proxy.len() > 20 {
        format!("{}...", &proxy[..20])
    } else {
        proxy.to_string()
    }
}
