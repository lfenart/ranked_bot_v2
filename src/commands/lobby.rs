use std::collections::HashSet;

use chrono::{DateTime, Utc};
use harmony::client::Context;
use harmony::model::id::{ChannelId, GuildId, UserId};
use harmony::model::{Member, Message};
use trueskill::SimpleTrueSkill;

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
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if checks::has_role(ctx, guild_id, msg.author.id, roles.banned)? {
        return Ok(());
    }
    join_internal(
        ctx,
        guild_id,
        msg.channel_id,
        msg.author.id,
        msg.timestamp,
        false,
        lobbies,
        trueskill,
        database,
    )
}

pub fn forcejoin(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
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
    user_id: UserId,
    timestamp: DateTime<Utc>,
    force: bool,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let players = {
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
            Some(lobby.clear().into_keys().collect())
        } else {
            None
        }
    };
    if let Some(players) = players {
        start_game(
            ctx, guild_id, channel_id, lobbies, players, trueskill, database,
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
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    leave_internal(
        ctx,
        guild_id,
        msg.channel_id,
        msg.author.id,
        false,
        lobbies,
        trueskill,
        database,
    )
}

pub fn forceleave(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
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
            ctx, guild_id, channel_id, lobbies, players, trueskill, database,
        )?;
    }
    Ok(())
}

pub fn players(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    let players = {
        let lobby = lobbies
            .get_mut(&msg.channel_id)
            .ok_or(Error::NotALobby(msg.channel_id))?;
        let x = args
            .iter()
            .next()
            .ok_or(Error::NotEnoughArguments)?
            .parse::<usize>()?;
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
    lobbies: &mut Lobbies,
    players: Vec<UserId>,
    trueskill: SimpleTrueSkill,
    database: &Database,
) -> Result {
    let lobby = lobbies.get(&channel_id).unwrap();
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
    let role0 = ctx.create_guild_role(guild_id, |r| {
        r.name(format!("{} Game {}", lobby.name(), game.id()))
            .mentionable(true)
            .hoist(true)
    })?;
    let role1 = ctx.create_guild_role(guild_id, |r| {
        r.name(format!("{} Game {} Team 1", lobby.name(), game.id()))
            .mentionable(true)
            .hoist(true)
    })?;
    let role2 = ctx.create_guild_role(guild_id, |r| {
        r.name(format!("{} Game {} Team 2", lobby.name(), game.id()))
            .mentionable(true)
            .hoist(true)
    })?;
    for (user_id, _) in teams[0].iter() {
        ctx.add_guild_member_role(guild_id, *user_id, role0.id)?;
        ctx.add_guild_member_role(guild_id, *user_id, role1.id)?;
    }
    for (user_id, _) in teams[1].iter() {
        ctx.add_guild_member_role(guild_id, *user_id, role0.id)?;
        ctx.add_guild_member_role(guild_id, *user_id, role2.id)?;
    }
    ctx.send_message(channel_id, |m| {
        m.content(format!("{} {}", role1.id.mention(), role2.id.mention()))
            .embed(|e| {
                e.title(title)
                    .description(description)
                    .timestamp(game.datetime())
            })
    })?;
    for (&channel_id, lobby) in lobbies.iter_mut() {
        for (user_id, _) in players.iter() {
            if lobby.leave(*user_id, true).is_ok() {
                ctx.send_message(channel_id, |m| {
                    m.embed(|e| {
                        e.description(format!(
                            "[{}/{}] {} left the queue (Game started).",
                            lobby.len(),
                            lobby.capacity(),
                            user_id.mention(),
                        ))
                    })
                })?;
            }
        }
    }
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
    let initial_ratings = database.get_initial_ratings()?;
    game.set_score(score);
    database.update_game(&game, msg.channel_id)?;
    let games = database
        .get_games()?
        .remove(&msg.channel_id)
        .unwrap_or_default();
    let ratings = Ratings::from_games(&games, &initial_ratings, trueskill);
    lobby.set_ratings(ratings);
    let leaderboard = utils::leaderboard(lobby, 20, |user_id| {
        checks::has_role(ctx, guild_id, user_id, roles.ranked)
    })?;
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        for &message in messages.iter() {
            ctx.webhook_delete_message(*webhook_id, webhook_token, message)
                .ok(); // Maybe it was already deleted
        }
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
    {
        let roles = ctx.get_guild_roles(guild_id)?;
        for role in roles {
            if role
                .name
                .contains(&format!("{} Game {}", lobby.name(), game_id))
            {
                ctx.delete_guild_role(guild_id, role.id)?;
            }
        }
    }
    for &user_id in game.teams()[0] {
        if !checks::has_role(ctx, guild_id, user_id, roles.ranked)? {
            continue;
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
        ctx.add_guild_member_role(guild_id, user_id, ranks[rank_index].id)?;
        if rank_index > 0 && score != Score::Team2 {
            ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index - 1].id)?;
        }
        if rank_index + 1 < ranks.len() && score != Score::Team1 {
            ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index + 1].id)?;
        }
    }
    for &user_id in game.teams()[1] {
        if !checks::has_role(ctx, guild_id, user_id, roles.ranked)? {
            continue;
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
        ctx.add_guild_member_role(guild_id, user_id, ranks[rank_index].id)?;
        if rank_index > 0 && score != Score::Team1 {
            ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index - 1].id)?;
        }
        if rank_index + 1 < ranks.len() && score != Score::Team2 {
            ctx.remove_guild_member_role(guild_id, user_id, ranks[rank_index + 1].id)?;
        }
    }
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Game updated"))
    })?;
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
    let roles = ctx.get_guild_roles(guild_id)?;
    for role in roles {
        if role
            .name
            .contains(&format!("{} Game {}", lobby.name(), game_id))
        {
            ctx.delete_guild_role(guild_id, role.id)?;
        }
    }
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Game cancelled."))
    })?;
    Ok(())
}

