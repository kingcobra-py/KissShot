use teloxide::payloads::SetMyCommandsSetters;
use teloxide::types::{BotCommand, BotCommandScope};
use teloxide::{prelude::Requester, Bot};

pub async fn register_bot_commands(bot: &Bot) -> Result<(), teloxide::RequestError> {
    let commands = vec![
        BotCommand {
            command: "start".into(),
            description: "Start KissShot".into(),
        },
        BotCommand {
            command: "setsk".into(),
            description: "Validate and save your Stripe SK".into(),
        },
        BotCommand {
            command: "setproxy".into(),
            description: "Set proxy for your SK session".into(),
        },
        BotCommand {
            command: "skchk".into(),
            description: "Check CC using your saved SK".into(),
        },
        BotCommand {
            command: "skstatus".into(),
            description: "View your SK session".into(),
        },
        BotCommand {
            command: "clearsk".into(),
            description: "Clear saved SK and proxy".into(),
        },
        BotCommand {
            command: "sk".into(),
            description: "Full Stripe SK check".into(),
        },
        BotCommand {
            command: "skbase".into(),
            description: "Base SK check (bypasses PM rate limit)".into(),
        },
        BotCommand {
            command: "proxy".into(),
            description: "Fetch working proxies".into(),
        },
        BotCommand {
            command: "skgen".into(),
            description: "Generate Stripe keys".into(),
        },
        BotCommand {
            command: "register".into(),
            description: "Register your account".into(),
        },
        BotCommand {
            command: "chk".into(),
            description: "Check card via Stripe gateway".into(),
        },
        BotCommand {
            command: "gen".into(),
            description: "Generate cards from BIN".into(),
        },
        BotCommand {
            command: "bin".into(),
            description: "BIN lookup".into(),
        },
        BotCommand {
            command: "id".into(),
            description: "Your account info".into(),
        },
        BotCommand {
            command: "redeem".into(),
            description: "Redeem a gift code".into(),
        },
    ];

    bot.set_my_commands(commands)
        .scope(BotCommandScope::Default)
        .await?;

    Ok(())
}
