use harmony::client::Context;
use harmony::model::id::{GuildId, RoleId, UserId};
use harmony::model::Message;

use crate::{Error, Result};

pub fn has_role(
    ctx: &Context,
    guild_id: GuildId,
    user_id: UserId,
    role_id: RoleId,
) -> Result<bool> {
    Ok(ctx
        .member(guild_id, user_id)?
        .map(|x| x.roles.contains(&role_id))
        .unwrap_or_default())
}

pub fn get_guild(msg: &Message) -> Result<GuildId> {
    if let Some(guild_id) = msg.guild_id {
        return Ok(guild_id);
    }
    Err(Error::NotAGuild)
}
