#[macro_export]
macro_rules! register_plugin {
    ($plugin:ident) => {{
        let plugin = $plugin {};
        $crate::plugin_handler::register(Box::new(plugin));
    }};
}
