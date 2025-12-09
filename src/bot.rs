use std::sync::Arc;

use chrono::{Duration, Utc};
use teloxide::prelude::*;
use teloxide::types::{InputFile, ParseMode};
use teloxide::utils::command::BotCommands;
use uuid::Uuid;

use crate::state::App;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(description = "помощь")]
    Help,
    #[command(description = "создать ключ: gen <days> <user_id>", parse_with = "split")]
    Gen(i64, i64),
    #[command(description = "забанить ключ: ban <key>")]
    Ban(String),
    #[command(description = "разбанить ключ: unban <key>")]
    Unban(String),
    #[command(description = "инфо о ключе: info <key>")]
    Info(String),
    #[command(description = "статистика сервера")]
    Stats,
    #[command(description = "скачать бэкап базы")]
    Backup,
}

async fn update(app: Arc<App>, bot: Bot, msg: Message, cmd: Command) -> ResponseResult<()> {
    if !app.admins.contains(&msg.chat.id.0) {
        return Ok(());
    }

    let _ = bot.set_my_commands(Command::bot_commands()).await;

    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
        }
        Command::Gen(days, user_id) => {
            let key = Uuid::new_v4().to_string();
            let exp = (Utc::now() + Duration::days(days)).naive_utc();

            let insert = sqlx::query!(
                "INSERT INTO licenses (key, tg_user_id, expires_at) VALUES (?, ?, ?)",
                key,
                user_id,
                exp
            )
            .execute(&app.db)
            .await;

            if let Err(err) = insert {
                bot.send_message(msg.chat.id, format!("Error: {err:?}")).await?
            } else {
                bot.send_message(msg.chat.id, format!("Key: <code>{}</code>", key))
                    .parse_mode(ParseMode::Html)
                    .await?
            };
        }
        Command::Backup => {
            if let Err(err) = app.perform_backup(msg.chat.id).await {
                let _ = bot.send_document(msg.chat.id, InputFile::file("licenses.db")).await;
                bot.send_message(msg.chat.id, format!("Backup failed: {err:?}")).await?;
            }
        }

        Command::Ban(key) => {
            let _res = sqlx::query!("UPDATE licenses SET is_blocked = TRUE WHERE key = ?", key)
                .execute(&app.db)
                .await;
            app.sessions.remove(&key);
            bot.send_message(msg.chat.id, "🚫 Key blocked and sessions dropped").await?;
        }
        Command::Unban(key) => {
            let _ = sqlx::query!("UPDATE licenses SET is_blocked = FALSE WHERE key = ?", key)
                .execute(&app.db)
                .await;
            bot.send_message(msg.chat.id, "✅ Key unblocked").await?;
        }
        Command::Info(key) => {
            let active =
                if let Some(sessions) = app.sessions.get(&key) { sessions.len() } else { 0 };

            let lic = sqlx::query!(
                "SELECT tg_user_id, expires_at, is_blocked FROM licenses WHERE key = ?",
                key
            )
            .fetch_optional(&app.db)
            .await;

            match lic {
                Ok(Some(l)) => {
                    let status = if l.is_blocked { "⛔ BLOCKED" } else { "Active" };
                    let response = format!(
                        "🔑 <b>Key Info</b>\nOwner: <code>{}</code>\nExpires: {}\nStatus: {}\nActive Sessions: {}",
                        l.tg_user_id, l.expires_at, status, active
                    );
                    bot.send_message(msg.chat.id, response).parse_mode(ParseMode::Html).await?;
                }
                _ => {
                    bot.send_message(msg.chat.id, "Key not found").await?;
                }
            }
        }
        Command::Stats => {
            let active_keys = app.sessions.len();
            let total_sessions: usize = app.sessions.iter().map(|entry| entry.value().len()).sum();

            bot.send_message(
                msg.chat.id,
                format!(
                    "📊 <b>System Stats</b>\nActive Keys Online: {}\nTotal Sessions (Windows): {}",
                    active_keys, total_sessions
                ),
            )
            .parse_mode(ParseMode::Html)
            .await?;
        }
    };
    respond(())
}

pub async fn run_bot(app: Arc<App>) {
    let bot = app.bot.clone();
    let handler = Update::filter_message().filter_command::<Command>().endpoint(
        move |bot: Bot, msg: Message, cmd: Command| {
            let app = app.clone();
            update(app, bot, msg, cmd)
        },
    );

    Dispatcher::builder(bot, handler).enable_ctrlc_handler().build().dispatch().await;
}
