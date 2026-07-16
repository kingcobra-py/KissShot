# KissShot

A Telegram bot for validating credit cards. Built in Rust with [teloxide](https://github.com/teloxide/teloxide).

## Features

- **CC Checking** — Validates cards against Stripe, Braintree, PayPal, and Authorize.net gateways
- **SK Checking** — Tests Stripe secret keys for validity
- **CC/SK Generation** — Generates card numbers and Stripe keys
- **BIN Lookup** — Pulls issuing bank info from BIN numbers
- **IP Lookup** — Resolves IP geolocation
- **Proxy Support** — Rotating proxy integration for request throttling
- **User System** — Registration, balance tracking, gift codes, and access tiers (FREE / VIP / PREMIUM)
- **Admin Panel** — User authorization, banning, broadcasting, and account management
- **Antispam** — Configurable rate limiting per user

## Architecture

```
src/
├── main.rs                    # Entry point, dispatcher setup
├── config/                    # TOML config loading + validation
├── database/
│   ├── sql.rs                 # MySQL (sqlx) — users, bans, sessions
│   └── kvs.rs                 # Redis/Valkey — key-value store, caching
├── plugin_handler/            # Plugin trait, dispatcher, macros
├── plugins/
│   ├── basic/                 # /start, /register, keyboard handlers
│   ├── admin/                 # /authorize, /ban, /broadcast, etc.
│   ├── gateways/              # /chk, /an, /pp, /sta, /vbv
│   ├── utility/               # /gen, /bin, /ip, /sk, /scr, etc.
│   └── helpers/               # Shared inspection checks, DB access, utils
├── logging/                   # File + console logging with levels
└── teloxide_plugin/           # Proc macro crate for #[TeloxidePlugin]
```

Plugins are registered at compile time using the `inventory` crate. Each plugin implements a `Plugin` trait and declares its commands via the `#[TeloxidePlugin]` attribute macro.

## Commands

<table>
<tr>
<td width="50%">

#### User

| Command              | Description                          |
|----------------------|--------------------------------------|
| `/start` `/help` `/cmds` | Bot info and command list      |
| `/register`          | Register an account                  |
| `/chk <card>`        | Check card via Stripe                |
| `/an <card>`         | Check card via Authorize.net         |
| `/pp <card>`         | Check card via PayPal                |
| `/sta <card>`        | Check card via Stripe (alt)          |
| `/vbv <card>`        | VBV verification check               |
| `/gen <bin>`         | Generate cards from BIN              |
| `/sk <key>`          | Check a Stripe secret key            |
| `/skgen`             | Generate Stripe keys                 |
| `/skscr`             | Scrape Stripe keys from Telegram     |
| `/scr`               | Scrape card numbers from Telegram    |
| `/bin <number>`      | BIN lookup                           |
| `/ip <address>`      | IP geolocation                       |
| `/filter <text>`     | Filter/validate card data            |
| `/id`                | Telegram user ID info                |
| `/proxy`             | Fetch working proxies                |
| `/redeem <code>`     | Redeem a gift code                   |

</td>
<td width="50%">

#### Admin

| Command              | Description                       |
|----------------------|-----------------------------------|
| `/authorize <user>`  | Grant access                      |
| `/deauthorize <user>`| Revoke access                     |
| `/ban <user>`        | Ban a user                        |
| `/unban <user>`      | Unban a user                      |
| `/broadcast <text>`  | Send message to all users         |
| `/upgrade <user>`    | Upgrade user tier                 |
| `/degrade <user>`    | Downgrade user tier               |
| `/codegen`           | Generate gift codes               |

</td>
</tr>
</table>

## Requirements

- Rust 1.83+
- MySQL database
- Redis or Valkey instance
- A Telegram bot token (from [@BotFather](https://t.me/BotFather))

## Setup

1. Clone the repo:
   ```sh
   git clone https://github.com/Junaid433/kissshot_rs.git
   cd kissshot_rs
   ```

2. Copy and edit the config:
   ```sh
   cp config.toml config.toml
   ```
   Fill in your Telegram bot token, database credentials, and admin user ID.

3. Build and run:
   ```sh
   cargo run --release
   ```

## Config

Configuration lives in `config.toml`.

```toml
[Telegram.BOT]
API_ID, API_HASH    # From my.telegram.org
BOT_TOKEN           # From @BotFather

[Telegram.USER]
API_ID, API_HASH, PHONE_NUMBER  # Used by scraper plugins (/scr, /skscr)

[Database.SQL]
URI    # MySQL connection string

[Database.REDIS]
URI    # Redis/Valkey connection string

[Teloxide]
WORKERS              # Async worker threads (default: 10)
TIMEOUT              # Telegram API request timeout in seconds
REQUEST_CONCURRENCY  # Max concurrent outgoing requests
LOGGING              # Verbose logging toggle

[Config.BASIC]
ADMIN    = [user_id]     # Admin Telegram user IDs
CHANNEL  = "username"    # Required channel for access checks
GROUP    = "username"    # Required group for access checks

[Config.LIMITS]
MAX_CC_SCR / MAX_SK_SCR   # Max items per scrape request
MAX_CC_CHK / MAX_SK_CHK   # Max items per check (non-admin)

[Config.REGEX]
CC_REGEX   # Card format: NUMBER|MM|YY|CVV
SK_REGEX   # Must start with sk_live_

[Config.PROXY]
PROXY    # Rotating proxy URL for gateway requests

[Config.DEFAULT_USER_VALUE]
ANTISPAM = 30   # Cooldown in seconds between commands
BALANCE  = 0    # Starting balance for new users
STATUS   = "FREE"  # Default tier: FREE / VIP / PREMIUM
```

## License

[MIT](LICENSE)
