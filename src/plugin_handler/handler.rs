pub use crate::plugin_handler::plugin::Plugin;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use teloxide::types::Message;
use teloxide::Bot;
pub struct PluginRegistration {
    pub name: &'static str,
    pub commands: &'static [&'static str],
    pub factory: fn() -> Arc<dyn Plugin + Send + Sync>,
}
inventory::collect!(PluginRegistration);
#[derive(Clone)]
pub struct PluginHandler;
impl PluginHandler {
    pub fn new() -> Self {
        for registration in inventory::iter::<PluginRegistration> {
            let plugin = (registration.factory)();
            register(plugin);
        }
        Self
    }
    pub async fn handle(&self, bot: &Bot, message: &Message, command: &str, msg: &str) {
        handle_command_async(bot, message, command, msg).await
    }
    pub async fn handle_callback(
        &self,
        bot: &Bot,
        message: &Message,
        command: &str,
        msg: &str,
        user_id: u64,
        callback_query: teloxide::types::CallbackQuery,
    ) {
        handle_callback_async(bot, message, command, msg, user_id, &callback_query).await
    }
}
lazy_static::lazy_static! {
    static ref PLUGIN_REGISTRY: RwLock<HashMap<&'static str, Arc<dyn Plugin + Send + Sync>>> = RwLock::new(HashMap::new());
}
pub fn register(plugin: Arc<dyn Plugin + Send + Sync>) {
    let name = plugin.name();
    PLUGIN_REGISTRY.write().unwrap().insert(name, plugin);
}
pub async fn handle_command_async(bot: &Bot, message: &Message, command: &str, msg: &str) {
    let plugin_names: Vec<&'static str> = {
        let registry = PLUGIN_REGISTRY.read().unwrap();
        registry.keys().cloned().collect()
    };
    for plugin_name in plugin_names {
        let plugin = {
            let registry = PLUGIN_REGISTRY.read().unwrap();
            registry.get(plugin_name).cloned()
        };
        if let Some(plugin) = plugin {
            // Check for exact command match
            if plugin.commands().contains(&command) {
                // For callbacks, pass the command as the msg parameter if msg is empty
                let msg_to_pass = if msg.is_empty() { command } else { msg };
                plugin.handle_message_async(bot, message, msg_to_pass).await;
                return;
            }
            // Check for dynamic callback patterns (folder:*, page:*)
            if command.starts_with("folder:") || command.starts_with("page:") {
                // Check if this plugin handles dynamic callbacks by looking for any folder: or page: commands
                let has_dynamic_handlers = plugin
                    .commands()
                    .iter()
                    .any(|cmd| cmd.starts_with("folder:") || cmd.starts_with("page:"));
                if has_dynamic_handlers {
                    // For callbacks, pass the command as the msg parameter if msg is empty
                    let msg_to_pass = if msg.is_empty() { command } else { msg };
                    plugin.handle_message_async(bot, message, msg_to_pass).await;
                    return;
                }
            }
            if command.starts_with('/') {
                let command_without_slash = &command[1..];
                if plugin.commands().contains(&command_without_slash) {
                    plugin.handle_message_async(bot, message, msg).await;
                    return;
                }
            }
            if !command.starts_with('/') {
                let command_with_slash = format!("/{}", command);
                if plugin.commands().contains(&command_with_slash.as_str()) {
                    plugin.handle_message_async(bot, message, msg).await;
                    return;
                }
            }
        }
    }
}
pub async fn handle_callback_async(
    bot: &Bot,
    message: &Message,
    command: &str,
    msg: &str,
    user_id: u64,
    callback_query: &teloxide::types::CallbackQuery,
) {
    println!(
        "DEBUG: handle_callback_async called with user_id: {}, command: {}",
        user_id, command
    );
    let plugin_names: Vec<&'static str> = {
        let registry = PLUGIN_REGISTRY.read().unwrap();
        registry.keys().cloned().collect()
    };
    for plugin_name in plugin_names {
        let plugin = {
            let registry = PLUGIN_REGISTRY.read().unwrap();
            registry.get(plugin_name).cloned()
        };
        if let Some(plugin) = plugin {
            if plugin.commands().contains(&command) {
                let msg_to_pass = if msg.is_empty() { command } else { msg };
                plugin
                    .handle_callback_async(bot, message, msg_to_pass, user_id, callback_query)
                    .await;
                return;
            }

            if command.starts_with("folder:")
                || command.starts_with("page:")
                || command.starts_with("gen_again:")
                || command.starts_with("vbv_remove_dead_")
            {
                println!(
                    "🔍 Command {} starts with folder:, page:, gen_again:, or vbv_remove_dead_, checking plugin {}",
                    command, plugin_name
                );

                let commands = plugin.commands();
                println!("🔍 Plugin {} commands: {:?}", plugin_name, commands);
                let should_handle = if command.starts_with("gen_again:") {
                    commands.iter().any(|cmd| cmd.starts_with("gen_again:"))
                } else if command.starts_with("folder:") {
                    commands.iter().any(|cmd| cmd.starts_with("folder:"))
                } else if command.starts_with("page:") {
                    commands.iter().any(|cmd| cmd.starts_with("page:"))
                } else if command.starts_with("vbv_remove_dead_") {
                    commands
                        .iter()
                        .any(|cmd| cmd.starts_with("vbv_remove_dead_"))
                } else {
                    false
                };
                println!(
                    "🔍 Plugin {} should handle this callback: {}",
                    plugin_name, should_handle
                );
                if should_handle {
                    println!("✅ Routing command {} to plugin {}", command, plugin_name);

                    let msg_to_pass = if msg.is_empty() { command } else { msg };
                    plugin
                        .handle_callback_async(bot, message, msg_to_pass, user_id, callback_query)
                        .await;
                    return;
                }
            }
        }
    }
}
