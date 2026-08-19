#![allow(unused_variables)]
mod bot_commands;
mod config;
mod database;
mod error_handler;
mod logging;
mod plugin_handler;
mod plugins;
use crate::config::get_config;
use crate::error_handler::GlobalErrorHandler;
use crate::logging::{get_logger, shutdown_logger, LogLevel, LoggerConfig};
use crate::plugin_handler::PluginHandler;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::Bot;
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install AWS-LC-RS crypto provider");
    let logger = get_logger("main").await;
    let banner = r#"
╔══════════════════════════════════════════════════════════════════════════════╗
║                                                                              ║
║  ██╗  ██╗██╗███████╗███████╗███████╗██╗  ██╗ ██████╗ ████████╗               ║
║  ██║ ██╔╝██║██╔════╝██╔════╝██╔════╝██║  ██║██╔═══██╗╚══██╔══╝               ║
║  █████╔╝ ██║███████╗███████╗███████╗███████║██║   ██║   ██║                  ║
║  ██╔═██╗ ██║╚════██║╚════██║╚════██║██╔══██║██║   ██║   ██║                  ║
║  ██║  ██╗██║███████║███████║███████║██║  ██║╚██████╔╝   ██║                  ║
║  ╚═╝  ╚═╝╚═╝╚══════╝╚══════╝╚══════╝╚═╝  ╚═╝ ╚═════╝    ╚═╝                  ║
║                                                                              ║
║  🤖 Starting KissShot...                                                     ║
║  📊 Enhanced Logging System                                                  ║
║                                                                              ║
╚══════════════════════════════════════════════════════════════════════════════╝
    "#;
    println!("{}", banner);
    let config = match get_config() {
        Ok(config) => {
            logger.info("✅ Configuration loaded successfully").await;
            config
        }
        Err(e) => {
            logger
                .critical(&format!("❌ Failed to load configuration: {}", e))
                .await;
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };
    let logger_config = LoggerConfig {
        level: if config.teloxide.logging {
            LogLevel::Info
        } else {
            LogLevel::Warning
        },
        console_output: true,
        file_output: true,
        json_output: false,
        max_file_size: 10 * 1024 * 1024,
        backup_count: 5,
        colored_console: true,
        include_traceback: true,
        log_sql: false,
        log_telegram: true,
        log_errors: true,
        log_performance: false,
        buffer_size: 1000,
    };
    logger.info("🔧 Configuring logging system...").await;
    crate::logging::log::configure_logger(logger_config).await;
    logger.info("🚀 Starting KissShot Bot...").await;
    logger
        .info(&format!(
            "📊 Bot Token: {}...",
            &config.telegram.bot.bot_token[..8]
        ))
        .await;
    logger
        .info(&format!("🔧 Workers: {}", config.teloxide.workers))
        .await;
    logger
        .info(&format!("⏱️ Timeout: {}s", config.teloxide.timeout))
        .await;
    let bot = Bot::new(&config.telegram.bot.bot_token);

    if let Err(e) = bot_commands::register_bot_commands(&bot).await {
        logger
            .warning(&format!("Failed to register Telegram command menu: {}", e))
            .await;
    } else {
        logger.info("📋 Telegram command menu registered").await;
    }

    // Initialize global error handler
    let error_handler = GlobalErrorHandler::new(bot.clone()).await;
    error_handler.init_panic_handler();
    logger.info("🛡️ Global error handler initialized").await;

    let handler = Arc::new(PluginHandler::new());
    let message_handler = {
        let handler = handler.clone();
        let telegram_logger = get_logger("telegram").await;
        let error_handler = error_handler.clone();
        move |bot: Bot, msg: Message| {
            let handler = handler.clone();
            let telegram_logger = telegram_logger.clone();
            let error_handler = error_handler.clone();
            async move {
                if let Some(text) = msg.text() {
                    let user_id = msg.from.as_ref().map(|u| u.id.0).unwrap_or(0);

                    telegram_logger
                        .info(&format!(
                            "📨 Received message from user {}: {}",
                            user_id, text
                        ))
                        .await;

                    let mut parts = text.splitn(2, ' ');
                    let command = parts.next().unwrap_or("");
                    let body = parts.next().unwrap_or("");

                    telegram_logger
                        .debug(&format!(
                            "🔍 Parsed command: '{}', body: '{}'",
                            command, body
                        ))
                        .await;

                    let handler = handler.clone();
                    let bot = bot.clone();
                    let msg = msg.clone();
                    let command = command.to_string();
                    let body = body.to_string();
                    let error_handler = error_handler.clone();

                    // Spawn with error handling
                    let chat_id = msg.chat.id;
                    error_handler.spawn_with_error_handling_with_chat(
                        "message_handler",
                        chat_id,
                        async move {
                            handler.handle(&bot, &msg, &command, &body).await;
                        },
                    );
                }
                Ok::<(), teloxide::RequestError>(())
            }
        }
    };
    let callback_handler = {
        let handler = handler.clone();
        let telegram_logger = get_logger("telegram").await;
        let error_handler = error_handler.clone();
        move |bot: Bot, cb: CallbackQuery| {
            let handler = handler.clone();
            let telegram_logger = telegram_logger.clone();
            let error_handler = error_handler.clone();
            async move {
                let user_id = cb.from.id.0;

                telegram_logger
                    .info(&format!(
                        "🔘 Received callback query from user {}: {:?}",
                        user_id, cb
                    ))
                    .await;

                if let Some(data) = &cb.data {
                    let data_clone = data.clone();
                    let cb_clone = cb.clone();
                    let cb_id = cb_clone.id.clone();

                    telegram_logger
                        .debug(&format!("📊 Callback data: {}", data_clone))
                        .await;

                    match &cb_clone.message {
                        Some(teloxide::types::MaybeInaccessibleMessage::Regular(message)) => {
                            println!(
                                "DEBUG: Callback query from user: {}, data: {}",
                                user_id, data_clone
                            );

                            // Clone values needed for the spawned task
                            let handler_clone = handler.clone();
                            let bot_clone = bot.clone();
                            let message_clone = message.clone();
                            let cb_for_spawn = cb_clone.clone();
                            let error_handler = error_handler.clone();

                            // Spawn with error handling
                            error_handler.spawn_with_error_handling(
                                "callback_handler",
                                async move {
                                    handler_clone
                                        .handle_callback(
                                            &bot_clone,
                                            &message_clone,
                                            &data_clone,
                                            "",
                                            user_id,
                                            cb_for_spawn,
                                        )
                                        .await;
                                },
                            );
                        }
                        Some(teloxide::types::MaybeInaccessibleMessage::Inaccessible(_)) => {
                            telegram_logger
                                .warning("Callback query with inaccessible message")
                                .await;
                        }
                        None => {
                            telegram_logger
                                .warning("Callback query without associated message")
                                .await;
                        }
                    }

                    if let Err(e) = bot.answer_callback_query(cb_id).await {
                        error_handler
                            .handle_telegram_error(&e, "answer_callback_query")
                            .await;
                    }
                }
                Ok::<(), teloxide::RequestError>(())
            }
        }
    };
    let chat_member_handler = {
        let telegram_logger = get_logger("telegram").await;
        let error_handler = error_handler.clone();
        move |bot: Bot, update: teloxide::types::ChatMemberUpdated| {
            let telegram_logger = telegram_logger.clone();
            let error_handler = error_handler.clone();
            async move {
                telegram_logger
                    .info(&format!(
                        "👥 Chat member updated in chat {}: {:?}",
                        update.chat.id, update
                    ))
                    .await;

                // Spawn with error handling
                error_handler.spawn_with_error_handling("chat_member_handler", async move {
                    crate::plugins::utility::welcome::handle_chat_member_updated(&bot, &update)
                        .await;
                });
                Ok::<(), teloxide::RequestError>(())
            }
        }
    };
    let dispatcher_handler = dptree::entry()
        .branch(Update::filter_message().endpoint(message_handler))
        .branch(Update::filter_callback_query().endpoint(callback_handler))
        .branch(Update::filter_chat_member().endpoint(chat_member_handler));
    logger.info("🔧 Building dispatcher...").await;
    let mut dispatcher = Dispatcher::builder(bot, dispatcher_handler).build();
    logger
        .info("✅ Bot is now running! Press Ctrl+C to stop.")
        .await;

    tokio::select! {
        _ = dispatcher.dispatch() => {
            logger.info("👋 Bot shutdown complete").await;
        }
        _ = tokio::signal::ctrl_c() => {
            logger.info("🛑 Received shutdown signal, stopping bot...").await;
            logger.info("👋 Bot shutdown complete").await;
        }
    }

    shutdown_logger().await;
}
