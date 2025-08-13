#![forbid(unsafe_code)]
#![allow(clippy::cognitive_complexity)]
// cognitive complexity allowed for event/command since it can branch

use std::time::Instant;
use std::{env, sync::Arc, time::Duration as StdDuration};

use chrono::{Duration, Local, Utc};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{FullEvent, Mentionable};
use rand::{random_bool, random_range};
use serenity::{
    builder::{CreateEmbed, CreateEmbedFooter},
    http::Http,
    model::{
        channel::Message,
        id::{ChannelId, GuildId, RoleId, UserId},
    },
};
use sqlx::mysql::MySqlPool;
use tokio::time::interval;
use tracing::{error, warn};

/// shared error type for commands and handlers
type Error = Box<dyn std::error::Error + Send + Sync>;

/// poise command context alias to keep signatures tidy
type Ctx<'a> = poise::Context<'a, Data, Error>;

const COLOR_PRIMARY: u32 = 0x0094_A425;

/// guild role IDs for each level tier
#[derive(Clone)]
struct RoleIds {
    t1: u64,
    t2: u64,
    t3: u64,
    t4: u64,
    t5: u64,
    t6: u64,
    t7: u64,
}

/// bot-wide state shared by commands and events
struct Data {
    pool: MySqlPool,
    roles: RoleIds,
    http: Arc<Http>,
}

/**
*** xp and levels
**/
struct Xp;

impl Xp {
    /// the total xp required to complete the given level
    fn req_for(level: u32) -> u32 {
        let l = u64::from(level);
        let req = 5u64
            .saturating_mul(l.saturating_mul(l))
            .saturating_add(50u64.saturating_mul(l))
            .saturating_add(100);

        // clamp if it grew too large
        u32::try_from(req).map_or(u32::MAX, |v| v)
    }

    /// the amount of xp needed to hit the next level
    fn xp_to_next(level: u32, prog: u32) -> u32 {
        let need = Self::req_for(level);
        need.saturating_sub(prog)
    }

    /// apply xp gain to level and prog
    fn apply(mut level: u32, mut prog: u32, mut xp_gain: u32) -> (u32, u32, u32) {
        debug_assert!(xp_gain <= 100_000_000);
        let mut steps = 0u32;
        let mut leveled = 0u32;

        while xp_gain > 0 {
            steps = steps.saturating_add(1);
            if steps > 10_000 {
                break;
            }

            // if progress already covers the need, then normalize
            let need = Self::req_for(level);
            if prog >= need {
                prog = prog.saturating_sub(need);
                level = level.saturating_add(1);
                leveled = leveled.saturating_add(1);
                continue;
            }

            // fill current level or add partial progress
            let rem = need - prog;
            if xp_gain >= rem {
                xp_gain = xp_gain.saturating_sub(rem);
                prog = 0;
                level = level.saturating_add(1);
                leveled = leveled.saturating_add(1);
            } else {
                prog = prog.saturating_add(xp_gain);
                xp_gain = 0;
            }
        }

        (level, prog, leveled)
    }
}