pub fn undo(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    database: &Database,
    trueskill: SimpleTrueSkill,
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
        m.embed(|e| e.description("Game updated"))
    })?;
    if prev_score == Score::Cancelled || prev_score == Score::Undecided {
        return Ok(());
    }
    let leaderboard = utils::leaderboard(lobby, 20, |user_id| {
        checks::has_role(ctx, guild_id, user_id, roles.ranked)
    })?;
    if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
        for &message in messages.iter() {
            ctx.webhook_delete_message(*webhook_id, webhook_token, message)
                .ok(); // Maybe it was already deleted
        }
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
    args: &[String],
) -> Result {
    let guild_id = checks::get_guild(msg)?;
    if !checks::has_role(ctx, guild_id, msg.author.id, roles.admin)? {
        return Ok(());
    }
    if lobbies.get(&msg.channel_id).is_none() {
        return Err(Error::NotALobby(msg.channel_id));
    }
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
            team1.remove(&member1.user.id);
            team1.insert(member2.user.id);
            if team2.contains(&member2.user.id) {
                team2.remove(&member2.user.id);
                team2.insert(member1.user.id);
            }
        }
    } else if team2.contains(&member1.user.id) {
        if team2.contains(&member2.user.id) {
            return Err(Error::SameTeam);
        } else {
            team2.remove(&member1.user.id);
            team2.insert(member2.user.id);
            if team1.contains(&member2.user.id) {
                team1.remove(&member2.user.id);
                team1.insert(member1.user.id);
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
    ctx.send_message(msg.channel_id, |m| {
        m.embed(|e| e.description("Players swapped"))
    })?;
    Ok(())
}

pub fn setrating(
    ctx: &Context,
    msg: &Message,
    roles: &Roles,
    lobbies: &mut Lobbies,
    trueskill: SimpleTrueSkill,
    database: &Database,
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
        let leaderboard = utils::leaderboard(lobby, 20, |user_id| {
            checks::has_role(ctx, guild_id, user_id, roles.ranked)
        })?;
        if let Some((webhook_id, webhook_token, messages)) = lobby.webhook_mut() {
            for &message in messages.iter() {
                ctx.webhook_delete_message(*webhook_id, webhook_token, message)
                    .ok(); // Maybe it was already deleted
            }
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
