use harmony::client::Context;
use harmony::model::Message;

use crate::config::Roles;
use crate::Result;

pub fn check_admin(ctx: &Context, msg: &Message, roles: &Roles) -> Result<bool> {
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Not in a guild
        return Ok(false);
    };
    let member = if let Some(member) = ctx.member(guild_id, msg.author.id)? {
        member
    } else {
        // Author of the command is not in the guild? Should never happen
        return Ok(false);
    };
    Ok(member.roles.contains(&roles.admin.into()))
}

pub fn check_banned(ctx: &Context, msg: &Message, roles: &Roles) -> Result<bool> {
    let guild_id = if let Some(guild_id) = msg.guild_id {
        guild_id
    } else {
        // Not in a guild
        return Ok(false);
    };
    let member = if let Some(member) = ctx.member(guild_id, msg.author.id)? {
        member
    } else {
        // Author of the command is not in the guild? Should never happen
        return Ok(false);
    };
    Ok(member.roles.contains(&roles.banned.into()))
}
