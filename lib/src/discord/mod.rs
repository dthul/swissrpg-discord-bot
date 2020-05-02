pub mod sync;
pub mod util;

use serenity::model::{
    channel::{Channel, PermissionOverwrite, PermissionOverwriteType},
    id::{ChannelId, UserId},
    permissions::Permissions,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct CacheAndHttp {
    pub cache: serenity::cache::CacheRwLock,
    pub http: Arc<serenity::http::client::Http>,
}

impl serenity::http::CacheHttp for CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::client::Http {
        &self.http
    }
}

impl serenity::http::CacheHttp for &CacheAndHttp {
    fn cache(&self) -> Option<&serenity::cache::CacheRwLock> {
        Some(&self.cache)
    }
    fn http(&self) -> &serenity::http::client::Http {
        &self.http
    }
}

impl From<&serenity::client::Context> for CacheAndHttp {
    fn from(ctx: &serenity::client::Context) -> Self {
        CacheAndHttp {
            cache: ctx.cache.clone(),
            http: ctx.http.clone(),
        }
    }
}

pub fn is_host(
    discord_api: &CacheAndHttp,
    channel_id: ChannelId,
    user_id: UserId,
    redis_connection: &mut redis::Connection,
) -> Result<bool, crate::meetup::Error> {
    let channel =
        if let Some(Channel::Guild(channel)) = discord_api.cache.read().channel(channel_id) {
            channel.clone()
        } else {
            return Err(simple_error::SimpleError::new("Could not find this channel").into());
        };
    // Assume that users with the READ_MESSAGES, MANAGE_MESSAGES and
    // MENTION_EVERYONE permission are channel hosts
    let user_permission_overwrites = channel
        .read()
        .permission_overwrites
        .iter()
        .find(|overwrite| PermissionOverwriteType::Member(user_id) == overwrite.kind)
        .cloned();
    let is_host = user_permission_overwrites.map_or(false, |overwrites| {
        overwrites.allow.contains(
            Permissions::READ_MESSAGES
                | Permissions::MANAGE_MESSAGES
                | Permissions::MENTION_EVERYONE,
        )
    });
    if !is_host {
        // Maybe the user is still on the old host roles
        let channel_roles = crate::get_channel_roles(channel_id.0, redis_connection)?;
        if let Some(crate::ChannelRoles {
            host: Some(host_role),
            ..
        }) = channel_roles
        {
            let user = user_id.to_user(discord_api)?;
            let is_host = user
                .has_role(discord_api, sync::ids::GUILD_ID, host_role)
                .unwrap_or(false);
            return Ok(is_host);
        } else {
            return Ok(false);
        }
    }
    Ok(is_host)
}

// True if permissions changed, false otherwise
pub fn add_channel_user_permissions(
    discord_api: &CacheAndHttp,
    channel_id: ChannelId,
    user_id: UserId,
    permissions: Permissions,
) -> Result<bool, crate::meetup::Error> {
    if permissions == Permissions::empty() {
        return Ok(false);
    }
    let channel =
        if let Some(Channel::Guild(channel)) = discord_api.cache.read().channel(channel_id) {
            channel.clone()
        } else {
            return Err(simple_error::SimpleError::new("Could not find this channel").into());
        };
    let current_permission_overwrites = channel
        .read()
        .permission_overwrites
        .iter()
        .find(|overwrite| PermissionOverwriteType::Member(user_id) == overwrite.kind)
        .cloned()
        .unwrap_or(PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(user_id),
        });
    let mut new_permission_overwrites = current_permission_overwrites.clone();
    new_permission_overwrites.allow |= permissions;
    if new_permission_overwrites.allow != current_permission_overwrites.allow {
        channel
            .read()
            .create_permission(&discord_api.http, &new_permission_overwrites)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

// True if permissions changed, false otherwise
pub fn remove_channel_user_permissions(
    discord_api: &CacheAndHttp,
    channel_id: ChannelId,
    user_id: UserId,
    permissions: Permissions,
) -> Result<bool, crate::meetup::Error> {
    if permissions == Permissions::empty() {
        return Ok(false);
    }
    let channel =
        if let Some(Channel::Guild(channel)) = discord_api.cache.read().channel(channel_id) {
            channel.clone()
        } else {
            return Err(simple_error::SimpleError::new("Could not find this channel").into());
        };
    let current_permission_overwrites = channel
        .read()
        .permission_overwrites
        .iter()
        .find(|overwrite| PermissionOverwriteType::Member(user_id) == overwrite.kind)
        .cloned()
        .unwrap_or(PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(user_id),
        });
    let mut new_permission_overwrites = current_permission_overwrites.clone();
    new_permission_overwrites.allow &= !permissions;
    if new_permission_overwrites.allow != current_permission_overwrites.allow {
        if new_permission_overwrites.allow == Permissions::empty()
            && new_permission_overwrites.deny == Permissions::empty()
        {
            channel
                .read()
                .delete_permission(&discord_api.http, new_permission_overwrites.kind)?;
        } else {
            channel
                .read()
                .create_permission(&discord_api.http, &new_permission_overwrites)?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}
