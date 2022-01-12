mod checks;
mod commands;
mod config;
mod error;
mod model;
mod utils;

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use harmony::client::{ClientBuilder, Context};
use harmony::gateway::{Intents, Ready};
use harmony::model::Message;
use parking_lot::RwLock;
use trueskill::SimpleTrueSkill;

use config::{Config, Roles};
pub use error::Error;
use model::{Database, Lobbies, Lobby, Ratings};

pub type Result<T = ()> = std::result::Result<T, Error>;

const REFRESH_DELAY: Duration = Duration::from_secs(60);

fn parse_command(msg: &str) -> Option<(String, Vec<String>)> {
    let mut it = msg.split_whitespace().map(|x| x.to_owned());
    let command = it.next()?;
    Some((command, it.collect()))
}

fn ready(ctx: Context, _: Ready, lobbies: Arc<RwLock<Lobbies>>, timeout: i64) {
    println!("Bot started");
    thread::spawn(move || {
        let timeout = chrono::Duration::minutes(timeout);
        loop {
            thread::sleep(REFRESH_DELAY);
            {
                let mut lobbies = lobbies.write();
                let limit = Utc::now() - timeout;
                for (&channel_id, lobby) in lobbies.iter_mut() {
                    let users = lobby
                        .queue()
                        .iter()
                        .filter_map(|(&user_id, &date_time)| {
                            if limit >= date_time {
                                Some(user_id)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    for user_id in users {
                        lobby.leave(user_id, true).ok();
                        ctx.send_message(channel_id, |m| {
                            m.content(user_id.mention()).embed(|e| {
                                e.description(format!(
                                    "[{}/{}] {} left the queue (Timeout).",
                                    lobby.len(),
                                    lobby.capacity(),
                                    user_id.mention()
                                ))
                            })
                        })
                        .ok();
                    }
                }
            }
        }
    });
}

fn message_create(
    ctx: Context,
    msg: Message,
    roles: &Roles,
    lobbies: Arc<RwLock<Lobbies>>,
    trueskill: &mut SimpleTrueSkill,
    database: &Database,
    initial_ratings: &HashMap<u64, f64>,
) {
    if let Some(content) = msg.content.strip_prefix('!') {
        if let Some((command, args)) = parse_command(content) {
            let result = match command.as_str() {
                "ping" => commands::ping(&ctx, &msg),
                "join" | "j" => commands::join(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    *trueskill,
                    database,
                ),
                "forcejoin" | "forcej" | "forceadd" => commands::forcejoin(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    *trueskill,
                    database,
                    &args,
                ),
                "leave" | "l" => {
                    commands::leave(&ctx, &msg, &mut lobbies.write(), *trueskill, database)
                }
                "forceleave" | "forcel" | "forceremove" => commands::forceleave(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    *trueskill,
                    database,
                    &args,
                ),
                "players" => commands::players(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    *trueskill,
                    database,
                    &args,
                ),
                "freeze" => commands::freeze(&ctx, &msg, roles, &mut lobbies.write()),
                "unfreeze" => commands::unfreeze(&ctx, &msg, roles, &mut lobbies.write()),
                "queue" | "q" => commands::queue(&ctx, &msg, &lobbies.read()),
                "score" | "g" => commands::score(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    *trueskill,
                    database,
                    initial_ratings,
                    &args,
                ),
                "cancel" => commands::cancel(&ctx, &msg, roles, &lobbies.read(), database, &args),
                "undo" | "unset" => commands::undo(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    database,
                    *trueskill,
                    initial_ratings,
                    &args,
                ),
                "gamelist" | "gl" => commands::gamelist(&ctx, &msg, &lobbies.read(), database),
                "gameinfo" | "gi" => {
                    commands::gameinfo(&ctx, &msg, &lobbies.read(), database, &args)
                }
                "clear" => commands::clear(&ctx, &msg, roles, &mut lobbies.write()),
                "rebalance" | "rb" => commands::rebalance(
                    &ctx,
                    &msg,
                    roles,
                    &mut lobbies.write(),
                    database,
                    *trueskill,
                ),
                "swap" => commands::swap(&ctx, &msg, roles, &lobbies.read(), database, &args),
                "info" => commands::info(&ctx, &msg, &lobbies.read(), &args),
                "forceinfo" => commands::forceinfo(&ctx, &msg, roles, &lobbies.read(), &args),
                _ => return,
            };
            if let Err(err) = result {
                ctx.send_message(msg.channel_id, |m| {
                    m.embed(|e| e.description(err.to_string()))
                })
                .ok();
            }
        }
    }
}

fn read_config<P: AsRef<Path>>(path: P) -> Config {
    let mut file = File::open(path).expect("Could not open config file");
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .expect("Could not read config file");
    serde_json::from_str(&buf).expect("Malformed config file")
}

fn main() {
    let token = env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN");
    let config = read_config("config.json");
    let database = Database::open(config.database).expect("Could not open database");
    let mut games = database.get_games().unwrap();
    let initials = database.get_initial_ratings().unwrap();
    let mut trueskill = config.trueskill;
    let lobbies = {
        let mut lobbies = Lobbies::default();
        for conf_lobby in config.lobbies {
            let ratings = Ratings::from_games(
                &games.remove(&conf_lobby.channel).unwrap_or_default(),
                &initials,
                trueskill,
            );
            let mut lobby = Lobby::new(conf_lobby.capacity, ratings);
            if let Some(webhook) = conf_lobby.webhook {
                lobby.set_webhook(webhook.id.into(), webhook.token);
            }
            lobbies.insert(conf_lobby.channel.into(), lobby);
        }
        Arc::new(RwLock::new(lobbies))
    };
    let roles = config.roles;
    let client = ClientBuilder::new()
        .with_bot_token(&token)
        .intents(Intents::GUILD_MESSAGES | Intents::DIRECT_MESSAGES)
        .on_ready(|ctx, rdy| ready(ctx, rdy, lobbies.clone(), config.timeout))
        .on_message_create(|ctx, msg| {
            message_create(
                ctx,
                msg,
                &roles,
                lobbies.clone(),
                &mut trueskill,
                &database,
                &initials,
            )
        })
        .build();
    if let Err(err) = client.run() {
        eprintln!("Error: {}", err);
    }
}