/// get a user's `(xp, level)`
async fn get_user_xp(user_id: u64, pool: &MySqlPool) -> (u32, u32) {
    sqlx::query_as::<_, (u32, u32)>("SELECT xp, level FROM user_xp WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None)
        .unwrap_or((0, 0))
}

/// set a user's xp and level
async fn set_user_xp(user_id: u64, xp: u32, level: u32, pool: &MySqlPool) {
    if let Err(e) = sqlx::query!(
        "INSERT INTO user_xp (user_id, xp, level) VALUES (?, ?, ?)
         ON DUPLICATE KEY UPDATE xp = VALUES(xp), level = VALUES(level)",
        user_id,
        xp,
        level
    )
        .execute(pool)
        .await
    {
        error!("DB error updating user_xp for {user_id}: {e}");
    }
}

/// top 10 users sorted by level, then xp; desc
async fn top_users(pool: &MySqlPool) -> Vec<(u64, u32)> {
    sqlx::query_as::<_, (u64, u32)>(
        "SELECT user_id, level FROM user_xp ORDER BY level DESC, xp DESC LIMIT 10",
    )
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}

/**
*** roles
**/
/// update a member's level role based on `new_level`
async fn update_roles(
    http: &Http,
    guild_id: GuildId,
    user_id: UserId,
    roles: &RoleIds,
    new_level: u32,
) {
    let Ok(member) = guild_id.member(http, user_id).await else {
        return;
    };

    let all_roles = [
        roles.t1, roles.t2, roles.t3, roles.t4, roles.t5, roles.t6, roles.t7,
    ]
        .map(RoleId::new);

    for role in all_roles {
        if member.roles.contains(&role) {
            let res = member.remove_role(http, role).await;
            if let Err(e) = res {
                warn!("Remove role failed: {e}");
            }
        }
    }

    let target = match new_level {
        1..=5 => RoleId::new(roles.t1),
        6..=10 => RoleId::new(roles.t2),
        11..=20 => RoleId::new(roles.t3),
        21..=30 => RoleId::new(roles.t4),
        31..=40 => RoleId::new(roles.t5),
        41..=50 => RoleId::new(roles.t6),
        _ => RoleId::new(roles.t7),
    };

    if let Err(e) = member.add_role(http, target).await {
        warn!("Assign role failed: {e}");
    }
}

/**
*** thanks auto reply
**/
/// if a message thanks the bot, return a thanks
fn thanks_reply(msg: &Message) -> Option<String> {
    // TODO: put responses and triggers into their own config file
    let responses = [
        "Glad to assist you, {user}!",
        "You're welcome, {user}, happy to help!",
    ];
    let triggers = [
        "thanks <@rust_discord_bot_id>",
        "thanks @rust_discord_bot",
    ];

    let content = msg.content.to_lowercase();
    if !triggers.iter().any(|trigger| content.contains(trigger)) {
        return None;
    }

    let idx = random_range(0..responses.len());
    let base = responses[idx].to_string();

    let reply = if random_bool(0.5) {
        base.replace("{user}", &msg.author.mention().to_string())
    } else {
        base.replace(", {user}", "")
    };

    Some(reply)
}

/**
*** reminders
**/
/// parse strings like `10 minutes`, `2 h`, or `1 day` into a UTC timestamp string
fn parse_time_string(input: &str) -> Result<String, &'static str> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 2 {
        return Err("Time format should be 'X unit'");
    }

    let amount = parts[0].parse::<i64>().map_err(|_| "Invalid number")?;
    let unit = parts[1].to_lowercase();

    let dur = match unit.as_str() {
        "minute" | "minutes" | "m" => Duration::minutes(amount),
        "hour" | "hours" | "h" => Duration::hours(amount),
        "day" | "days" | "d" => Duration::days(amount),
        _ => return Err("Unknown time unit"),
    };

    let abs_time = Utc::now() + dur;

    Ok(abs_time.format("%Y-%m-%d %H:%M:%S").to_string())
}

/// background task, every 60s it:
/// - pulls due reminders
/// - sends them
/// - deletes them
async fn reminder_loop(pool: MySqlPool, http: Arc<Http>) {
    let mut ticker = interval(StdDuration::from_secs(60));

    loop {
        ticker.tick().await;

        let now = Utc::now()
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // reminders due at or before `now`
        let Ok(reminders) = sqlx::query!(
            "SELECT id, user_id, channel_id, reminder_text FROM reminders WHERE remind_at <= ?",
            now
        )
            .fetch_all(&pool)
            .await
        else {
            // hope the db hiccuped; try again on the next tick
            continue;
        };

        // send and delete each reminder
        for reminder in reminders {
            let Ok(ch_u64) = u64::try_from(reminder.channel_id) else {
                // skip rows with broken channel ids
                continue;
            };

            let ch = ChannelId::new(ch_u64);
            let text = format!("<@{}>: {}", reminder.user_id, reminder.reminder_text);

            if let Err(e) = ch.say(&http, text).await {
                warn!("Reminder send failed: {e}");
            }

            if let Err(e) = sqlx::query!("DELETE FROM reminders WHERE id = ?", reminder.id)
                .execute(&pool)
                .await
            {
                warn!("Reminder delete failed: {e}");
            }
        }

        debug_assert!(ticker.period() == StdDuration::from_secs(60));
    }
}

