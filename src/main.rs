use rand::Rng;
use serenity::{
    all::Member,
    async_trait,
    framework::standard::{
        macros::{command, group},
        CommandResult, Configuration, StandardFramework,
    },
    http::Http,
    model::{channel::Message, gateway::Ready, id::GuildId, id::RoleId, id::UserId},
    prelude::*,
};
use sqlx::mysql::MySqlPool;
use std::env;
use std::ops::Deref;

struct Handler;

impl Handler {
    async fn get_user_xp(user_id: u64, pool: &MySqlPool) -> (u32, u32) {
        // Returns (XP, Level)
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
        level - 1 // Subtract 1 because level will be one higher than the level for the given XP
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
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }

    // Include other event handler methods if necessary
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

#[group]
#[commands(ping, top)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "I'm still alive!").await?;
    Ok(())
}

#[command]
async fn top(ctx: &Context, msg: &Message) -> CommandResult {
    let data_read = ctx.data.read().await;
    let pool = data_read.get::<MySqlPoolContainer>().unwrap().pool.clone();

    let top_users = Handler::get_top_users(&pool).await;
    let mut response = String::new();

    for (index, (user_id, level)) in top_users.into_iter().enumerate() {
        let user_name = match ctx.http.get_user(user_id.into()).await {
            Ok(user) => user.name,
            Err(_) => format!("Unknown User ({})", user_id),
        };
        response.push_str(&format!(
            "{}. {} - Level: {}\n",
            index + 1,
            user_name,
            level
        ));
    }

    if !response.is_empty() {
        msg.channel_id.say(&ctx.http, &response).await?;
    } else {
        msg.channel_id.say(&ctx.http, "No users found.").await?;
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

    let beginner_role_id = env::var("BEGINNER_ROLE_ID")
        .expect("Expected BEGINNER_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("BEGINNER_ROLE_ID must be a valid u64");

    let rookie_role_id = env::var("ROOKIE_ROLE_ID")
        .expect("Expected ROOKIE_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("ROOKIE_ROLE_ID must be a valid u64");

    let intermediate_role_id = env::var("INTERMEDIATE_ROLE_ID")
        .expect("Expected INTERMEDIATE_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("INTERMEDIATE_ROLE_ID must be a valid u64");

    let advanced_role_id = env::var("ADVANCED_ROLE_ID")
        .expect("Expected ADVANCED_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("ADVANCED_ROLE_ID must be a valid u64");

    let expert_role_id = env::var("EXPERT_ROLE_ID")
        .expect("Expected EXPERT_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("EXPERT_ROLE_ID must be a valid u64");

    let master_role_id = env::var("MASTER_ROLE_ID")
        .expect("Expected MASTER_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("MASTER_ROLE_ID must be a valid u64");

    let elite_role_id = env::var("ELITE_ROLE_ID")
        .expect("Expected ELITE_ROLE_ID in the environment")
        .parse::<u64>()
        .expect("ELITE_ROLE_ID must be a valid u64");

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
    }

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

// You can add other commands and implementations as needed
