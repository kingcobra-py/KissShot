use crate::config::get_config;
use crate::database::sql::{self, fetch_user};
use crate::handle_database_error;
use crate::logging::{get_logger, LoggerHandle};
use crate::plugin_handler::*;
use crate::plugins::helpers::*;
use crate::safe_database_operation;
use chrono::Utc;
use grammers_client::grammers_tl_types as tl;
use grammers_client::session::Session;
use grammers_client::{Client, Config};
use regex::Regex;
use std::collections::HashSet;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use teloxide::payloads::{EditMessageTextSetters, SendDocumentSetters, SendMessageSetters};
use teloxide::types::{InputFile, Message, ParseMode};
use teloxide::{prelude::Requester, Bot};
use teloxide_plugin::TeloxidePlugin;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::task;
lazy_static::lazy_static! {
    static ref LOGGER: std::sync::Arc<LoggerHandle> = {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                get_logger("ScraperPlugin").await
            })
        })
    };
    static ref SCRAPE_QUEUE: AtomicI32 = AtomicI32::new(0);
    static ref USER_CLIENT: Arc<Mutex<Option<Client>>> = Arc::new(Mutex::new(None));
}
#[TeloxidePlugin(commands = ["scr", "ccscr", "scr@KissShotChkBot", "ccscr@KissShotChkBot"])]
pub struct ScraperPlugin;
impl ScraperPlugin {
    pub async fn handle(&self, bot: &Bot, message: &Message, msg: &str) {
        let bot_clone = bot.clone();
        let message_clone = message.clone();
        let msg_clone = msg.to_string();

        let bot_timeout = bot.clone();
        let message_timeout = message.clone();

        let task_handle = task::spawn_blocking(move || {
            futures::executor::block_on(Self::cc_scraper(&bot_clone, &message_clone, &msg_clone))
        });

        tokio::select! {
            result = task_handle => {
                if let Err(e) = result {
                    LOGGER.error(&format!("❌ Scraping task failed: {}", e)).await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> Task execution failed.\n\
                         <b>Timestamp:</b> {}",
                        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    );
                    let _ = bot_timeout.edit_message_text(message_timeout.chat.id, message_timeout.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await;
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                LOGGER.error("❌ Scraping task timed out after 1 minute").await;
                let reply = format!(
                    "<b>Credit Card Scraping Failed ❌</b>\n\n\
                     <b>Reason:</b> Task timed out (1 minute).\n\
                     <b>Timestamp:</b> {}",
                    Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                );
                let _ = bot_timeout.edit_message_text(message_timeout.chat.id, message_timeout.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await;
            }
        }
    }
    pub async fn cc_scraper(bot: &Bot, message: &Message, msg: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();
        let user_id = message.from.as_ref().map(|u| u.id.0).unwrap_or(0) as i64;
        let first_name = message
            .from
            .as_ref()
            .map(|user| user.first_name.clone())
            .unwrap_or_else(|| "User".to_string());
        let sent_msg = bot
            .send_message(message.chat.id, "<b>Please wait...</b>")
            .parse_mode(ParseMode::Html)
            .await
            .unwrap();

        if !check_registration(user_id).await {
            let reply = format!(
                "<b>Credit Card Scraping Failed ❌</b>\n\n\
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
                "<b>Credit Card Scraping Failed ❌</b>\n\n\
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

        let cooldown_remaining = get_antispam(user_id).await;
        if cooldown_remaining > 0 {
            let reply = format!(
                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                 <b>Reason:</b> Please wait {} seconds before using this command again.\n\
                 <b>Timestamp:</b> {}",
                cooldown_remaining, timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let user_cooldown = get_user_cooldown(user_id).await;
        if user_cooldown == 0 {
            if let Err(e) = set_user_antispam(user_id, 30).await {
                LOGGER
                    .error(&format!(
                        "Failed to set antispam for user {}: {}",
                        user_id, e
                    ))
                    .await;
            }
        }

        let command_parts: Vec<&str> = msg.split_whitespace().collect();
        if command_parts.len() <= 1 {
            let reply = format!(
                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                 <b>Reason:</b> You need to specify a chat_id to scrape cards.\n\
                 <b>Example:</b> <code>/scr fpsv2 100</code>\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }
        let command_parts: Vec<&str> = msg.split_whitespace().collect();
        let chat_id = command_parts[0].to_string();
        let limit = command_parts[1].parse::<i32>().unwrap();
        println!("Limit: {}", limit);
        let bin_filter = command_parts.get(2).map(|s| s.to_string());
        let user = handle_database_error!(
            bot,
            message,
            crate::plugins::helpers::database::safe_fetch_user(user_id).await,
            "Credit Card Scraping"
        );
        let max_limit = if let Some(user) = user {
            if user.status == "ADMIN" || user.status == "PREMIUM" {
                5001
            } else {
                1001
            }
        } else {
            1001
        };
        if limit > max_limit {
            let reply = format!(
                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                 <b>Reason:</b> Scrape limit exceeded! Maximum: <code>{}</code>\n\
                 <b>Timestamp:</b> {}",
                max_limit, timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let queue_position = SCRAPE_QUEUE.fetch_add(1, Ordering::SeqCst) + 1;
        bot.edit_message_text(
            message.chat.id,
            sent_msg.id,
            format!("<b>Credit Card Scraping Waiting...</b>\n\n<b>Position in queue:</b> {}\n<b>Timestamp:</b> {}", queue_position, timestamp),
        )
        .parse_mode(ParseMode::Html)
        .await
        .unwrap();

        while SCRAPE_QUEUE.load(Ordering::SeqCst) != 1 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        print!("Fuck at que");
        let start_time = std::time::Instant::now();

        let client = {
            let mut client_guard = USER_CLIENT.lock().await;
            if let Some(ref client) = *client_guard {
                client.clone()
            } else {
                let config = match get_config() {
                    Ok(config) => config,
                    Err(e) => {
                        LOGGER
                            .error(&format!("❌ Failed to get config: {}", e))
                            .await;
                        let reply = format!(
                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Failed to load configuration.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        );
                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                            .parse_mode(ParseMode::Html)
                            .await
                            .unwrap();
                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                };
                let telegram_config = &config.telegram.user;
                let session = match Session::load_file_or_create("user_session.session") {
                    Ok(session) => session,
                    Err(e) => {
                        LOGGER
                            .error(&format!("❌ Failed to load session: {}", e))
                            .await;
                        let reply = format!(
                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Failed to load user session.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        );
                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                            .parse_mode(ParseMode::Html)
                            .await
                            .unwrap();
                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                };
                let client = match Client::connect(Config {
                    session,
                    api_id: telegram_config.api_id as i32,
                    api_hash: telegram_config.api_hash.clone(),
                    params: Default::default(),
                })
                .await
                {
                    Ok(client) => client,
                    Err(e) => {
                        LOGGER
                            .error(&format!("❌ Failed to connect client: {}", e))
                            .await;
                        let reply = format!(
                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Failed to connect to Telegram.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        );
                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                            .parse_mode(ParseMode::Html)
                            .await
                            .unwrap();
                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                };
                if !client.is_authorized().await.unwrap_or(false) {
                    println!("Input Phone Number: ");
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap();
                    let phone = input.trim().to_string();
                    let login_token = match client.request_login_code(&phone.trim()).await {
                        Ok(token) => token,
                        Err(e) => {
                            LOGGER
                                .error(&format!("❌ Failed to request login code: {}", e))
                                .await;
                            let reply = format!(
                                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                 <b>Reason:</b> Failed to request login code.\n\
                                 <b>Timestamp:</b> {}",
                                timestamp
                            );
                            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                .parse_mode(ParseMode::Html)
                                .await
                                .unwrap();
                            SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                            return;
                        }
                    };
                    println!("✉️ Enter the code you received:");
                    input.clear();
                    std::io::stdin().read_line(&mut input).unwrap();
                    let code = input.trim();
                    match client.sign_in(&login_token, code).await {
                        Ok(_) => {
                            println!("🎉 Successfully signed in!");
                        }
                        Err(grammers_client::SignInError::PasswordRequired(password_token)) => {
                            println!("🔑 This account has 2FA enabled. Enter your password:");
                            input.clear();
                            std::io::stdin().read_line(&mut input).unwrap();
                            let password = input.trim();
                            if let Err(e) = client.check_password(password_token, password).await {
                                LOGGER
                                    .error(&format!("❌ Password auth failed: {}", e))
                                    .await;
                                let reply = format!(
                                    "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                     <b>Reason:</b> Password authentication failed.\n\
                                     <b>Timestamp:</b> {}",
                                    timestamp
                                );
                                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                    .parse_mode(ParseMode::Html)
                                    .await
                                    .unwrap();
                                SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                                return;
                            }
                            println!("🎉 Successfully signed in with 2FA!");
                        }
                        Err(e) => {
                            LOGGER.error(&format!("❌ Sign-in failed: {}", e)).await;
                            let reply = format!(
                                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                 <b>Reason:</b> Sign-in failed.\n\
                                 <b>Timestamp:</b> {}",
                                timestamp
                            );
                            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                .parse_mode(ParseMode::Html)
                                .await
                                .unwrap();
                            SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                            return;
                        }
                    }
                }
                if let Err(e) = client.session().save_to_file("user_session.session") {
                    LOGGER
                        .error(&format!("❌ Failed to save session: {}", e))
                        .await;
                }

                *client_guard = Some(client.clone());
                client
            }
        };

        let chat_title = {
            if let Ok(_) = chat_id.parse::<i64>() {
                format!("Chat: {}", chat_id)
            } else {
                match client
                    .invoke(&tl::functions::contacts::ResolveUsername {
                        username: chat_id.to_string(),
                        referer: None,
                    })
                    .await
                {
                    Ok(resolved) => match resolved {
                        tl::enums::contacts::ResolvedPeer::Peer(peer) => match peer.peer {
                            tl::enums::Peer::Channel(channel) => {
                                match client
                                    .invoke(&tl::functions::channels::GetChannels {
                                        id: vec![tl::enums::InputChannel::Channel(
                                            tl::types::InputChannel {
                                                channel_id: channel.channel_id,
                                                access_hash: 0,
                                            },
                                        )],
                                    })
                                    .await
                                {
                                    Ok(channel_info) => match channel_info {
                                        tl::enums::messages::Chats::Chats(chats) => {
                                            if let Some(chat) = chats.chats.first() {
                                                match chat {
                                                    tl::enums::Chat::Channel(channel) => {
                                                        channel.title.clone()
                                                    }
                                                    _ => format!("Chat: {}", chat_id),
                                                }
                                            } else {
                                                format!("Chat: {}", chat_id)
                                            }
                                        }
                                        _ => format!("Chat: {}", chat_id),
                                    },
                                    Err(_) => format!("Chat: {}", chat_id),
                                }
                            }
                            tl::enums::Peer::Chat(chat) => {
                                match client
                                    .invoke(&tl::functions::messages::GetChats {
                                        id: vec![chat.chat_id],
                                    })
                                    .await
                                {
                                    Ok(chat_info) => match chat_info {
                                        tl::enums::messages::Chats::Chats(chats) => {
                                            if let Some(chat) = chats.chats.first() {
                                                match chat {
                                                    tl::enums::Chat::Chat(chat) => {
                                                        chat.title.clone()
                                                    }
                                                    _ => format!("Chat: {}", chat_id),
                                                }
                                            } else {
                                                format!("Chat: {}", chat_id)
                                            }
                                        }
                                        _ => format!("Chat: {}", chat_id),
                                    },
                                    Err(_) => format!("Chat: {}", chat_id),
                                }
                            }
                            tl::enums::Peer::User(_) => {
                                format!("User: {}", chat_id)
                            }
                        },
                    },
                    Err(e) => {
                        LOGGER
                            .error(&format!("❌ Failed to resolve username {}: {}", chat_id, e))
                            .await;
                        format!("Chat: {}", chat_id)
                    }
                }
            }
        };
        bot.edit_message_text(
            message.chat.id,
            sent_msg.id,
            format!(
                "<b>Credit Card Scraping Processing...</b>\n\n\
                 <b>Requested Amount:</b> <code>{}</code>\n\n\
                 <b>Target Chat:</b> <code>{}</code>\n\
                 <b>BIN Filtering:</b> <code>{}</code>\n\
                 <b>Timestamp:</b> {}",
                chat_title,
                limit,
                bin_filter.as_deref().unwrap_or("None"),
                timestamp
            ),
        )
        .parse_mode(ParseMode::Html)
        .await
        .unwrap();

        let (total_found, unique_cards) = {
            let config = match get_config() {
                Ok(config) => config,
                Err(e) => {
                    LOGGER
                        .error(&format!("❌ Failed to get config: {}", e))
                        .await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> Failed to load configuration.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            };
            let cc_regex = match Regex::new(&config.config.regex.cc_regex) {
                Ok(regex) => regex,
                Err(e) => {
                    LOGGER
                        .error(&format!("❌ Failed to create regex: {}", e))
                        .await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> Failed to create regex pattern.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            };
            let mut total_found = 0;
            let mut cards_set = HashSet::new();

            let input_peer = if let Ok(id) = chat_id.parse::<i64>() {
                match client
                    .invoke(&tl::functions::channels::GetChannels {
                        id: vec![tl::enums::InputChannel::Channel(tl::types::InputChannel {
                            channel_id: id,
                            access_hash: 0,
                        })],
                    })
                    .await
                {
                    Ok(tl::enums::messages::Chats::Chats(chats)) => {
                        if let Some(chat) = chats.chats.first() {
                            match chat {
                                tl::enums::Chat::Channel(channel) => {
                                    tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                                        channel_id: channel.id,
                                        access_hash: channel.access_hash.unwrap_or(0),
                                    })
                                }
                                _ => {
                                    LOGGER
                                        .error(&format!("❌ Invalid channel type for ID: {}", id))
                                        .await;
                                    let reply = format!(
                                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                         <b>Reason:</b> Invalid channel type.\n\
                                         <b>Timestamp:</b> {}",
                                        timestamp
                                    );
                                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                        .parse_mode(ParseMode::Html)
                                        .await
                                        .unwrap();
                                    SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                                    return;
                                }
                            }
                        } else {
                            LOGGER
                                .error(&format!("❌ Channel not found for ID: {}", id))
                                .await;
                            let reply = format!(
                                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                 <b>Reason:</b> Channel not found.\n\
                                 <b>Timestamp:</b> {}",
                                timestamp
                            );
                            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                .parse_mode(ParseMode::Html)
                                .await
                                .unwrap();
                            SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                            return;
                        }
                    }
                    Ok(tl::enums::messages::Chats::Slice(_)) => {
                        LOGGER
                            .error(&format!("❌ Unexpected response format for ID: {}", id))
                            .await;
                        let reply = format!(
                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Unexpected response format.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        );
                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                            .parse_mode(ParseMode::Html)
                            .await
                            .unwrap();
                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                    Err(e) => {
                        LOGGER
                            .error(&format!(
                                "GetChannels failed for ID {}: {:?}, trying direct approach",
                                id, e
                            ))
                            .await;
                        tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                            channel_id: id,
                            access_hash: 0,
                        })
                    }
                }
            } else {
                match client
                    .invoke(&tl::functions::contacts::ResolveUsername {
                        username: chat_id.to_string(),
                        referer: None,
                    })
                    .await
                {
                    Ok(resolved) => match resolved {
                        tl::enums::contacts::ResolvedPeer::Peer(peer) => match peer.peer {
                            tl::enums::Peer::Channel(channel) => {
                                match client
                                    .invoke(&tl::functions::channels::GetChannels {
                                        id: vec![tl::enums::InputChannel::Channel(
                                            tl::types::InputChannel {
                                                channel_id: channel.channel_id,
                                                access_hash: 0,
                                            },
                                        )],
                                    })
                                    .await
                                {
                                    Ok(tl::enums::messages::Chats::Chats(chats)) => {
                                        if let Some(chat) = chats.chats.first() {
                                            match chat {
                                                tl::enums::Chat::Channel(channel) => {
                                                    tl::enums::InputPeer::Channel(
                                                        tl::types::InputPeerChannel {
                                                            channel_id: channel.id,
                                                            access_hash: channel
                                                                .access_hash
                                                                .unwrap_or(0),
                                                        },
                                                    )
                                                }
                                                _ => {
                                                    LOGGER
                                                        .error(&format!("❌ Invalid channel type"))
                                                        .await;
                                                    let reply = format!(
                                                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                         <b>Reason:</b> Invalid channel type.\n\
                                         <b>Timestamp:</b> {}",
                                                        timestamp
                                                    );
                                                    bot.edit_message_text(
                                                        message.chat.id,
                                                        sent_msg.id,
                                                        reply,
                                                    )
                                                    .parse_mode(ParseMode::Html)
                                                    .await
                                                    .unwrap();
                                                    SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                                                    return;
                                                }
                                            }
                                        } else {
                                            LOGGER.error(&format!("❌ Channel not found")).await;
                                            let reply = format!(
                                                "<b>Credit Card Scraping Failed ❌</b>\n\n\
                                 <b>Reason:</b> Channel not found.\n\
                                 <b>Timestamp:</b> {}",
                                                timestamp
                                            );
                                            bot.edit_message_text(
                                                message.chat.id,
                                                sent_msg.id,
                                                reply,
                                            )
                                            .parse_mode(ParseMode::Html)
                                            .await
                                            .unwrap();
                                            SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                                            return;
                                        }
                                    }
                                    Ok(tl::enums::messages::Chats::Slice(_)) => {
                                        LOGGER
                                            .error(&format!("❌ Unexpected response format"))
                                            .await;
                                        let reply = format!(
                                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Unexpected response format.\n\
                             <b>Timestamp:</b> {}",
                                            timestamp
                                        );
                                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                                            .parse_mode(ParseMode::Html)
                                            .await
                                            .unwrap();
                                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                                        return;
                                    }
                                    Err(e) => {
                                        LOGGER.error(&format!("GetChannels failed for username {}: {:?}, trying direct approach", chat_id, e)).await;
                                        tl::enums::InputPeer::Channel(tl::types::InputPeerChannel {
                                            channel_id: channel.channel_id,
                                            access_hash: 0,
                                        })
                                    }
                                }
                            }
                            tl::enums::Peer::Chat(chat) => {
                                tl::enums::InputPeer::Chat(tl::types::InputPeerChat {
                                    chat_id: chat.chat_id,
                                })
                            }
                            tl::enums::Peer::User(user) => {
                                tl::enums::InputPeer::User(tl::types::InputPeerUser {
                                    user_id: user.user_id,
                                    access_hash: 0,
                                })
                            }
                        },
                    },
                    Err(e) => {
                        LOGGER
                            .error(&format!("❌ Failed to resolve username {}: {}", chat_id, e))
                            .await;
                        let reply = format!(
                            "<b>Credit Card Scraping Failed ❌</b>\n\n\
                             <b>Reason:</b> Failed to resolve username.\n\
                             <b>Timestamp:</b> {}",
                            timestamp
                        );
                        bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                            .parse_mode(ParseMode::Html)
                            .await
                            .unwrap();
                        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                }
            };

            let messages = match client
                .invoke(&tl::functions::messages::GetHistory {
                    peer: input_peer.clone(),
                    offset_id: 0,
                    offset_date: 0,
                    add_offset: 0,
                    limit: limit,
                    max_id: 0,
                    min_id: 0,
                    hash: 0,
                })
                .await
            {
                Ok(messages) => messages,
                Err(e) => {
                    let error_msg = match e.to_string().as_str() {
                        msg if msg.contains("CHANNEL_INVALID") => {
                            format!("Channel '{}' is invalid, private, or doesn't exist. Make sure the channel ID is correct and the bot has access to it.", chat_id)
                        }
                        msg if msg.contains("CHANNEL_PRIVATE") => {
                            format!("Channel '{}' is private. The bot needs to be added to the channel first.", chat_id)
                        }
                        msg if msg.contains("CHAT_INVALID") => {
                            format!("Chat '{}' is invalid or doesn't exist.", chat_id)
                        }
                        msg if msg.contains("ACCESS_TOKEN_INVALID") => {
                            format!("Access denied for channel '{}'. The bot doesn't have permission to access this channel.", chat_id)
                        }
                        _ => {
                            format!("Failed to access channel '{}': {}", chat_id, e)
                        }
                    };
                    LOGGER.error(&format!("❌ {}", error_msg)).await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> {}\n\
                         <b>Timestamp:</b> {}",
                        error_msg, timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            };
            match messages {
                tl::enums::messages::Messages::Messages(messages) => {
                    for message in messages.messages {
                        if let tl::enums::Message::Message(msg) = message {
                            let text = msg.message.clone();
                            for line in text.lines() {
                                if let Some(mat) = cc_regex.find(line) {
                                    let cc_number = mat.as_str().to_string();

                                    if let Some(ref bin) = bin_filter {
                                        if !cc_number.starts_with(bin) {
                                            continue;
                                        }
                                    }
                                    total_found += 1;
                                    cards_set.insert(cc_number);
                                }
                            }
                        }
                    }
                }
                tl::enums::messages::Messages::ChannelMessages(channel_messages) => {
                    for message in channel_messages.messages {
                        if let tl::enums::Message::Message(msg) = message {
                            let text = msg.message.clone();
                            for line in text.lines() {
                                if let Some(mat) = cc_regex.find(line) {
                                    let cc_number = mat.as_str().to_string();

                                    if let Some(ref bin) = bin_filter {
                                        if !cc_number.starts_with(bin) {
                                            continue;
                                        }
                                    }
                                    total_found += 1;
                                    cards_set.insert(cc_number);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            (total_found, cards_set.into_iter().collect::<Vec<String>>())
        };
        SCRAPE_QUEUE.fetch_sub(1, Ordering::SeqCst);
        if total_found == 0 {
            let reply = format!(
                "<b>Credit Card Scraping Information ℹ️</b>\n\
                 <b>Reason:</b> No cards were found.\n\
                 <b>Timestamp:</b> {}",
                timestamp
            );
            bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                .parse_mode(ParseMode::Html)
                .await
                .unwrap();
            return;
        }

        let unique_count = unique_cards.len();
        let duplicates = total_found - unique_count;
        let time_taken = start_time.elapsed().as_secs_f64();

        {
            let filename = format!("x{}_Scrapped_KissShot.txt", unique_count);
            let mut file = match File::create(&filename).await {
                Ok(file) => file,
                Err(e) => {
                    LOGGER
                        .error(&format!("❌ Failed to create file: {}", e))
                        .await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> Failed to create result file.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                    return;
                }
            };
            if let Err(e) = file.write_all(unique_cards.join("\n").as_bytes()).await {
                LOGGER
                    .error(&format!("❌ Failed to write file: {}", e))
                    .await;
                let reply = format!(
                    "<b>Credit Card Scraping Failed ❌</b>\n\n\
                     <b>Reason:</b> Failed to write result file.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            if let Err(e) = file.flush().await {
                LOGGER
                    .error(&format!("❌ Failed to flush file: {}", e))
                    .await;
                let reply = format!(
                    "<b>Credit Card Scraping Failed ❌</b>\n\n\
                     <b>Reason:</b> Failed to flush result file.\n\
                     <b>Timestamp:</b> {}",
                    timestamp
                );
                bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                    .parse_mode(ParseMode::Html)
                    .await
                    .unwrap();
                return;
            }
            drop(file);

            let user = handle_database_error!(
                bot,
                message,
                crate::plugins::helpers::database::safe_fetch_user(user_id).await,
                "Credit Card Scraping"
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
            let caption = format!(
                "<b>Credit Card Scraping Successful ✅</b>\n\n\
                 <b>Scrape Target Chat:</b> <code>{}</code>\n\
                 <b>Requested Amount:</b> <code>{}</code>\n\n\
                 <b>Credit Cards Found:</b> <code>{}</code>\n\
                 <b>Unique Credit Cards:</b> <code>{}</code>\n\
                 <b>Duplicates:</b> <code>{}</code>\n\
                 <b>Time Taken:</b> <code>{:.2}s</code>\n\n\
                 {}",
                chat_title, limit, total_found, unique_count, duplicates, time_taken, end_text
            );
            let input_file = InputFile::file(&filename);
            match bot
                .send_document(message.chat.id, input_file)
                .caption(caption)
                .parse_mode(ParseMode::Html)
                .await
            {
                Ok(_) => {
                    bot.delete_message(message.chat.id, sent_msg.id).await.ok();
                    LOGGER
                        .info(&format!(
                        "✅ Credit Card Scraping Successful: {} total, {} unique, {} duplicates",
                        total_found, unique_count, duplicates
                    ))
                        .await;

                    let (remaining_time, cooldown_setting) = get_user_antispam_info(user_id).await;
                    LOGGER
                        .info(&format!(
                            "User {} used CC scraper successfully. Cooldown: {}s, Remaining: {}s",
                            user_id, cooldown_setting, remaining_time
                        ))
                        .await;
                }
                Err(e) => {
                    LOGGER
                        .error(&format!("❌ Failed to send file: {}", e))
                        .await;
                    let reply = format!(
                        "<b>Credit Card Scraping Failed ❌</b>\n\n\
                         <b>Reason:</b> Failed to send result file.\n\
                         <b>Timestamp:</b> {}",
                        timestamp
                    );
                    bot.edit_message_text(message.chat.id, sent_msg.id, reply)
                        .parse_mode(ParseMode::Html)
                        .await
                        .unwrap();
                }
            }

            tokio::fs::remove_file(&filename).await.ok();
        }
    }
}
