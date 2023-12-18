use chrono::{Duration, Timelike, Utc};
use rand::{seq::SliceRandom, Rng};
use serenity::{
    all::{Channel, Member},
    async_trait,
    builder::{CreateEmbed, CreateEmbedFooter, CreateMessage},
    framework::standard::{
        macros::{command, group},
        Args, CommandResult, Configuration, StandardFramework,
    },
    gateway::ShardManager,
    http::Http,
    model::{
        channel::Message,
        gateway::Ready,
        id::{ChannelId, GuildId, RoleId, UserId},
    },
    prelude::*,
};
use sqlx::mysql::MySqlPool;
use std::time::Duration as StdDuration;
use std::{env, ops::Deref, sync::Arc};

struct Handler;

impl Handler {
    async fn get_user_xp(user_id: u64, pool: &MySqlPool) -> (u32, u32) {
        let result =
            sqlx::query_as::<_, (u32, u32)>("SELECT xp, level FROM user_xp WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(pool)
                .await
                .unwrap()
                .unwrap_or((0, 0));
        result
    }

    async fn update_user_xp_and_level(
        user_id: u64,
        xp_gain: u32,
        new_level: u32,
        pool: &MySqlPool,
    ) {
        sqlx::query!(
            "INSERT INTO user_xp (user_id, xp, level) VALUES (?, ?, ?) ON DUPLICATE KEY UPDATE xp = xp + ?, level = ?",
            user_id,
            xp_gain,
            new_level,
            xp_gain,
            new_level,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    const BASE_XP: u32 = 50;
    const GROWTH_RATE: f64 = 1.15;

    fn calculate_xp_for_level(level: u32) -> u32 {
        (Self::BASE_XP as f64 * (1.0 + Self::GROWTH_RATE).powi(level as i32 - 1)) as u32
    }

    fn calculate_level(xp: u32) -> u32 {
        let mut level = 1;
        while Self::calculate_xp_for_level(level) <= xp {
            level += 1;
        }
        level - 1
    }

    async fn get_top_users(pool: &MySqlPool) -> Vec<(u64, u32)> {
        sqlx::query_as::<_, (u64, u32)>(
            "SELECT user_id, level FROM user_xp ORDER BY level DESC LIMIT 10",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_else(|_| vec![])
    }
}

async fn send_level_up_message(http: &Http, msg: &Message, new_level: u32) {
    if let Err(why) = msg
        .reply(
            http,
            &format!("Congratulations! You've reached level {}!", new_level),
        )
        .await
    {
        println!("Error sending message: {:?}", why);
    }
}

async fn update_user_roles(
    http: &Http,
    guild_id: GuildId,
    data: &TypeMap,
    user_id: UserId,
    new_level: u32,
) {
    let member = match guild_id.member(http, user_id).await {
        Ok(member) => member,
        Err(_) => return,
    };

    if let Some(roles) = data.get::<RoleIds>() {
        remove_existing_roles(http, &member, roles).await;
        assign_new_role(http, &member, roles, new_level).await;
    } else {
        println!("Error: RoleIds not found in context data.");
    }
}

async fn remove_existing_roles(http: &Http, member: &Member, roles: &RoleIds) {
    let all_role_ids = vec![
        RoleId::from(roles.beginner),
        RoleId::from(roles.rookie),
        RoleId::from(roles.intermediate),
        RoleId::from(roles.advanced),
        RoleId::from(roles.expert),
        RoleId::from(roles.master),
        RoleId::from(roles.elite),
    ];

    for role_id in all_role_ids {
        if member.roles.contains(&role_id) {
            if let Err(why) = member.remove_role(http, role_id).await {
                println!("Error removing role: {:?}", why);
            }
        }
    }
}

async fn assign_new_role(http: &Http, member: &Member, roles: &RoleIds, new_level: u32) {
    let new_role_id = match new_level {
        1..=5 => RoleId::from(roles.beginner),
        6..=10 => RoleId::from(roles.rookie),
        11..=20 => RoleId::from(roles.intermediate),
        21..=30 => RoleId::from(roles.advanced),
        31..=40 => RoleId::from(roles.expert),
        41..=50 => RoleId::from(roles.master),
        _ => RoleId::from(roles.elite),
    };

    if let Err(why) = member.add_role(http, new_role_id).await {
        println!("Error assigning role: {:?}", why);
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let user_id = u64::from(msg.author.id);
        let xp_gain = rand::thread_rng().gen_range(1..=10);

        if let Some(pool_container) = ctx.data.read().await.get::<MySqlPoolContainer>() {
            let pool = &pool_container.pool;
            let (current_xp, current_level) = Handler::get_user_xp(user_id, pool).await;
            let new_level = Handler::calculate_level(current_xp + xp_gain);
            Handler::update_user_xp_and_level(user_id, xp_gain, new_level, pool).await;

            if new_level > current_level {
                send_level_up_message(&ctx.http, &msg, new_level).await;
                if let Some(guild_id) = msg.guild_id {
                    let data = ctx.data.read().await;
                    update_user_roles(&ctx.http, guild_id, data.deref(), msg.author.id, new_level)
                        .await;
                }
            }
        } else {
            println!("Error: MySqlPoolContainer not found in context data.");
        }

        let responses = vec![
            "Glad to assist you, {user}!",
            "You're welcome, {user}, happy to help!",
            "Anytime, {user}, that's what I'm here for!",
            "No problem at all, {user}!",
            "Always here to provide the wisdom of the goblins!",
            "Your gratitude is noted and appreciated, {user}!",
            "I'm here whenever you need me, {user}!",
            "My goblin wisdom is at your service!",
            "It's my pleasure to assist!",
            "Don't mention it, {user}, I'm here to support you!",
        ];

        if msg.content.contains("thanks <@1184389136497512458>")
            || msg.content.contains("thanks @Professor Gizmo")
        {
            let should_mention = rand::random();

            let mut response = responses
                .choose(&mut rand::thread_rng())
                .unwrap()
                .to_string();

            if should_mention {
                response = response.replace("{user}", &msg.author.mention().to_string());
            } else {
                response = response.replace(", {user}", "");
            }

            if let Err(why) = msg.reply(&ctx.http, &*&mut *response).await {
                println!("Error sending reply: {:?}", why);
            }
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);

        tokio::spawn(async move {
            reminder_task(ctx.into()).await;
        });
    }
}
struct MySqlPoolContainer {
    pool: MySqlPool,
}

impl TypeMapKey for MySqlPoolContainer {
    type Value = MySqlPoolContainer;
}

struct RoleIds {
    beginner: u64,
    rookie: u64,
    intermediate: u64,
    advanced: u64,
    expert: u64,
    master: u64,
    elite: u64,
}

impl TypeMapKey for RoleIds {
    type Value = RoleIds;
}

struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

#[group]
#[commands(ping, top, next_reset, help, remindme)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;

    let shard_manager = match data.get::<ShardManagerContainer>() {
        Some(v) => v,
        None => {
            let _ = msg
                .reply(ctx, "There was a problem getting the shard manager")
                .await;
            return Ok(());
        }
    };

    let runners = shard_manager.runners.lock().await;

    let runner = match runners.get(&ctx.shard_id) {
        Some(runner) => runner,
        None => {
            let _ = msg.reply(ctx, "No shard found").await;
            return Ok(());
        }
    };

    let latency = runner.latency.map_or_else(
        || "N/A".to_string(),
        |duration| format!("I'm still Alive! Latency: {:.2?}", duration),
    );

    let _ = msg.reply(ctx, &latency).await;

    Ok(())
}

#[command]
async fn top(ctx: &Context, msg: &Message) -> CommandResult {
    let data_read = ctx.data.read().await;
    let pool = data_read.get::<MySqlPoolContainer>().unwrap().pool.clone();

    let top_users = Handler::get_top_users(&pool).await;

    let mut embed = CreateEmbed::default()
        .title("Top Users")
        .description("Here are the top users:")
        .color(0x00ff00); // You can change the color as per your preference

    for (index, (user_id, level)) in top_users.into_iter().enumerate() {
        let user = match ctx.http.get_user(user_id.into()).await {
            Ok(user) => user,
            Err(_) => continue, // Skip users that can't be fetched
        };

        let user_name = &user.name;

        // Add a field for each user, not inline
        embed = embed.field(
            format!("{}. {}", index + 1, user_name),
            format!("Level: {}", level),
            false, // Not inline
        );
    }

    // Create the message builder and attach the embed
    let builder = CreateMessage::new().embed(embed);

    // Send the message with the embed
    if let Err(why) = msg.channel_id.send_message(&ctx.http, builder).await {
        println!("Error sending message: {why:?}");
    }

    Ok(())
}

#[command]
async fn next_reset(ctx: &Context, msg: &Message) -> CommandResult {
    // Get the current UTC time
    let now = Utc::now();

    // Define the reset hours
    let reset_hours = vec![0, 6, 12, 18]; // 0 for 12:00 AM, 6 for 6:00 AM, etc.

    // Find the next reset hour
    let next_reset_hour = reset_hours
        .iter()
        .find(|&&hour| now.hour() < hour)
        .copied()
        .unwrap_or(0);

    // Calculate the next reset time
    let next_reset_time = if next_reset_hour > now.hour() {
        now.with_hour(next_reset_hour)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
    } else {
        // If the next reset hour is the first of the next day
        (now + Duration::days(1))
            .with_hour(next_reset_hour)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
    };

    // Calculate the duration until the next reset
    let duration_until_reset = next_reset_time - now;

    // Format the duration for display
    let hours_until_reset = duration_until_reset.num_hours();
    let minutes_until_reset = duration_until_reset.num_minutes() % 60; // Get remaining minutes

    // Create a response message
    let response = format!(
        "The next shop reset is in {} hours and {} minutes.",
        hours_until_reset, minutes_until_reset
    );

    // Send the response message
    if let Err(why) = msg.reply(&ctx.http, &response).await {
        println!("Error sending reply: {:?}", why);
    }

    Ok(())
}

#[command]
async fn remindme(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    let full_arg = args.rest();

    if let Some((reminder_text, time_string)) = full_arg.split_once(" in ") {
        match parse_time_string(time_string) {
            Ok(remind_at) => {
                // Continue with reminder setup
                println!("Reminder time parsed: {:?}", remind_at);

                let pool = {
                    let data_read = ctx.data.read().await;
                    data_read.get::<MySqlPoolContainer>().unwrap().pool.clone()
                };

                sqlx::query!(
                "INSERT INTO reminders (user_id, channel_id, guild_id, reminder_text, remind_at) VALUES (?, ?, ?, ?, ?)",
                i64::from(msg.author.id),
                i64::from(msg.channel_id),
                i64::from(msg.guild_id.unwrap()), // Assuming this command is only used in guilds
                reminder_text,
                &remind_at
            )
            .execute(&pool)
            .await
            .unwrap();

                msg.reply(
                    ctx,
                    &format!("I'll remind you about '{}' at {}", reminder_text, remind_at),
                )
                .await?;
            }
            Err(e) => {
                println!("Error parsing time string: {:?}", e);
                msg.reply(ctx, "Error parsing time.").await?;
            }
        }
    } else {
        msg.reply(ctx, "Please use the format: !remindme [reminder] in [time]")
            .await?;
    }

    Ok(())
}

fn parse_time_string(input: &str) -> Result<String, &'static str> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 2 {
        return Err("Time format should be 'X unit'");
    }

    let amount = parts[0].parse::<i64>().map_err(|_| "Invalid number")?;
    let unit = parts[1].to_lowercase();

    let duration = match unit.as_str() {
        "minute" | "minutes" | "m" => Duration::minutes(amount),
        "hour" | "hours" | "h" => Duration::hours(amount),
        "day" | "days" | "d" => Duration::days(amount),
        _ => return Err("Unknown time unit"),
    };

    let remind_at = Utc::now() + duration;
    Ok(remind_at.format("%Y-%m-%d %H:%M:%S").to_string())
}

async fn reminder_task(ctx: Arc<Context>) {
    let mut interval = tokio::time::interval(StdDuration::from_secs(60));
    loop {
        interval.tick().await;
        let context = Arc::clone(&ctx);
        tokio::spawn(async move {
            check_and_send_reminders(&context).await;
        });
    }
}

async fn check_and_send_reminders(ctx: &Context) {
    let data_read = ctx.data.read().await;
    let pool = data_read.get::<MySqlPoolContainer>().unwrap().pool.clone();

    let current_time = Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let reminders = sqlx::query!(
        "SELECT id, user_id, channel_id, reminder_text FROM reminders WHERE remind_at <= ?",
        current_time
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    for reminder in reminders {
        let channel_id = ChannelId::from(reminder.channel_id as u64);
        if let Ok(channel) = channel_id.to_channel(&ctx.http).await {
            if let Channel::Guild(guild_channel) = channel {
                if let Err(e) = guild_channel
                    .say(
                        &ctx.http,
                        format!("<@{}>: {}", reminder.user_id, reminder.reminder_text),
                    )
                    .await
                {
                    println!("Error sending reminder: {:?}", e);
                }
            }
        }

        sqlx::query!("DELETE FROM reminders WHERE id = ?", reminder.id)
            .execute(&pool)
            .await
            .unwrap();
    }
}

#[command]
async fn help(ctx: &Context, msg: &Message) -> CommandResult {
    // Create the footer for the embed
    let footer = CreateEmbedFooter::new("Use !command to run any of these commands.");

    // Create the embed
    let embed = CreateEmbed::new()
        .title("Help: List of Commands")
        .description("Here's a list of all the commands you can use:")
        .field("!ping", "Responds with latency of the bot.", false)
        .field("!top", "Shows the top users.", false)
        .field(
            "!next_reset",
            "Tells you how long until the next shop reset.",
            false,
        )
        .field(
            "!remindme",
            "Sets a reminder. Usage: !remindme \"<text>\" in <time>",
            false,
        )
        .field("!help", "Shows this message.", false)
        .footer(footer)
        .colour(0x00ff00); // You can set the embed color here

    // Create the message builder
    let builder = CreateMessage::new()
        .content("Commands available:")
        .embed(embed);

    // Send the message with the embed
    if let Err(why) = msg.channel_id.send_message(&ctx.http, builder).await {
        println!("Error sending help message: {why:?}");
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in the environment");
    let pool = MySqlPool::connect(&database_url)
        .await
        .expect("Could not connect to the database");

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let beginner_role_id = get_role_id("BEGINNER_ROLE_ID");
    let rookie_role_id = get_role_id("ROOKIE_ROLE_ID");
    let intermediate_role_id = get_role_id("INTERMEDIATE_ROLE_ID");
    let advanced_role_id = get_role_id("ADVANCED_ROLE_ID");
    let expert_role_id = get_role_id("EXPERT_ROLE_ID");
    let master_role_id = get_role_id("MASTER_ROLE_ID");
    let elite_role_id = get_role_id("ELITE_ROLE_ID");

    let http = Http::new(&token);

    let bot_id = match http.get_current_user().await {
        Ok(info) => info.id,
        Err(why) => panic!("Could not access the bot id: {:?}", why),
    };

    let framework = StandardFramework::new().group(&GENERAL_GROUP);

    framework.configure(
        Configuration::new()
            .with_whitespace(true)
            .on_mention(Some(bot_id))
            .prefix("!")
            .delimiters(vec![", ", ","]),
    );

    let intents = GatewayIntents::all();

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<MySqlPoolContainer>(MySqlPoolContainer { pool });
        data.insert::<RoleIds>(RoleIds {
            beginner: beginner_role_id,
            rookie: rookie_role_id,
            intermediate: intermediate_role_id,
            advanced: advanced_role_id,
            expert: expert_role_id,
            master: master_role_id,
            elite: elite_role_id,
        });
        data.insert::<ShardManagerContainer>(Arc::clone(&client.shard_manager));
    }

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

fn get_role_id(var_name: &str) -> u64 {
    env::var(var_name)
        .expect(&format!("Expected {} in the environment", var_name))
        .parse::<u64>()
        .expect(&format!("{} must be a valid u64", var_name))
}