/**
*** commands
**/
/// `!ping`
/// sends a message, measures round-trip, and edits with the observed latency
#[poise::command(prefix_command)]
async fn ping(ctx: Ctx<'_>) -> Result<(), Error> {
    let start = Instant::now();
    let handle = ctx.say("Pinging...").await?;
    let elapsed = start.elapsed();

    let content = if elapsed.as_millis() < 1000 {
        format!("Ping took {}ms", elapsed.as_millis())
    } else {
        format!("Ping took {:.2}s", elapsed.as_secs_f64())
    };

    handle
        .edit(ctx, poise::CreateReply::default().content(content))
        .await?;

    Ok(())
}

/// `!top`
/// shows the top users by level, then xp
#[poise::command(prefix_command)]
async fn top(ctx: Ctx<'_>) -> Result<(), Error> {
    let users = top_users(&ctx.data().pool).await;

    let mut embed = CreateEmbed::default()
        .title("Top Users")
        .description("Here are the top users:")
        .color(COLOR_PRIMARY);

    // resolve usernames; if not, skip row
    for (idx, (uid, level)) in users.iter().enumerate() {
        let user_res = ctx.serenity_context().http.get_user((*uid).into()).await;
        if let Ok(u) = user_res {
            embed = embed.field(
                format!("{}. {}", idx + 1, u.name),
                format!("Level: {level}"),
                false,
            );
        }
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// `!mystats`
/// shows users normalized level and xp progress
#[poise::command(prefix_command)]
async fn mystats(ctx: Ctx<'_>) -> Result<(), Error> {
    let uid = ctx.author().id.get();

    let (xp, level) = get_user_xp(uid, &ctx.data().pool).await;
    let (norm_level, norm_xp, _) = Xp::apply(level, xp, 0);

    let need = Xp::req_for(norm_level);
    let xp_to_next = Xp::xp_to_next(norm_level, norm_xp);

    let embed = CreateEmbed::new()
        .title("Current Stats:")
        .field("Level", norm_level.to_string(), true)
        .field("XP Progress", format!("{norm_xp}/{need}"), true)
        .field("XP to Next Level", xp_to_next.to_string(), true)
        .color(COLOR_PRIMARY);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;

    Ok(())
}

/// `!remindme "<text>" in <time>`
/// example: `!remindme "stand up" in 10 minutes`
/// accepts minutes/m, hours/h, or days/d; stores times in UTC
#[poise::command(prefix_command)]
async fn remindme(ctx: Ctx<'_>, #[rest] full: String) -> Result<(), Error> {
    if let Some((reminder, time_str)) = full.split_once(" in ") {
        match parse_time_string(time_str) {
            Ok(at) => {
                let author = ctx.author().id;
                let ch = ctx.channel_id();
                let guild = ctx.guild_id().ok_or("Guild only")?;

                sqlx::query!(
                    "INSERT INTO reminders (user_id, channel_id, guild_id, reminder_text, remind_at)
                     VALUES (?, ?, ?, ?, ?)",
                    i64::from(author),
                    i64::from(ch),
                    i64::from(guild),
                    reminder,
                    &at
                )
                    .execute(&ctx.data().pool)
                    .await?;

                ctx.reply(format!("I'll remind you about '{reminder}' at {at}"))
                   .await?;
            }

            Err(_) => {
                ctx.reply("Error parsing time.").await?;
            }
        }
    } else {
        ctx.reply("Please use the format: !remindme \"[reminder]\" in [time]")
           .await?;
    }

    Ok(())
}

/// `!help`
/// lists available commands with brief descriptions
#[poise::command(prefix_command)]
async fn help(ctx: Ctx<'_>) -> Result<(), Error> {
    let footer = CreateEmbedFooter::new("Use !command to run any of these commands.");

    let embed = CreateEmbed::new()
        .title("Help: List of Commands")
        .description("Here's a list of all the commands you can use:")
        .field("!ping", "Responds with latency of the bot.", false)
        .field("!top", "Shows the top users.", false)
        .field("!mystats", "Shows your level and XP progress.", false)
        .field(
            "!remindme",
            "Sets a reminder. Usage: !remindme \"<text>\" in <time>",
            false,
        )
        .field("!help", "Shows this message.", false)
        .color(COLOR_PRIMARY)
        .footer(footer)
        .timestamp(Utc::now());

    ctx.send(
        poise::CreateReply::default()
            .content("Commands available:")
            .embed(embed),
    )
       .await?;

    Ok(())
}

/**
*** event handler for msg xp & thanks
**/
/// event hook for new messages
async fn on_event(
    _ctx: &serenity::Context,
    event: &FullEvent,
    _fw_ctx: poise::FrameworkContext<'_, Data, Error>,
    data: &Data,
) -> Result<(), Error> {
    if let FullEvent::Message { new_message } = event {
        let msg = new_message;

        // ignore bot messages
        if msg.author.bot {
            return Ok(());
        }

        let xp_gain: u32 = random_range(5..=10);
        let uid = msg.author.id.get();

        // normalize stored state, then apply xp gain
        let (stored_xp, stored_level) = get_user_xp(uid, &data.pool).await;
        let (norm_level, norm_xp, _) = Xp::apply(stored_level, stored_xp, 0);
        let (new_level, new_xp, levels) = Xp::apply(norm_level, norm_xp, xp_gain);

        set_user_xp(uid, new_xp, new_level, &data.pool).await;

        // if levels gained send a message and adjust roles
        if levels > 0 {
            let xp_left = Xp::xp_to_next(new_level, new_xp);

            let text = if levels == 1 {
                format!("Level up. You reached level {new_level}. {xp_left} XP to next level.")
            } else {
                format!(
                    "Massive gains. +{levels} levels. You are now level {new_level}. {xp_left} XP to next level."
                )
            };

            if let Err(e) = msg.reply(&data.http, text).await {
                warn!("Level-up reply failed: {e}");
            }

            if let Some(gid) = msg.guild_id {
                update_roles(&data.http, gid, msg.author.id, &data.roles, new_level).await;
            }
        }

        if let Some(reply) = thanks_reply(msg)
            && let Err(e) = msg.reply(&data.http, reply).await
        {
            warn!("Thanks reply failed: {e}");
        }
    }

    Ok(())
}

/**
*** main
**/
#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let database_url = env_var("DATABASE_URL")?;
    let pool = MySqlPool::connect(&database_url).await?;

    let token = env_var("DISCORD_TOKEN")?;

    let roles = RoleIds {
        t1: get_role_id("T1_ROLE_ID")?,
        t2: get_role_id("T2_ROLE_ID")?,
        t3: get_role_id("T3_ROLE_ID")?,
        t4: get_role_id("T4_ROLE_ID")?,
        t5: get_role_id("T5_ROLE_ID")?,
        t6: get_role_id("T6_ROLE_ID")?,
        t7: get_role_id("T7_ROLE_ID")?,
    };

    // register prefixed commands and attach the event handler
    let options = poise::FrameworkOptions {
        commands: vec![ping(), top(), mystats(), remindme(), help()],
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some("!".into()),
            ..Default::default()
        },
        event_handler: |ctx, ev, fw_ctx, data| Box::pin(on_event(ctx, ev, fw_ctx, data)),
        ..Default::default()
    };

    // intents needed for member and message content access
    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let http = Arc::new(Http::new(&token));
    let fw = poise::Framework::builder()
        .options(options)
        .setup(move |_ctx, ready, _| {
            let pool_clone = pool.clone();
            let http_clone = Arc::clone(&http);
            let roles_moved = roles;

            Box::pin(async move {
                let now = Local::now().format("[%Y-%m-%d-%H:%M]").to_string();

                println!("{} {} is connected!", now, ready.user.name);

                // detached background task for reminders
                tokio::spawn(reminder_loop(pool_clone.clone(), http_clone.clone()));

                Ok(Data {
                    pool: pool_clone,
                    roles: roles_moved,
                    http: http_clone,
                })
            })
        })
        .build();

    let mut client = serenity::Client::builder(&token, intents)
        .framework(fw)
        .await?;

    if let Err(why) = client.start().await {
        eprintln!("Client error: {why:?}");
    }

    Ok(())
}

/// read in an env var
fn env_var(var: &str) -> Result<String, Error> {
    env::var(var).map_err(|e| format!("Missing env {var}: {e}").into())
}

// parse a role id from an env var
fn get_role_id(var: &str) -> Result<u64, Error> {
    let s = env_var(var)?;
    let v = s
        .parse::<u64>()
        .map_err(|e| format!("Invalid {var}: {e}"))?;
    Ok(v)
}
