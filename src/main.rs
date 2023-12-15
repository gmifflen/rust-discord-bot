use rand::Rng;
use serenity::{
    async_trait,
    framework::standard::{
        macros::{command, group},
        CommandResult, Configuration, StandardFramework,
    },
    http::Http,
    model::{
        channel::Message,
        gateway::{GatewayIntents, Ready},
    },
    prelude::*,
};
use sqlx::mysql::MySqlPool;
use std::env;

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

    fn calculate_level(xp: u32) -> u32 {
        (0.1 * (xp as f64).sqrt()) as u32
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }

        let data_read = ctx.data.read().await;
        let pool = &data_read.get::<MySqlPoolContainer>().unwrap().pool;

        let user_id = msg.author.id.get() as u64;
        let xp_gain = rand::thread_rng().gen_range(1..=10);
        let (current_xp, current_level) = Handler::get_user_xp(user_id, &pool).await;
        let new_level = Handler::calculate_level(current_xp + xp_gain);

        Handler::update_user_xp_and_level(user_id, xp_gain, new_level, &pool).await;

        if new_level > current_level {
            if let Err(why) = msg
                .reply(
                    &ctx.http,
                    &format!("Congratulations! You've reached level {}!", new_level),
                )
                .await
            {
                println!("Error sending message: {:?}", why);
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct MySqlPoolContainer {
    pool: MySqlPool,
}

impl TypeMapKey for MySqlPoolContainer {
    type Value = MySqlPoolContainer;
}

#[group]
#[commands(ping)]
struct General;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "Pong!").await?;
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
    }

    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
