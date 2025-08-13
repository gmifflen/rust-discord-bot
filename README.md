# Rust Discord Bot

## Description

Discord bot written in Rust, using the [Poise](https://github.com/serenity-rs/poise/) framework<br />
It tracks message XP with level-ups, assigns roles by level, replies to “thanks” mentions, and supports reminders<br />
Command prefix is `!` but can also be @'ed instead

## Features

- **XP Tracking**: Automatically tracks and updates user xp when they send messages
- **Role Management**: Assigns roles to users based on their xp level
- **Commands**:
  - `!ping` – real round-trip latency
  - `!top` – top 10 users by level
  - `!mystats` – users level, XP progress, XP to next level
  - `!remindme "<text>" in <time>` – set a reminder in minutes/hours/days
  - `!help` – embed with commands
- **“Thanks” auto-reply**: Replies when users say thanks to the bot mention
   - i.e., "thanks @rust-discord-bot"

## Requirements

- Rust stable: <https://www.rust-lang.org/tools/install>
- MySQL or MariaDB: <https://dev.mysql.com/downloads/> or <https://mariadb.com/downloads/community/>

## Discord setup

1. Create a bot in the Discord Developer Portal and copy the **Bot Token**
2. Enable **Privileged Gateway Intents** for the bot:
   - `MESSAGE CONTENT` (required for reading messages)
   - `SERVER MEMBERS` (needed if you assign roles)
3. Invite the bot to your server with permissions to read/send messages and manage roles

## Dependencies
```toml
[dependencies]
poise = "0.6.1"
serenity = { version = "0.12.4", features = ["full"] }
tokio = { version = "1.47.1", features = ["full"] }
rand = "0.9.2"
sqlx = { version = "0.8.6", features = ["mysql", "runtime-tokio", "tls-rustls"] }
dotenvy = "0.15.7"
regex = "1.11.1"
chrono = "0.4.41"
tracing = "0.1.41"
tracing-subscriber = "0.3.19"
```

## Configuration

Edit the `.env` file in the project root:
```env
DATABASE_URL="mysql://USER:PASS@localhost:3306/discord_bot"
DISCORD_TOKEN=DISCORD_BOT_TOKEN
T1_ROLE_ID=ROLE_ID_1
T2_ROLE_ID=ROLE_ID_2
T3_ROLE_ID=ROLE_ID_3
T4_ROLE_ID=ROLE_ID_4
T5_ROLE_ID=ROLE_ID_5
T6_ROLE_ID=ROLE_ID_6
T7_ROLE_ID=ROLE_ID_7
```

## Database schema

```sql
-- XP storage
CREATE TABLE IF NOT EXISTS user_xp (
  user_id BIGINT UNSIGNED NOT NULL,
  xp      INT UNSIGNED NOT NULL DEFAULT 0,
  level   INT UNSIGNED NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Reminders
CREATE TABLE IF NOT EXISTS reminders (
  id            BIGINT NOT NULL AUTO_INCREMENT,
  user_id       BIGINT NOT NULL,
  channel_id    BIGINT NOT NULL,
  guild_id      BIGINT NOT NULL,
  reminder_text TEXT   NOT NULL,
  remind_at     DATETIME NOT NULL,
  PRIMARY KEY (id),
  KEY idx_reminders_due (remind_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
```

## Build and run

```fish
cargo build
cargo run
# for optimized binary/executable
cargo run --release
```

## Usage

* Add the bot to your server
* In any channel it can read:

  * `!ping` or `@<bot_name> ping`
  * `!top` or `@<bot_name> top`
  * `!mystats` or `@<bot_name> mystats`
  * `!remindme "drink water" in 30 minutes` or `@<bot_name>  "drink water" in 30 minutes`
     * Time parser supports `minutes|m`, `hours|h`, `days|d`.
  * `!help` or `@<bot_name> help`
* Level-up messages post automatically when thresholds are reached
* Role updates apply on level-up if the bot can **Manage Roles** and the target roles are **below** the bot’s highest role

## TODO
- [x] Move from [Serenity](https://github.com/serenity-rs/serenity/) to [Poise](https://github.com/serenity-rs/poise/)
- [ ] Switch to slash commands
- [ ] Break `main.rs` down into multiple modules,
    - At the very least move commands into their own directory
- [ ] Change `mystats` to a general `stats` to view other's stats too
- [ ] Implement ephemeral replies for certain commands
- [ ] Allow user to cancel `remindme` reminders
    - Will likely have to give IDs on creation, paired with ephemeral messages
- [ ] Cache for usernames in `top` with API calls being the fallback
- [ ] Implement role-gated commands
- [ ] Add SQLite support to minimize user installation requirements
    - Will be added alongside MySQL/MariaDB using Cargo features, rather than replacing them, to maintain compatibility
    - Will become the default backend
- [ ] Add unit tests
- [ ] Add PostgreSQL support; possibly replace MySQL/MariaDB altogether
- [ ] Improving `remindme` parsing
    - etc. `tomorrow 9am`, `next wed 14:30`, `in 90m`
    - look into NLP?

## Development

* Strict clippy linting is used:

  ```fish
  cargo clippy -- -W clippy::pedantic -W clippy::nursery -W clippy::unwrap_used -W clippy::expect_used
  ```

* Currently only built and tested on Windows

## Contributing

Issues and PRs are welcome; I'm maintaining this in my own time, so I may not be the fastest to merge

## License

GPLv3 ([COPYING](https://github.com/gmifflen/rust-discord-bot/blob/main/COPYING) or <https://spdx.org/licenses/GPL-3.0-only.html>)
