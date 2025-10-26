use crate::config::get_config;
use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::utils::bin_lookup::lookup_bin;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Serialize};
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::sugar::request::RequestLinkPreviewExt;
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("VBVPlugin"))
        })
    };
}

#[TeloxidePlugin(commands = ["vbv", "vbv@KissShotChkBot"])]
pub struct VBVPlugin;

impl VBVPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let chat_id = message.chat.id.0 as i64;

        let mut input_text = msg.to_string();
        if msg.len() < 16 {
            if let Some(reply) = message.reply_to_message().as_ref() {
                if let Some(reply_text) = reply.text() {
                    input_text = reply_text.to_string();
                }
            }
        }

        let credit_cards = Regex::new(
            get_config()
                .expect("Operation failed")
                .config
                .regex
                .cc_regex
                .as_str(),
        )
        .unwrap();

        let credit_cards_list: Vec<&str> = match credit_cards.find(input_text.as_str()) {
            Some(m) => vec![m.as_str()],
            None => Vec::new(),
        };

        if credit_cards_list.is_empty() {
            let reply = "<b>No credit cards found ❌</b>";
            bot.send_message(message.chat.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let sent_msg = bot
            .send_message(message.chat.id, "<b>Processing...</b>")
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await
            .unwrap();

        if !check_registration(user_id).await {
            let access_denied_reason = "<b>Access denied ❌</b>\n\n<b>Reason:</b> You are not registered. Please use: /register";
            bot.edit_message_text(message.chat.id, sent_msg.id, access_denied_reason)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        } else if check_banned(user_id).await {
            let access_denied_reason =
                "<b>Access denied ❌</b>\n\n<b>Reason:</b> You are banned from using this bot.";
            bot.edit_message_text(message.chat.id, sent_msg.id, access_denied_reason)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        } else if !check_access(chat_id).await {
            let access_denied_reason = "<b>Access denied ❌</b>\n\n<b>Reason:</b> You are not allowed to access this gateway here. Consider using it at: @heckervaultchat";
            bot.edit_message_text(message.chat.id, sent_msg.id, access_denied_reason)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let cooldown_remaining = get_antispam(user_id).await;
        if cooldown_remaining > 0 {
            let reply = format!("<b>Rate limited ⏱️</b>\n\n<b>Please wait {} seconds before using this command again.</b>", cooldown_remaining);
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let user_cooldown = get_user_cooldown(user_id).await;
        if user_cooldown == 0 {
            if let Err(e) = set_user_antispam(user_id, 10).await {
                // Log error but don't fail the request
                println!("Failed to set antispam for user {}: {}", user_id, e);
            }
        }

        let mut results = Vec::new();
        let mut vbv_responses: Vec<Option<(String, String, String)>> = Vec::new();
        let start_time = std::time::Instant::now();

        for cc in credit_cards_list.iter() {
            let card_number = cc.split('|').next().unwrap_or(cc);

            if !luhn_check(card_number) {
                results.push(format!(
                    "<code>{}</code>\n<b>Status:</b> Invalid",
                    cc.replace(" | ", "|")
                ));
                vbv_responses.push(None);
            } else {
                match lookup_vbv(cc).await {
                    Ok((bin, response, enrolled)) => {
                        let status = if response.contains("Authenticate Successful")
                            || response.contains("Authenticate Attempt Successful")
                        {
                            "Approved ✅"
                        } else {
                            "Rejected ❌"
                        };
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> {}",
                            cc.replace(" | ", "|"),
                            status
                        ));
                        vbv_responses.push(Some((bin, response, enrolled)));
                    }
                    Err(_) => {
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> Error",
                            cc.replace(" | ", "|")
                        ));
                        vbv_responses.push(None);
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }

        let total_time = start_time.elapsed().as_secs_f64();
        let final_text = if credit_cards_list.len() == 1 {
            // Gather requester info
            let (first_name, profile_url) = if let Some(u) = message.from.as_ref() {
                let fname = u.first_name.clone();
                let url = if let Some(username) = u.username.clone() {
                    format!("https://t.me/{}", username)
                } else {
                    format!("tg://user?id={}", u.id.0)
                };
                (fname, url)
            } else {
                ("User".to_string(), "#".to_string())
            };

            // Determine role
            let role = match fetch_user(user_id).await {
                Ok(Some(u)) => match u.status.as_str() {
                    "ADMIN" => "Admin".to_string(),
                    "PREMIUM" => "Premium".to_string(),
                    _ => "Free".to_string(),
                },
                _ => "Free".to_string(),
            };

            // Proxy status
            let proxy_live = get_config()
                .expect("Operation failed")
                .config
                .proxy
                .proxy
                .is_some();
            let proxy_str = if proxy_live { "Live" } else { "Off" };

            let status = if results[0].contains("Approved") {
                "Passed ✅"
            } else {
                "Rejected ❌"
            };
            // Use raw VBV response for the "Response:" line (do not change the top status)
            let response_text = if let Some(Some((_, resp, _))) = vbv_responses.get(0) {
                if !resp.is_empty() {
                    // Remove emojis from response but keep the text
                    resp.replace("✅", "").replace("❌", "").trim().to_string()
                } else if results[0].contains("Approved") {
                    "Passed".to_string()
                } else {
                    "Rejected".to_string()
                }
            } else {
                if results[0].contains("Approved") {
                    "Passed".to_string()
                } else {
                    "Rejected".to_string()
                }
            };

            let bin_info = match lookup_bin(&credit_cards_list[0][..6]).await {
                Ok(bin_data) => format!(
                    "\n\n<b>Info</b>: {} - {} - {}\n<b>Issuer</b>: {}\n<b>Country</b>: {} {}\n\n<b>Requested</b>: <a href=\"{}\">{}</a> [{}]\n<b>Proxy</b>: {}  <b>Retry</b>: 0\n<b>Time</b>: {:.2} seconds",
                    bin_data.vendor, bin_data.btype, bin_data.level, bin_data.bank, bin_data.country, bin_data.flag, profile_url, first_name, role, proxy_str, total_time
                ),
                Err(_) => format!(
                    "\n\n<b>Requested</b>: <a href=\"{}\">{}</a> [{}]\n<b>Proxy</b>: {} | <b>Retry</b>: 0\n<b>Time</b>: {:.2} seconds",
                    profile_url, first_name, role, proxy_str, total_time
                )
            };

            format!(
                "<b>{}</b>\n\n<b>Card</b>: <code>{}</code>\n<b>Gateway</b>: VBV Lookup\n<b>Response</b>: {}{}",
                status,
                credit_cards_list[0].replace(" | ", "|"),
                response_text,
                bin_info
            )
        } else {
            format!(
                "<b>Total:</b> <code>{}</code> | <b>Processed:</b> <code>{}</code>\n\n{}\n\n<b>Time:</b> {:.2}s",
                credit_cards_list.len(),
                results.len(),
                results.join("\n\n"),
                total_time
            )
        };

        bot.edit_message_text(message.chat.id, sent_msg.id, final_text)
            .parse_mode(ParseMode::Html)
            .disable_link_preview(true)
            .await
            .unwrap();
    }
}

#[derive(Debug, Deserialize)]
pub struct VBVLookup {
    #[serde(rename = "BIN")]
    pub bin: Option<String>,
    #[serde(rename = "Response")]
    pub response: Option<String>,
    #[serde(rename = "Enrolled")]
    pub enrolled: Option<String>,
}
pub async fn lookup_vbv(cc: &str) -> Result<(String, String, String), String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://ayanvbv.alwaysdata.net/api/vbv.php?bin={}",
            &cc[..6]
        ))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    let vbv_lookup: VBVLookup = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok((
        vbv_lookup.bin.unwrap_or_default(),
        vbv_lookup.response.unwrap_or_default(),
        vbv_lookup.enrolled.unwrap_or_default(),
    ))
}
