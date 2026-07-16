# KissShot RS

A Telegram bot for validating credit cards. Built in Rust with [teloxide](https://github.com/teloxide/teloxide).

Currently in super alpha.

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

| Command | Description |
|---------|-------------|
| `/start` `/help` `/cmds` | Bot info and command list |
| `/register` | Register an account |
| `/chk <card>` | Check card via Stripe |
| `/an <card>` | Check card via Authorize.net |
| `/pp <card>` | Check card via PayPal |
| `/sta <card>` | Check card via Stripe (alt) |
| `/vbv <card>` | VBV verification check |
| `/gen <bin>` | Generate cards from BIN |
| `/sk <key>` | Check a Stripe secret key |
| `/skgen` | Generate Stripe keys |
| `/skscr` | Scrape Stripe keys from Telegram |
| `/scr` | Scrape card numbers from Telegram |
| `/bin <number>` | BIN lookup |
| `/ip <address>` | IP geolocation |
| `/filter <text>` | Filter/validate card data |
| `/id` | Telegram user ID info |
| `/proxy` | Fetch working proxies |
| `/redeem <code>` | Redeem a gift code |
| `/authorize <user>` | Grant access _(admin)_ |
| `/deauthorize <user>` | Revoke access _(admin)_ |
| `/ban <user>` | Ban a user _(admin)_ |
| `/unban <user>` | Unban a user _(admin)_ |
| `/broadcast <text>` | Send message to all users _(admin)_ |
| `/upgrade <user>` | Upgrade user tier _(admin)_ |
| `/degrade <user>` | Downgrade user tier _(admin)_ |
| `/codegen` | Generate gift codes _(admin)_ |

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

All configuration lives in `config.toml`.

### Telegram

```toml
[Telegram.BOT]
API_ID       = "..."
API_HASH     = "..."
BOT_TOKEN    = "..."

[Telegram.USER]
API_ID       = "..."
API_HASH     = "..."
PHONE_NUMBER = "+..."
```

- **`BOT`** — Bot API credentials from [my.telegram.org](https://my.telegram.org). The bot token comes from BotFather.
- **`USER`** — User account credentials, used by the scraper plugins (`/scr`, `/skscr`) which log into a user Telegram account to scrape channels.

### Database

```toml
[Database.SQL]
URI = "mysql://user:pass@host:port/db?ssl-mode=REQUIRED"

[Database.REDIS]
URI = "rediss://default:pass@host:port"
```

- **`SQL`** — MySQL connection. Stores users, bans, sessions, and gift codes.
- **`REDIS`** — Redis or Valkey instance. Used for caching, rate limiting, and temporary key-value storage.

### Teloxide

```toml
[Teloxide]
WORKERS        = 10
TIMEOUT        = 30
REQUEST_CONCURRENCY = 100
MAX_NETWORK_RETRIES = 3
LOGGING        = true
```

- **`WORKERS`** — Number of async worker threads for handling updates.
- **`TIMEOUT`** — Seconds before a Telegram API request times out.
- **`REQUEST_CONCURRENCY`** — Max concurrent outgoing HTTP requests.
- **`MAX_NETWORK_RETRIES`** — How many times to retry a failed Telegram API call.
- **`LOGGING`** — Toggle verbose logging output.

### Bot Settings

```toml
[Config.BASIC]
ADMIN   = [766109755]
CHANNEL = "heckervault"
GROUP   = "heckervaultchat"
```

- **`ADMIN`** — Telegram user IDs that have admin access. Can hold multiple IDs.
- **`CHANNEL`** — Telegram channel username the bot checks for user membership (no `@` or `t.me/`).
- **`GROUP`** — Telegram group username the bot checks for user membership.

### Limits

```toml
[Config.LIMITS]
MAX_CC_SCR = 3001
MAX_SK_SCR = 3001
MAX_CC_CHK = 5
MAX_SK_CHK = 20
```

- **`MAX_CC_SCR`** — Max cards to scrape in a single `/scr` request.
- **`MAX_SK_SCR`** — Max Stripe keys to scrape in a single `/skscr` request.
- **`MAX_CC_CHK`** — Max cards a non-admin user can check at once with `/chk`.
- **`MAX_SK_CHK`** — Max Stripe keys a non-admin user can check at once with `/sk`.

### Regex Patterns

```toml
[Config.REGEX]
CC_REGEX = '[0-9]{16}[|][0-9]{1,2}[|][0-9]{2,4}[|][0-9]{3}'
SK_REGEX = 'sk_live_\\S+'
BIN_REGEX = '^[0-9]{16}$'
```

Used to validate input. Cards are expected in `NUMBER|MM|YY|CVV` format. Stripe keys must start with `sk_live_`.

### Other

```toml
[Config.GIFT_CODE]
PREFIX = "HeckerVault_"

[Config.PROXY]
PROXY = "http://user:pass@proxy:port/"

[Config.DEFAULT_USER_VALUE]
ANTISPAM = 30
BALANCE  = 0
STATUS   = "FREE"
```

- **`GIFT_CODE.PREFIX`** — String prepended to generated gift codes (e.g. `HeckerVault_abc123`).
- **`PROXY`** — Rotating proxy URL. Used by gateway plugins for making outbound check requests.
- **`DEFAULT_USER_VALUE`** — Defaults assigned to newly registered users. `ANTISPAM` is the cooldown in seconds between commands. `STATUS` is the access tier (`FREE`, `VIP`, or `PREMIUM`).

## License

[MIT](LICENSE)
