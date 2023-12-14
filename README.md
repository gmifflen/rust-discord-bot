# Rust Discord Bot

## Description

This project is a Discord bot written in Rust, using the Serenity framework. It's designed to interact with users in a Discord server, managing user XP based on message activity, and handling role assignments based on XP levels.

## Features

- **XP Tracking**: Automatically tracks and updates user XP when they send messages.
- **Role Management**: Assigns roles to users based on their XP level.
- **Commands**: Responds to specific commands such as `!ping` and `!top`.

## Installation

### Prerequisites

- Rust Programming Language: [Install Rust](https://www.rust-lang.org/tools/install)
- SQLx CLI: Used for handling database migrations.
- MySQL Database: The bot uses a MySQL database to store user data.

### Setup

1. **Clone the Repository**

   ```bash
   git clone https://github.com/gmifflen/rust-discord-bot.git
   cd rust-discord-bot
   ```

2. **Environment Variables**
   Create a `.env` file in the root directory with the following content:

   ```
   DISCORD_TOKEN=your_discord_bot_token
   DATABASE_URL=mysql://username:password@localhost/discord_bot
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   XYZ_ROLE_ID=123456789
   ```

3. **Database Setup**
   Ensure your MySQL database is running and use SQLx CLI to set up the database schema.

   ```SQL
    CREATE TABLE `user_xp` (
      `user_id` bigint unsigned NOT NULL,
      `xp` int unsigned DEFAULT NULL,
      `level` int unsigned DEFAULT NULL,
      PRIMARY KEY (`user_id`)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
   ```

   ```bash
    cargo sqlx prepare
   ```

4. **Build the Project**
   ```bash
   cargo build
   ```

## Usage

To run the bot:

```bash
cargo run
```

The bot will start on any server it has been added to Discord server. It will begin listening for messages and commands.

## Contributing

I'm not the best at Rust, this is my third public Rust project.
Any suggestions on improvements or ways to write it in a better/safer way is much appreciated.

## License

[MIT](https://choosealicense.com/licenses/mit/)
