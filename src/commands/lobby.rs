use std::collections::HashSet;

use chrono::{DateTime, Utc};
use harmony::client::Context;
use harmony::model::id::{ChannelId, GuildId, UserId};
use harmony::model::{Member, Message};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator,
};
use serde_json::json;
use trueskill::SimpleTrueSkill;

use crate::bridge::{GameStarted, OpCode};
use crate::checks;
use crate::config::{Rank, Roles};
use crate::model::{Database, Game, Lobbies, Ratings, Score};
use crate::utils;
use crate::{Error, Result};

pub fn join(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    bridge: ChannelId,
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if checks::has_role(ctx, guild_id, msg.author.id, roles.banned)? {
        return Ok(());
    }
    join_internal(
        ctx,
        guild_id,
        msg.channel_id,
        bridge,
        msg.author.id,
        msg.timestamp,
        false,
        lobbies,
        trueskill,
        database,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn forcejoin(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    bridge: ChannelId,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let members = args
        .iter()
        .map(|arg| match Member::parse(ctx, guild_id, arg) {
            Ok(Some(x)) => Ok(x),
            Ok(None) => Err(Error::MemberNotFound(arg.clone())),
            Err(err) => Err(err.into()),
        })
        .collect::<Result<Vec<_>>>()?;
    for member in members {
        join_internal(
            ctx,
            guild_id,
            msg.channel_id,
            bridge,
            member.user.id,
            msg.timestamp,
            true,
            lobbies,
            trueskill,
            database,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn join_internal(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    bridge: ChannelId,
    user_id: UserId,
    timestamp: DateTime<Utc>,
    force: bool,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let lobby = lobbies
        .get_mut(&channel_id)
        .ok_or(Error::NotALobby(channel_id))?;
    lobby.join(user_id, timestamp, force)?;
    ctx.send_message(channel_id, |m| {
        m.embed(|e| {
            e.description(format!(
                "[{}/{}] {} joined the queue.",
                lobby.len(),
                lobby.capacity(),
                user_id.mention()
            ))
        })
    })?;
    if lobby.len() == lobby.capacity() {
        let players = lobby.clear().into_keys().collect();
        start_game(
            ctx, guild_id, channel_id, bridge, lobbies, players, trueskill, database,
        )?;
    }
    Ok(())
}

pub fn leave(
    ctx: &Context,
    msg: &Message,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    bridge: ChannelId,
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    leave_internal(
        ctx,
        guild_id,
        msg.channel_id,
        bridge,
        msg.author.id,
        false,
        lobbies,
        trueskill,
        database,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn forceleave(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    bridge: ChannelId,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let members = args
        .iter()
        .map(|arg| match Member::parse(ctx, guild_id, arg) {
            Ok(Some(x)) => Ok(x),
            Ok(None) => Err(Error::MemberNotFound(arg.clone())),
            Err(err) => Err(err.into()),
        })
        .collect::<Result<Vec<_>>>()?;
    for member in members {
        leave_internal(
            ctx,
            guild_id,
            msg.channel_id,
            bridge,
            member.user.id,
            true,
            lobbies,
            trueskill,
            database,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn leave_internal(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    bridge: ChannelId,
    user_id: UserId,
    force: bool,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let players = {
        let lobby = lobbies
            .get_mut(&channel_id)
            .ok_or(Error::NotALobby(channel_id))?;
        lobby.leave(user_id, force)?;
        ctx.send_message(channel_id, |m| {
            m.embed(|e| {
                e.description(format!(
                    "[{}/{}] {} left the queue.",
                    lobby.len(),
                    lobby.capacity(),
                    user_id.mention()
                ))
            })
        })?;
        if lobby.len() == lobby.capacity() {
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(
            ctx, guild_id, channel_id, bridge, lobbies, players, trueskill, database,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn players(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    bridge: ChannelId,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let players = {
        let lobby = lobbies
            .get_mut(&msg.channel_id)
            .ok_or(Error::NotALobby(msg.channel_id))?;
        let x = args[0].parse::<usize>()?;
        lobby.set_capacity(2 * x);
        ctx.send_message(msg.channel_id, |m| {
            m.embed(|e| e.description(format!("Players per team set to {}.", x)))
        })?;
        if lobby.len() == lobby.capacity() {
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(
            ctx,
            guild_id,
            msg.channel_id,
            bridge,
            lobbies,
            players,
            trueskill,
            database,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn start_game(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    bridge: ChannelId,
    lobbies: &mut Lobbies,
    players: Vec<UserId>,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let lobby_name = lobbies.get(&channel_id).unwrap().name().to_owned();
    let players = players
        .into_iter()
        .map(|x| {
            (
                x,
                lobbies[&channel_id]
                    .ratings()
                    .get(&x)
                    .map(|x| x.rating)
                    .unwrap_or_else(|| trueskill.create_rating()),
            )
        })
        .collect::<Vec<_>>();
    let teams = utils::balance(&players);
    let quality = utils::quality(&teams, trueskill);
    let mut game = Game::create(
        teams[0].iter().map(|x| x.0).collect(),
        teams[1].iter().map(|x| x.0).collect(),
        Utc::now(),
    );
    database.insert_game(&mut game, channel_id)?;
    let f = |users: &[(UserId, _)]| {
        users
            .iter()
            .map(|x| x.0.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let title = format!("Game {} started", game.id());
    let description = format!(
        "Quality: {:.0}\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        100.0 * quality,
        f(&teams[0]),
        f(&teams[1])
    );
    rayon::scope(|s| {
        // Send game started message
        s.spawn(|_| {
            let content = teams[0]
                .iter()
                .chain(teams[1].iter())
                .map(|(x, _)| x.mention())
                .collect::<Vec<_>>()
                .join(" ");
            let message = match ctx.send_message(channel_id, |m| {
                m.content(content).embed(|e| {
                    e.title(title)
                        .description(description)
                        .timestamp(game.datetime())
                })
            }) {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("Err: {:?}", err);
                    return;
                }
            };
            teams[0]
                .par_iter()
                .chain(teams[1].par_iter())
                .for_each(|(user_id, _)| {
                    if let Err(err) = (|| {
                        let channel = ctx.create_dm(*user_id)?;
                        ctx.send_message(channel.id, |m| {
                            m.content(format!("Game started: {}", message.channel_id.mention()))
                        })?;
                        Result::Ok(())
                    })() {
                        eprintln!("Err: {:?}", err);
                    }
                });
        });
        // Create global game role
        s.spawn(|_| {
            if let Err(err) = (|| {
                let role_id = ctx
                    .create_guild_role(guild_id, |r| {
                        r.name(format!("{} Game {}", lobby_name, game.id()))
                            .mentionable(true)
                            .hoist(true)
                    })?
                    .id;
                teams[0]
                    .par_iter()
                    .chain(teams[1].par_iter())
                    .for_each(|(user_id, _)| {
                        if let Err(err) = ctx.add_guild_member_role(guild_id, *user_id, role_id) {
                            eprintln!("Err: {:?}", err);
                        }
                    });
                Result::Ok(())
            })() {
                eprintln!("Err: {:?}", err);
            }
        });
        // Create team 1 role
        s.spawn(|_| {
            if let Err(err) = (|| {
                let role_id = ctx
                    .create_guild_role(guild_id, |r| {
                        r.name(format!("{} Game {} Team 1", lobby_name, game.id()))
                            .mentionable(true)
                            .hoist(true)
                    })?
                    .id;
                teams[0].par_iter().for_each(|(user_id, _)| {
                    if let Err(err) = ctx.add_guild_member_role(guild_id, *user_id, role_id) {
                        eprintln!("Err: {:?}", err);
                    }
                });
                Result::Ok(())
            })() {
                eprintln!("Err: {:?}", err);
            }
        });
        // Create team 2 role
        s.spawn(|_| {
            if let Err(err) = (|| {
                let role_id = ctx
                    .create_guild_role(guild_id, |r| {
                        r.name(format!("{} Game {} Team 2", lobby_name, game.id()))
                            .mentionable(true)
                            .hoist(true)
                    })?
                    .id;
                teams[1].par_iter().for_each(|(user_id, _)| {
                    if let Err(err) = ctx.add_guild_member_role(guild_id, *user_id, role_id) {
                        eprintln!("Err: {:?}", err);
                    }
                });
                Result::Ok(())
            })() {
                eprintln!("Err: {:?}", err);
            }
        });
        // Send "GAME_STARTED" message to bridge
        s.spawn(|_| {
            let bridge_event = GameStarted {
                players: game.teams()[0]
                    .iter()
                    .copied()
                    .chain(game.teams()[1].iter().copied())
                    .collect(),
            };
            if let Err(err) = ctx.send_message(bridge, |m| {
                m.content(json!({
                    "t": OpCode::GameStarted,
                    "d": bridge_event,
                }))
            }) {
                eprintln!("Err: {:?}", err);
            }
        });
        // Remove players from other lobbies
        lobbies.par_iter_mut().for_each(|(channel_id, lobby)| {
            for (user_id, _) in players.iter() {
                if lobby.leave(*user_id, true).is_ok() {
                    if let Err(err) = ctx.send_message(*channel_id, |m| {
                        m.embed(|e| {
                            e.description(format!(
                                "[{}/{}] {} left the queue (Game started).",
                                lobby.len(),
                                lobby.capacity(),
                                user_id.mention(),
                            ))
                        })
                    }) {
                        eprintln!("Err: {:?}", err);
                    }
                }
            }
        });
    });
    Ok(())
}

pub fn freeze(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.freeze();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue frozen."))
    })?;
    Ok(())
}

pub fn unfreeze(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.unfreeze();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue unfrozen."))
    })?;
    Ok(())
}

pub fn queue(ctx: &Context, msg: &Message, lobbies: &Lobbies) -> Result {
    let lobby = lobbies
        .get(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    let description = lobby
        .queue()
        .keys()
        .map(|x| x.mention())
        .collect::<Vec<_>>()
        .join("\n");
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(format!("Queue [{}/{}]", lobby.len(), lobby.capacity()))
                .description(description)
        })
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn score(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    ranks: &[Rank],
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    let lobby_name = lobby.name().to_owned();
    if args.len() < 2 {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let score = match args[1].to_lowercase().as_ref() {
        "1" => Score::Team1,
        "2" => Score::Team2,
        "draw" | "d" => Score::Draw,
        _ => return Err(Error::BadArgument),
    };
    let mut game = match database.get_game(msg.channel_id.0, game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    game.set_score(score);
    database.update_game(&game, msg.channel_id)?;
    let teams = game.teams();
    let default_rating = trueskill.create_rating();
    let old_ratings: [Vec<f64>; 2] = [
        teams[0]
            .iter()
            .map(|x| {
                lobby
                    .ratings()
                    .get(x)
                    .map(|x| x.rating)
                    .unwrap_or(default_rating)
                    .mean()
            })
            .collect(),
        teams[1]
            .iter()
            .map(|x| {
                lobby
                    .ratings()
                    .get(x)
                    .map(|x| x.rating)
                    .unwrap_or(default_rating)
                    .mean()
            })
            .collect(),
    ];
    let initial_ratings = database.get_initial_ratings()?;
    let games = database
        .get_games()?
        .remove(&msg.channel_id)
        .unwrap_or_default();
    let ratings = Ratings::from_games(&games, &initial_ratings, trueskill);
    lobby.set_ratings(ratings);
    let new_ratings: [Vec<f64>; 2] = [
        teams[0]
            .iter()
            .map(|x| {
                lobby
                    .ratings()
                    .get(x)
                    .map(|x| x.rating)
                    .unwrap_or(default_rating)
                    .mean()
            })
            .collect(),
        teams[1]
            .iter()
            .map(|x| {
                lobby
                    .ratings()
                    .get(x)
                    .map(|x| x.rating)
                    .unwrap_or(default_rating)
                    .mean()
            })
            .collect(),
    ];
    let leaderboard = utils::leaderboard(lobby, 15, ranks, |user_id| {
        checks::has_role(ctx, guild_id, user_id, roles.ranked)
    })?;
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        messages.par_iter().for_each(|&message| {
            if let Err(err) = ctx.webhook_delete_message(*webhook_id, webhook_token, message) {
                eprintln!("Err: {:?}", err);
            }
        });
        messages.clear();
        for (title, description) in leaderboard.iter() {
            let message = ctx.execute_webhook(*webhook_id, webhook_token, true, |m| {
                m.embed(|e| e.description(description).title(title))
            });
            let message = match message {
                Ok(message) => message,
                Err(err) => {
                    eprintln!("Err: {:?}", err);
                    continue;
                }
            };
            if let Some(message) = message {
                messages.push(message.id);
            }
        }
    }
    rayon::scope(|s| {
        s.spawn(|_| {
            if let Err(err) = (|| {
                ctx.get_guild_roles(guild_id)?.par_iter().for_each(|role| {
                    if role
                        .name
                        .contains(&format!("{} Game {}", lobby_name, game_id))
                    {
                        if let Err(err) = ctx.delete_guild_role(guild_id, role.id) {
                            eprintln!("Err: {:?}", err);
                        }
                    }
                });
                Result::Ok(())
            })() {
                eprintln!("Err: {:?}", err);
            }
        });
        s.spawn(|_| {
            game.teams()[0].par_iter().for_each(|&user_id| {
                match checks::has_role(ctx, guild_id, user_id, roles.ranked) {
                    Ok(false) => return,
                    Err(err) => {
                        eprintln!("Err: {:?}", err);
                        return;
                    }
                    _ => (),
                }
                let rating = lobbies
                    .iter()
                    .map(|(_, x)| {
                        x.ratings()
                            .get(&user_id)
                            .map(|y| y.rating.mean())
                            .unwrap_or_default()
                    })
                    .fold(0f64, |acc, x| acc.max(x));
                let rank_index = ranks
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, x)| rating >= x.limit)
                    .map(|x| x.0 + 1)
                    .unwrap_or_default()
                    .min(ranks.len() - 1);
                if let Err(err) = ctx.add_guild_member_role(guild_id, user_id, ranks[rank_index].id)
                {
                    eprintln!("Err: {:?}", err);
                }
                if rank_index > 0 && score != Score::Team2 {
                    if let Err(err) =
                        ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index - 1].id)
                    {
                        eprintln!("Err: {:?}", err);
                    }
                }
                if rank_index + 1 < ranks.len() && score != Score::Team1 {
                    if let Err(err) =
                        ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index + 1].id)
                    {
                        eprintln!("Err: {:?}", err);
                    }
                }
            });
        });
        s.spawn(|_| {
            game.teams()[1].par_iter().for_each(|&user_id| {
                match checks::has_role(ctx, guild_id, user_id, roles.ranked) {
                    Ok(false) => return,
                    Err(err) => {
                        eprintln!("Err: {:?}", err);
                        return;
                    }
                    _ => (),
                }
                let rating = lobbies
                    .iter()
                    .map(|(_, x)| {
                        x.ratings()
                            .get(&user_id)
                            .map(|y| y.rating.mean())
                            .unwrap_or_default()
                    })
                    .fold(0f64, |acc, x| acc.max(x));
                let rank_index = ranks
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, x)| rating >= x.limit)
                    .map(|x| x.0 + 1)
                    .unwrap_or_default();
                if let Err(err) = ctx.add_guild_member_role(guild_id, user_id, ranks[rank_index].id)
                {
                    eprintln!("Err: {:?}", err);
                }
                if rank_index > 0 && score != Score::Team1 {
                    if let Err(err) =
                        ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index - 1].id)
                    {
                        eprintln!("Err: {:?}", err);
                    }
                }
                if rank_index + 1 < ranks.len() && score != Score::Team2 {
                    if let Err(err) =
                        ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index + 1].id)
                    {
                        eprintln!("Err: {:?}", err);
                    }
                }
            });
        });
        s.spawn(|_| {
            let f = |users: &[UserId], old_ratings: &[f64], new_ratings: &[f64]| {
                users
                    .par_iter()
                    .zip(old_ratings.par_iter().zip(new_ratings.par_iter()))
                    .map(|(x, (old, new))| {
                        let old_rank = utils::get_rank(ranks, *old);
                        let new_rank = utils::get_rank(ranks, *new);
                        if let Ok(true) = checks::has_role(ctx, guild_id, *x, roles.ranked) {
                            let rank_update = if old_rank.id != new_rank.id {
                                format!("{} => {}", old_rank.id.mention(), new_rank.id.mention())
                            } else {
                                "".to_owned()
                            };
                            if new >= old {
                                format!(
                                    "{} {:.0} + {:.0} = {:.0} {}",
                                    x.mention(),
                                    old,
                                    new - old,
                                    new,
                                    rank_update,
                                )
                            } else {
                                format!(
                                    "{} {:.0} - {:.0} = {:.0} {}",
                                    x.mention(),
                                    old,
                                    old - new,
                                    new,
                                    rank_update,
                                )
                            }
                        } else if new >= old {
                            format!("{} +{:.0}", x.mention(), new - old,)
                        } else {
                            format!("{} -{:.0}", x.mention(), old - new,)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let description = format!(
                "**{}**\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
                score,
                f(teams[0], &old_ratings[0], &new_ratings[0]),
                f(teams[1], &old_ratings[1], &new_ratings[1])
            );
            if let Err(err) = ctx.send_message(msg.channel_id, |m| {
                m.embed(|e| {
                    e.description(description)
                        .title(format!("Game {}", game_id))
                })
            }) {
                eprintln!("Err: {:?}", err);
            }
        });
    });
    Ok(())
}

pub fn cancel(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &Lobbies,
    database: &Database,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = if let Some(lobby) = lobbies.get(&msg.channel_id) {
        lobby
    } else {
        return Err(Error::NotALobby(msg.channel_id));
    };
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let mut game = database.get_game(msg.channel_id.into(), game_id)?;
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    game.set_score(Score::Cancelled);
    database.update_game(&game, msg.channel_id)?;
    ctx.get_guild_roles(guild_id)?.par_iter().for_each(|role| {
        if role
            .name
            .contains(&format!("{} Game {}", lobby.name(), game_id))
        {
            if let Err(err) = ctx.delete_guild_role(guild_id, role.id) {
                eprintln!("Err: {:?}", err);
            }
        }
    });
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description(format!("Game {} cancelled.", game_id)))
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn undo(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    ranks: &[Rank],
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let initial_ratings = database.get_initial_ratings()?;
    let mut game = match database.get_game(msg.channel_id.0, game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    let prev_score = game.score();
    game.set_score(Score::Undecided);
    database.update_game(&game, msg.channel_id)?;
    let games = database
        .get_games()?
        .remove(&msg.channel_id)
        .unwrap_or_default();
    let ratings = Ratings::from_games(&games, &initial_ratings, trueskill);
    lobby.set_ratings(ratings);
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description(format!("Game {} undone.", game_id)))
    })?;
    if prev_score == Score::Cancelled || prev_score == Score::Undecided {
        return Ok(());
    }
    let leaderboard = utils::leaderboard(lobby, 15, ranks, |user_id| {
        checks::has_role(ctx, guild_id, user_id, roles.ranked)
    })?;
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        messages.par_iter().for_each(|&message| {
            if let Err(err) = ctx.webhook_delete_message(*webhook_id, webhook_token, message) {
                eprintln!("Err: {:?}", err);
            }
        });
        messages.clear();
        for (title, description) in leaderboard.iter() {
            let message = ctx.execute_webhook(*webhook_id, webhook_token, true, |m| {
                m.embed(|e| e.description(description).title(title))
            })?;
            if let Some(message) = message {
                messages.push(message.id);
            }
        }
    }
    Ok(())
}

pub fn gamelist(ctx: &Context, msg: &Message, lobbies: &Lobbies, database: &Database) -> Result {
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    let games = database
        .get_games()?
        .remove(&msg.channel_id)
        .unwrap_or_default();
    if games.is_empty() {
        return Err(Error::GameNotFound(1));
    }
    let description = games
        .into_iter()
        .rev()
        .take(20)
        .map(|(id, game)| format!("Game {}: {}", id, game.score()))
        .collect::<Vec<_>>()
        .join("\n");
    ctx.send_message(msg.channel_id, |m| m.embed(|e| e.description(description)))?;
    Ok(())
}

pub fn lastgame(ctx: &Context, msg: &Message, lobbies: &Lobbies, database: &Database) -> Result {
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    let games = database.get_games()?;
    let game = match games.get(&msg.channel_id).and_then(|x| x.values().last()) {
        Some(game) => game,
        None => return Err(Error::GameNotFound(1)),
    };
    let title = format!("Game {}", game.id());
    let f = |users: &[UserId]| {
        users
            .iter()
            .map(|x| x.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "**{}**\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        game.score(),
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    Ok(())
}

pub fn gameinfo(
    ctx: &Context,
    msg: &Message,
    lobbies: &Lobbies,
    database: &Database,
    args: &[String],
) -> Result {
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
    if args.is_empty() {
        return Err(Error::NotEnoughArguments);
    }
    let game_id = args[0].parse()?;
    let game = match database.get_game(msg.channel_id.into(), game_id) {
        Ok(game) => game,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Err(Error::GameNotFound(game_id)),
        Err(err) => return Err(err.into()),
    };
    let title = format!("Game {}", game.id());
    let f = |users: &[UserId]| {
        users
            .iter()
            .map(|x| x.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "**{}**\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        game.score(),
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    Ok(())
}

pub fn clear(ctx: &Context, msg: &Message, roles: &Roles, lobbies: &mut Lobbies) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    lobby.clear();
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Queue cleared"))
    })?;
    Ok(())
}

pub fn rebalance(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = lobbies
        .get_mut(&msg.channel_id)
        .ok_or(Error::NotALobby(msg.channel_id))?;
    let mut game = match database.get_games()?.remove(&msg.channel_id) {
        Some(games) => {
            if let Some(game) = games.into_iter().last() {
                game.1
            } else {
                return Ok(());
            }
        }
        None => return Ok(()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    let players = game
        .teams()
        .into_iter()
        .flatten()
        .map(|x| {
            (
                x,
                lobby
                    .ratings()
                    .get(x)
                    .map(|x| x.rating)
                    .unwrap_or_else(|| trueskill.create_rating()),
            )
        })
        .collect::<Vec<_>>();
    let teams = utils::balance(&players);
    let quality = utils::quality(&teams, trueskill);
    let team1 = teams[0].iter().map(|x| x.0).copied().collect::<Vec<_>>();
    let team2 = teams[1].iter().map(|x| x.0).copied().collect::<Vec<_>>();
    game.set_teams([team1, team2]);
    database.update_game(&game, msg.channel_id)?;
    let title = format!("Game {}", game.id());
    let f = |users: &[UserId]| {
        users
            .iter()
            .map(|x| x.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let description = format!(
        "Quality: {:.0}\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        100.0 * quality,
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| {
            e.title(title)
                .description(description)
                .timestamp(game.datetime())
        })
    })?;
    Ok(())
}

pub fn swap(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let lobby = if let Some(lobby) = lobbies.get(&msg.channel_id) {
        lobby
    } else {
        return Err(Error::NotALobby(msg.channel_id));
    };
    let member1 = if let Some(member) = Member::parse(ctx, guild_id, &args[0])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[0].to_owned()));
    };
    let member2 = if let Some(member) = Member::parse(ctx, guild_id, &args[1])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[1].to_owned()));
    };
    let mut game = match database.get_games()?.remove(&msg.channel_id) {
        Some(games) => {
            if let Some(game) = games.into_iter().last() {
                game.1
            } else {
                return Ok(());
            }
        }
        None => return Ok(()),
    };
    if game.score() != Score::Undecided {
        return Err(Error::GameAlreadySet);
    }
    let teams = game.teams();
    let mut team1 = teams[0].iter().copied().collect::<HashSet<_>>();
    let mut team2 = teams[1].iter().copied().collect::<HashSet<_>>();
    if team1.contains(&member1.user.id) {
        if team1.contains(&member2.user.id) {
            return Err(Error::SameTeam);
        } else {
            let roles = ctx.get_guild_roles(guild_id)?;
            let role1 = roles
                .iter()
                .find(|x| x.name == format!("{} Game {} Team 1", lobby.name(), game.id()));
            team1.remove(&member1.user.id);
            team1.insert(member2.user.id);
            if let Some(role1) = role1 {
                ctx.remove_guild_member_role(guild_id, member1.user.id, role1.id)?;
                ctx.add_guild_member_role(guild_id, member2.user.id, role1.id)?;
            }
            if team2.contains(&member2.user.id) {
                let role2 = roles
                    .iter()
                    .find(|x| x.name == format!("{} Game {} Team 2", lobby.name(), game.id()));
                team2.remove(&member2.user.id);
                team2.insert(member1.user.id);
                if let Some(role2) = role2 {
                    ctx.remove_guild_member_role(guild_id, member2.user.id, role2.id)?;
                    ctx.add_guild_member_role(guild_id, member1.user.id, role2.id)?;
                }
            }
        }
    } else if team2.contains(&member1.user.id) {
        if team2.contains(&member2.user.id) {
            return Err(Error::SameTeam);
        } else {
            let roles = ctx.get_guild_roles(guild_id)?;
            let role2 = roles
                .iter()
                .find(|x| x.name == format!("{} Game {} Team 2", lobby.name(), game.id()));
            team2.remove(&member1.user.id);
            team2.insert(member2.user.id);
            if let Some(role2) = role2 {
                ctx.remove_guild_member_role(guild_id, member1.user.id, role2.id)?;
                ctx.add_guild_member_role(guild_id, member2.user.id, role2.id)?;
            }
            if team1.contains(&member2.user.id) {
                let role1 = roles
                    .iter()
                    .find(|x| x.name == format!("{} Game {} Team 1", lobby.name(), game.id()));
                team1.remove(&member2.user.id);
                team1.insert(member1.user.id);
                if let Some(role1) = role1 {
                    ctx.remove_guild_member_role(guild_id, member2.user.id, role1.id)?;
                    ctx.add_guild_member_role(guild_id, member1.user.id, role1.id)?;
                }
            }
        }
    } else {
        return Err(Error::NotPlaying(member1.user.id));
    }
    game.set_teams([
        team1.into_iter().collect::<Vec<_>>(),
        team2.into_iter().collect::<Vec<_>>(),
    ]);
    database.update_game(&game, msg.channel_id)?;
    let f = |users: &[UserId]| {
        users
            .iter()
            .map(|x| x.mention())
            .collect::<Vec<_>>()
            .join("\n")
    };
    let ratings = lobby.ratings();
    let quality = utils::quality(
        &[
            game.teams()[0]
                .iter()
                .map(|x| {
                    (
                        (),
                        ratings
                            .get(x)
                            .map(|x| x.rating)
                            .unwrap_or_else(|| trueskill.create_rating()),
                    )
                })
                .collect(),
            game.teams()[1]
                .iter()
                .map(|x| {
                    (
                        (),
                        ratings
                            .get(x)
                            .map(|x| x.rating)
                            .unwrap_or_else(|| trueskill.create_rating()),
                    )
                })
                .collect(),
        ],
        trueskill,
    );
    let title = format!("Game {}", game.id());
    let description = format!(
        "Quality: {:.0}\n\nTeam 1:\n{}\n\nTeam 2:\n{}",
        100.0 * quality,
        f(game.teams()[0]),
        f(game.teams()[1])
    );
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description(description).title(title))
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn setrating(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    ranks: &[Rank],
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    if args.len() < 2 {
        return Err(Error::NotEnoughArguments);
    }
    let member = if let Some(member) = Member::parse(ctx, guild_id, &args[0])? {
        member
    } else {
        return Err(Error::MemberNotFound(args[0].to_owned()));
    };
    let rating = args[1].parse::<i64>()?;
    database.insert_initial_rating(member.user.id, rating as f64)?;
    let mut games = database.get_games()?;
    for (channel_id, lobby) in lobbies.iter_mut() {
        let games = if let Some(games) = games.remove(channel_id) {
            games
        } else {
            continue;
        };
        let initials = database.get_initial_ratings()?;
        let ratings = Ratings::from_games(&games, &initials, trueskill);
        lobby.set_ratings(ratings);

        // Update all leaderboards
        let leaderboard = utils::leaderboard(lobby, 15, ranks, |user_id| {
            checks::has_role(ctx, guild_id, user_id, roles.ranked)
        })?;
        if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
            messages.par_iter().for_each(|&message| {
                if let Err(err) = ctx.webhook_delete_message(*webhook_id, webhook_token, message) {
                    eprintln!("Err: {:?}", err);
                }
            });
            messages.clear();
            for (title, description) in leaderboard.iter() {
                let message = ctx.execute_webhook(*webhook_id, webhook_token, true, |m| {
                    m.embed(|e| e.description(description).title(title))
                })?;
                if let Some(message) = message {
                    messages.push(message.id);
                }
            }
        }
    }
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description(format!("Initial rating set to {}", rating)))
    })?;
    Ok(())
}
