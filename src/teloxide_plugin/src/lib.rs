#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(non_snake_case)]
use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{parse_macro_input, Expr, ExprArray, ItemStruct, Meta};
#[proc_macro_attribute]
pub fn TeloxidePlugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let attr: Expr = parse_macro_input!(attr as Expr);
    let commands: Vec<String> = if let Expr::Assign(assign) = attr {
        let attr_name = assign.left.to_token_stream().to_string();
        if attr_name == "commands" || attr_name == "callback_data" {
            match *assign.right {
                Expr::Array(ExprArray { elems, .. }) => elems
                    .into_iter()
                    .filter_map(|e| {
                        if let Expr::Lit(lit) = e {
                            if let syn::Lit::Str(s) = lit.lit {
                                Some(s.value())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect(),
                Expr::Lit(lit) => {
                    if let syn::Lit::Str(s) = lit.lit {
                        s.value()
                            .split(',')
                            .map(|cmd| cmd.trim().to_string())
                            .filter(|cmd| !cmd.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    }
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let command_literals: Vec<_> = commands.iter().map(|c| c.as_str()).collect();
    let name = &input.ident;
    let expanded = quote! {
        #input
        #[async_trait::async_trait]
        impl crate::plugin_handler::Plugin for #name {
            fn name(&self) -> &'static str { stringify!(#name) }
            fn commands(&self) -> &'static [&'static str] {
                &[#(#command_literals),*]
            }
            async fn handle_message_async(&self, bot: &teloxide::Bot, message: &teloxide::types::Message, msg: &str) {
                self.handle(bot, message, msg).await
            }
            async fn handle_callback_async(&self, bot: &teloxide::Bot, message: &teloxide::types::Message, msg: &str, user_id: u64, callback_query: &teloxide::types::CallbackQuery) {
                // For KeyboardHandlerPlugin and CCGeneratorPlugin, we need to pass the user_id
                if stringify!(#name) == "KeyboardHandlerPlugin" || stringify!(#name) == "CCGeneratorPlugin" {
                    // Create a temporary message with the correct user ID
                    let mut temp_message = message.clone();
                    if let Some(from) = temp_message.from.as_mut() {
                        from.id = teloxide::types::UserId(user_id);
                        // Use the actual user's first name from the callback query
                        from.first_name = callback_query.from.first_name.clone();
                        from.last_name = callback_query.from.last_name.clone();
                        from.username = callback_query.from.username.clone();
                    }
                    self.handle(bot, &temp_message, msg).await
                } else {
                    self.handle(bot, message, msg).await
                }
            }
        }
        inventory::submit! {
            crate::plugin_handler::PluginRegistration {
                name: stringify!(#name),
                commands: &[#(#command_literals),*],
                factory: || std::sync::Arc::new(#name{}) as std::sync::Arc<dyn crate::plugin_handler::Plugin + Send + Sync>,
            }
        }
    };
    TokenStream::from(expanded)
}
