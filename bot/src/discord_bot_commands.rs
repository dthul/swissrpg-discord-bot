use crate::{error::BoxedError, strings};
use futures_util::compat::Future01CompatExt;
use redis::{Commands, PipelineCommands};
use regex::Regex;
use serenity::{
    model::{channel::Message, id::UserId, user::User},
    prelude::*,
};
use simple_error::SimpleError;
use std::borrow::Cow;

const MENTION_PATTERN: &'static str = r"<@!?(?P<mention_id>[0-9]+)>";

pub struct Regexes {
    pub bot_mention: String,
    pub link_meetup_dm: Regex,
    pub link_meetup_mention: Regex,
    pub link_meetup_bot_admin_dm: Regex,
    pub link_meetup_bot_admin_mention: Regex,
    pub unlink_meetup_dm: Regex,
    pub unlink_meetup_mention: Regex,
    pub unlink_meetup_bot_admin_dm: Regex,
    pub unlink_meetup_bot_admin_mention: Regex,
    pub sync_meetup_mention: Regex,
    pub sync_discord_mention: Regex,
    pub add_user_bot_admin_mention: Regex,
    pub add_host_bot_admin_mention: Regex,
    pub remove_user_mention: Regex,
    pub remove_host_bot_admin_mention: Regex,
    pub stop_bot_admin_dm: Regex,
    pub stop_bot_admin_mention: Regex,
    pub send_expiration_reminder_bot_admin_mention: Regex,
    pub end_adventure_host_mention: Regex,
    pub help_dm: Regex,
    pub help_mention: Regex,
}

impl Regexes {
    pub fn link_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_dm
        } else {
            &self.link_meetup_mention
        }
    }

    pub fn link_meetup_bot_admin(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_bot_admin_dm
        } else {
            &self.link_meetup_bot_admin_mention
        }
    }

    pub fn unlink_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_dm
        } else {
            &self.unlink_meetup_mention
        }
    }

    pub fn unlink_meetup_bot_admin(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_bot_admin_dm
        } else {
            &self.unlink_meetup_bot_admin_mention
        }
    }

    pub fn stop_bot_admin(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.stop_bot_admin_dm
        } else {
            &self.stop_bot_admin_mention
        }
    }

    pub fn help(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.help_dm
        } else {
            &self.help_mention
        }
    }
}

pub fn compile_regexes(bot_id: u64) -> Regexes {
    let bot_mention = format!(r"<@{}>", bot_id);
    let link_meetup_dm = r"^(?i)link[ -]?meetup\s*$";
    let link_meetup_mention = format!(
        r"^{bot_mention}\s+(?i)link[ -]?meetup\s*$",
        bot_mention = bot_mention
    );
    let link_meetup_bot_admin = format!(
        r"(?i)link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
        mention_pattern = MENTION_PATTERN
    );
    let link_meetup_bot_admin_dm = format!(
        r"^{link_meetup_bot_admin}\s*$",
        link_meetup_bot_admin = link_meetup_bot_admin
    );
    let link_meetup_bot_admin_mention = format!(
        r"^{bot_mention}\s+{link_meetup_bot_admin}\s*$",
        bot_mention = bot_mention,
        link_meetup_bot_admin = link_meetup_bot_admin
    );
    let unlink_meetup = r"(?i)unlink[ -]?meetup";
    let unlink_meetup_dm = format!(r"^{unlink_meetup}\s*$", unlink_meetup = unlink_meetup);
    let unlink_meetup_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup}\s*$",
        bot_mention = bot_mention,
        unlink_meetup = unlink_meetup
    );
    let unlink_meetup_bot_admin = format!(
        r"(?i)unlink[ -]?meetup\s+{mention_pattern}",
        mention_pattern = MENTION_PATTERN
    );
    let unlink_meetup_bot_admin_dm = format!(
        r"^{unlink_meetup_bot_admin}\s*$",
        unlink_meetup_bot_admin = unlink_meetup_bot_admin
    );
    let unlink_meetup_bot_admin_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup_bot_admin}\s*$",
        bot_mention = bot_mention,
        unlink_meetup_bot_admin = unlink_meetup_bot_admin
    );
    let sync_meetup_mention = format!(
        r"^{bot_mention}\s+(?i)sync\s+meetup\s*$",
        bot_mention = bot_mention
    );
    let sync_discord_mention = format!(
        r"^{bot_mention}\s+(?i)sync\s+discord\s*$",
        bot_mention = bot_mention
    );
    let add_user_bot_admin_mention = format!(
        r"^{bot_mention}\s+(?i)add\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let add_host_bot_admin_mention = format!(
        r"^{bot_mention}\s+(?i)add\s+host\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let remove_user_mention = format!(
        r"^{bot_mention}\s+(?i)remove\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let remove_host_bot_admin_mention = format!(
        r"^{bot_mention}\s+(?i)remove\s+host\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let stop_bot_admin_dm = r"^(?i)stop\s*$";
    let stop_bot_admin_mention =
        format!(r"^{bot_mention}\s+(?i)stop\s*$", bot_mention = bot_mention);
    let send_expiration_reminder_bot_admin_mention = format!(
        r"^{bot_mention}\s+(?i)remind\s+expiration\s*$",
        bot_mention = bot_mention
    );
    let end_adventure_host_mention = format!(
        r"^{bot_mention}\s+(?i)end\s+adventure\s*$",
        bot_mention = bot_mention
    );
    let help_dm = r"^(?i)help\s*$";
    let help_mention = format!(r"^{bot_mention}\s+(?i)help\s*$", bot_mention = bot_mention);
    Regexes {
        bot_mention: bot_mention,
        link_meetup_dm: Regex::new(link_meetup_dm).unwrap(),
        link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
        link_meetup_bot_admin_dm: Regex::new(link_meetup_bot_admin_dm.as_str()).unwrap(),
        link_meetup_bot_admin_mention: Regex::new(link_meetup_bot_admin_mention.as_str()).unwrap(),
        unlink_meetup_dm: Regex::new(unlink_meetup_dm.as_str()).unwrap(),
        unlink_meetup_mention: Regex::new(unlink_meetup_mention.as_str()).unwrap(),
        unlink_meetup_bot_admin_dm: Regex::new(unlink_meetup_bot_admin_dm.as_str()).unwrap(),
        unlink_meetup_bot_admin_mention: Regex::new(unlink_meetup_bot_admin_mention.as_str())
            .unwrap(),
        sync_meetup_mention: Regex::new(sync_meetup_mention.as_str()).unwrap(),
        sync_discord_mention: Regex::new(sync_discord_mention.as_str()).unwrap(),
        add_user_bot_admin_mention: Regex::new(add_user_bot_admin_mention.as_str()).unwrap(),
        add_host_bot_admin_mention: Regex::new(add_host_bot_admin_mention.as_str()).unwrap(),
        remove_user_mention: Regex::new(remove_user_mention.as_str()).unwrap(),
        remove_host_bot_admin_mention: Regex::new(remove_host_bot_admin_mention.as_str()).unwrap(),
        stop_bot_admin_dm: Regex::new(stop_bot_admin_dm).unwrap(),
        stop_bot_admin_mention: Regex::new(stop_bot_admin_mention.as_str()).unwrap(),
        send_expiration_reminder_bot_admin_mention: Regex::new(
            send_expiration_reminder_bot_admin_mention.as_str(),
        )
        .unwrap(),
        end_adventure_host_mention: Regex::new(end_adventure_host_mention.as_str()).unwrap(),
        help_dm: Regex::new(help_dm).unwrap(),
        help_mention: Regex::new(help_mention.as_str()).unwrap(),
    }
}

impl crate::discord_bot::Handler {
    pub fn link_meetup(ctx: &Context, msg: &Message, user_id: u64) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let (redis_client, redis_connection_mutex, meetup_client_mutex, bot_id) = {
            let data = ctx.data.read();
            (
                data.get::<crate::discord_bot::RedisClientKey>()
                    .ok_or_else(|| SimpleError::new("Redis client was not set"))?
                    .clone(),
                data.get::<crate::discord_bot::RedisConnectionKey>()
                    .ok_or_else(|| SimpleError::new("Redis connection was not set"))?
                    .clone(),
                data.get::<crate::discord_bot::MeetupClientKey>()
                    .ok_or_else(|| SimpleError::new("Meetup client was not set"))?
                    .clone(),
                data.get::<crate::discord_bot::BotIdKey>()
                    .ok_or_else(|| SimpleError::new("Bot ID was not set"))?
                    .clone(),
            )
        };
        // Check if there is already a meetup id linked to this user
        // and issue a warning
        let linked_meetup_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_d2m)?
        };
        if let Some(linked_meetup_id) = linked_meetup_id {
            let meetup_client_guard = crate::ASYNC_RUNTIME.block_on(meetup_client_mutex.lock());
            match *meetup_client_guard {
                Some(ref meetup_client) => {
                    match meetup_client.get_member_profile(Some(linked_meetup_id))? {
                        Some(user) => {
                            let _ = msg.author.direct_message(ctx, |message| {
                                message.content(strings::DISCORD_ALREADY_LINKED_MESSAGE1(
                                    &user.name, bot_id.0,
                                ))
                            });
                            let _ = msg.react(ctx, "\u{2705}");
                        }
                        _ => {
                            let _ = msg.author.direct_message(ctx, |message| {
                                message
                                    .content(strings::NONEXISTENT_MEETUP_LINKED_MESSAGE(bot_id.0))
                            });
                            let _ = msg.react(ctx, "\u{2705}");
                        }
                    }
                }
                _ => {
                    let _ = msg.author.direct_message(ctx, |message| {
                        message.content(strings::DISCORD_ALREADY_LINKED_MESSAGE2(bot_id.0))
                    });
                    let _ = msg.react(ctx, "\u{2705}");
                }
            }
            return Ok(());
        }
        // TODO: creates a new Redis connection. Not optimal...
        let (_, url) = crate::ASYNC_RUNTIME.block_on(async {
            let redis_connection = redis_client.get_async_connection().compat().await?;
            crate::meetup_oauth2::generate_meetup_linking_link(redis_connection, user_id).await
        })?;
        let dm = msg.author.direct_message(ctx, |message| {
            message.content(strings::MEETUP_LINKING_MESSAGE(&url))
        });
        match dm {
            Ok(_) => {
                let _ = msg.react(ctx, "\u{2705}");
            }
            Err(why) => {
                eprintln!("Error sending Meetup linking DM: {:?}", why);
                let _ = msg.reply(ctx, "There was an error trying to send you instructions.");
            }
        }
        Ok(())
    }

    pub fn link_meetup_bot_admin(
        ctx: &Context,
        msg: &Message,
        regexes: &Regexes,
        user_id: u64,
        meetup_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
        let (redis_connection_mutex, meetup_client_mutex) = {
            let data = ctx.data.read();
            (
                data.get::<crate::discord_bot::RedisConnectionKey>()
                    .ok_or_else(|| SimpleError::new("Redis connection was not set"))?
                    .clone(),
                data.get::<crate::discord_bot::MeetupClientKey>()
                    .ok_or_else(|| SimpleError::new("Meetup client was not set"))?
                    .clone(),
            )
        };
        // Check if there is already a meetup id linked to this user
        // and issue a warning
        let linked_meetup_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_d2m)?
        };
        if let Some(linked_meetup_id) = linked_meetup_id {
            if linked_meetup_id == meetup_id {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "All good, this Meetup account was already linked to <@{}>",
                        user_id
                    ),
                );
                return Ok(());
            } else {
                // TODO: answer in DM?
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "<@{discord_id}> is already linked to a different Meetup account. If you \
                         want to change this, unlink the currently linked Meetup account first by \
                         writing:\n{bot_mention} unlink meetup <@{discord_id}>",
                        discord_id = user_id,
                        bot_mention = regexes.bot_mention
                    ),
                );
                return Ok(());
            }
        }
        // Check if this meetup id is already linked
        // and issue a warning
        let linked_discord_id: Option<u64> = {
            let mut redis_connection = redis_connection_mutex.lock();
            redis_connection.get(&redis_key_m2d)?
        };
        if let Some(linked_discord_id) = linked_discord_id {
            let _ = msg.author.direct_message(ctx, |message_builder| {
                message_builder.content(format!(
                    "This Meetup account is alread linked to <@{linked_discord_id}>. If you want \
                     to change this, unlink the Meetup account first by writing\n{bot_mention} \
                     unlink meetup <@{linked_discord_id}>",
                    linked_discord_id = linked_discord_id,
                    bot_mention = regexes.bot_mention
                ))
            });
            return Ok(());
        }
        // The user has not yet linked their meetup account.
        // Test whether the specified Meetup user actually exists.
        let meetup_client_guard = crate::ASYNC_RUNTIME.block_on(meetup_client_mutex.lock());
        let meetup_user = meetup_client_guard
            .as_ref()
            .ok_or_else(|| SimpleError::new("Meetup API unavailable"))?
            .get_member_profile(Some(meetup_id))?;
        drop(meetup_client_guard);
        match meetup_user {
            None => {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    "It looks like this Meetup profile does not exist",
                );
                return Ok(());
            }
            Some(meetup_user) => {
                let mut successful = false;
                {
                    let mut redis_connection = redis_connection_mutex.lock();
                    // Try to atomically set the meetup id
                    redis::transaction(
                        &mut *redis_connection,
                        &[&redis_key_d2m, &redis_key_m2d],
                        |con, pipe| {
                            let linked_meetup_id: Option<u64> = con.get(&redis_key_d2m)?;
                            let linked_discord_id: Option<u64> = con.get(&redis_key_m2d)?;
                            if linked_meetup_id.is_some() || linked_discord_id.is_some() {
                                // The meetup id was linked in the meantime, abort
                                successful = false;
                                // Execute empty transaction just to get out of the closure
                                pipe.query(con)
                            } else {
                                pipe.sadd("meetup_users", meetup_id)
                                    .sadd("discord_users", user_id)
                                    .set(&redis_key_d2m, meetup_id)
                                    .set(&redis_key_m2d, user_id);
                                successful = true;
                                pipe.query(con)
                            }
                        },
                    )?;
                }
                if successful {
                    let photo_url = meetup_user.photo.as_ref().map(|p| p.thumb_link.as_str());
                    let _ = msg.channel_id.send_message(&ctx.http, |message| {
                        message.embed(|embed| {
                            embed.title("Linked Meetup account");
                            embed.description(format!(
                                "Successfully linked <@{}> to {}'s Meetup account",
                                user_id, meetup_user.name
                            ));
                            if let Some(photo_url) = photo_url {
                                embed.image(photo_url)
                            } else {
                                embed
                            }
                        })
                    });
                    return Ok(());
                } else {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "Could not assign meetup id (timing error)");
                    return Ok(());
                }
            }
        }
    }

    pub fn unlink_meetup(
        ctx: &Context,
        msg: &Message,
        is_bot_admin_command: bool,
        user_id: u64,
        bot_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let redis_connection_mutex = {
            ctx.data
                .read()
                .get::<crate::discord_bot::RedisConnectionKey>()
                .ok_or_else(|| SimpleError::new("Redis connection was not set"))?
                .clone()
        };
        let mut redis_connection = redis_connection_mutex.lock();
        // Check if there is actually a meetup id linked to this user
        let linked_meetup_id: Option<u64> = redis_connection.get(&redis_key_d2m)?;
        match linked_meetup_id {
            Some(meetup_id) => {
                let redis_key_m2d = format!("meetup_user:{}:discord_user", meetup_id);
                redis_connection.del(&[&redis_key_d2m, &redis_key_m2d])?;
                let message = if is_bot_admin_command {
                    format!("Unlinked <@{}>'s Meetup account", user_id)
                } else {
                    strings::MEETUP_UNLINK_SUCCESS(bot_id)
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
            None => {
                let message = if is_bot_admin_command {
                    Cow::Owned(format!(
                        "There was seemingly no meetup account linked to <@{}>",
                        user_id
                    ))
                } else {
                    Cow::Borrowed(strings::MEETUP_UNLINK_NOT_LINKED)
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
        }
        Ok(())
    }

    fn get_channel_roles(
        channel_id: u64,
        redis_connection: &mut redis::Connection,
    ) -> Result<Option<ChannelRoles>, BoxedError> {
        // Check that this message came from a bot controlled channel
        let redis_channel_role_key = format!("discord_channel:{}:discord_role", channel_id);
        let redis_channel_host_role_key =
            format!("discord_channel:{}:discord_host_role", channel_id);
        let channel_roles: redis::RedisResult<(Option<u64>, Option<u64>)> = redis::pipe()
            .get(redis_channel_role_key)
            .get(redis_channel_host_role_key)
            .query(redis_connection);
        match channel_roles {
            Ok((Some(role), Some(host_role))) => Ok(Some(ChannelRoles {
                user: role,
                host: host_role,
            })),
            Ok((None, None)) => Ok(None),
            Ok(_) => {
                return Err(SimpleError::new("Channel has only one of two roles").into());
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    pub fn end_adventure(
        ctx: &Context,
        msg: &Message,
        redis_client: redis::Client,
    ) -> Result<(), BoxedError> {
        let mut redis_connection = redis_client.get_connection()?;
        // Check whether this is a bot controlled channel
        let channel_roles = Self::get_channel_roles(msg.channel_id.0, &mut redis_connection)?;
        let channel_roles = match channel_roles {
            Some(roles) => roles,
            None => {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, strings::CHANNEL_NOT_BOT_CONTROLLED);
                return Ok(());
            }
        };
        // This is only for bot_admins and channel hosts
        let is_bot_admin = msg
            .author
            .has_role(
                ctx,
                crate::discord_sync::GUILD_ID,
                crate::discord_sync::BOT_ADMIN_ID,
            )
            .unwrap_or(false);
        let is_host = msg
            .author
            .has_role(ctx, crate::discord_sync::GUILD_ID, channel_roles.host)
            .unwrap_or(false);
        if !is_bot_admin && !is_host {
            let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_CHANNEL_ADMIN);
            return Ok(());
        }
        // Check if there is a channel expiration time in the future
        let redis_channel_expiration_key =
            format!("discord_channel:{}:expiration_time", msg.channel_id.0);
        let expiration_time: Option<String> =
            redis_connection.get(&redis_channel_expiration_key)?;
        let expiration_time = expiration_time
            .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
            .transpose()?
            .map(|t| t.with_timezone(&chrono::Utc));
        if let Some(expiration_time) = expiration_time {
            if expiration_time > chrono::Utc::now() {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, strings::CHANNEL_NOT_YET_CLOSEABLE);
                return Ok(());
            }
        }
        // Schedule this channel for deletion
        // TODO: in 24 hours
        let new_deletion_time = chrono::Utc::now();
        let redis_channel_deletion_key =
            format!("discord_channel:{}:deletion_time", msg.channel_id.0);
        let current_deletion_time: Option<String> =
            redis_connection.get(&redis_channel_deletion_key)?;
        let current_deletion_time = current_deletion_time
            .map(|t| chrono::DateTime::parse_from_rfc3339(&t))
            .transpose()?
            .map(|t| t.with_timezone(&chrono::Utc));
        if let Some(current_deletion_time) = current_deletion_time {
            if new_deletion_time > current_deletion_time {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, strings::CHANNEL_ALREADY_MARKED_FOR_CLOSING);
                return Ok(());
            }
        }
        let _: () =
            redis_connection.set(&redis_channel_deletion_key, new_deletion_time.to_rfc3339())?;
        let _ = msg
            .channel_id
            .say(&ctx.http, strings::CHANNEL_MARKED_FOR_CLOSING);
        Ok(())
    }

    pub fn channel_add_or_remove_user(
        ctx: &Context,
        msg: &Message,
        discord_id: u64,
        add: bool,
        as_host: bool,
        redis_client: redis::Client,
    ) -> Result<(), BoxedError> {
        let mut redis_connection = redis_client.get_connection()?;
        // Check whether this is a bot controlled channel
        let channel_roles = Self::get_channel_roles(msg.channel_id.0, &mut redis_connection)?;
        let channel_roles = match channel_roles {
            Some(roles) => roles,
            None => {
                let _ = msg
                    .channel_id
                    .say(&ctx.http, strings::CHANNEL_NOT_BOT_CONTROLLED);
                return Ok(());
            }
        };
        // This is only for bot_admins and channel hosts
        let is_bot_admin = msg
            .author
            .has_role(
                ctx,
                crate::discord_sync::GUILD_ID,
                crate::discord_sync::BOT_ADMIN_ID,
            )
            .unwrap_or(false);
        let is_host = msg
            .author
            .has_role(ctx, crate::discord_sync::GUILD_ID, channel_roles.host)
            .unwrap_or(false);
        // Only bot admins and channel hosts can add/remove users
        if !is_bot_admin && !is_host {
            let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_CHANNEL_ADMIN);
            return Ok(());
        }
        // Only bot admins can add/remove hosts
        if !is_bot_admin && as_host {
            let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
            return Ok(());
        }
        // Only bot admins can add users
        if !is_bot_admin && add {
            let _ = msg.channel_id.say(&ctx.http, strings::NOT_A_BOT_ADMIN);
            return Ok(());
        }
        if add {
            // Try to add the user to the channel
            match ctx.http.add_member_role(
                crate::discord_sync::GUILD_ID.0,
                discord_id,
                channel_roles.user,
            ) {
                Ok(()) => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, format!("Welcome <@{}>!", discord_id));
                }
                Err(err) => {
                    eprintln!("Could not assign channel role: {}", err);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ROLE_ADD_ERROR);
                }
            }
            if as_host {
                match ctx.http.add_member_role(
                    crate::discord_sync::GUILD_ID.0,
                    discord_id,
                    channel_roles.host,
                ) {
                    Ok(()) => {
                        let _ = msg
                            .channel_id
                            .say(&ctx.http, strings::CHANNEL_ADDED_NEW_HOST(discord_id));
                    }
                    Err(err) => {
                        eprintln!("Could not assign channel role: {}", err);
                        let _ = msg
                            .channel_id
                            .say(&ctx.http, strings::CHANNEL_ROLE_ADD_ERROR);
                    }
                }
            }
            Ok(())
        } else {
            // Try to remove the user from the channel
            match ctx.http.remove_member_role(
                crate::discord_sync::GUILD_ID.0,
                discord_id,
                channel_roles.host,
            ) {
                Ok(()) => {
                    let _ = msg.react(ctx, "\u{2705}");
                }
                Err(err) => {
                    eprintln!("Could not remove host channel role: {}", err);
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, strings::CHANNEL_ROLE_REMOVE_ERROR);
                }
            }
            if !as_host {
                match ctx.http.remove_member_role(
                    crate::discord_sync::GUILD_ID.0,
                    discord_id,
                    channel_roles.user,
                ) {
                    Err(err) => {
                        eprintln!("Could not remove channel role: {}", err);
                        let _ = msg
                            .channel_id
                            .say(&ctx.http, strings::CHANNEL_ROLE_REMOVE_ERROR);
                    }
                    _ => (),
                }
            }
            // Remember which users were removed manually
            if as_host {
                let redis_channel_removed_hosts_key =
                    format!("discord_channel:{}:removed_hosts", msg.channel_id.0);
                redis_connection.sadd(redis_channel_removed_hosts_key, discord_id)?;
            } else {
                let redis_channel_removed_users_key =
                    format!("discord_channel:{}:removed_users", msg.channel_id.0);
                redis_connection.sadd(redis_channel_removed_users_key, discord_id)?
            }
            Ok(())
        }
    }

    pub fn send_welcome_message(ctx: &Context, user: &User) {
        let _ = user.direct_message(ctx, |message_builder| {
            message_builder
                .content(crate::strings::WELCOME_MESSAGE_PART1)
                .embed(|embed_builder| {
                    embed_builder
                        .colour(serenity::utils::Colour::new(0xFF1744))
                        .title(crate::strings::WELCOME_MESSAGE_PART2_EMBED_TITLE)
                        .description(crate::strings::WELCOME_MESSAGE_PART2_EMBED_CONTENT)
                })
        });
    }

    pub fn send_help_message(ctx: &Context, msg: &Message, bot_id: UserId) {
        let is_bot_admin = msg
            .author
            .has_role(
                ctx,
                crate::discord_sync::GUILD_ID,
                crate::discord_sync::BOT_ADMIN_ID,
            )
            .unwrap_or(false);
        let mut dm_result = msg
            .author
            .direct_message(ctx, |message_builder| {
                message_builder
                    .content(crate::strings::HELP_MESSAGE_INTRO(bot_id.0))
                    .embed(|embed_builder| {
                        embed_builder
                            .colour(serenity::utils::Colour::BLUE)
                            .title(crate::strings::HELP_MESSAGE_PLAYER_EMBED_TITLE)
                            .description(crate::strings::HELP_MESSAGE_PLAYER_EMBED_CONTENT)
                    })
            })
            .and_then(|_| {
                msg.author.direct_message(ctx, |message_builder| {
                    message_builder.embed(|embed_builder| {
                        embed_builder
                            .colour(serenity::utils::Colour::DARK_GREEN)
                            .title(crate::strings::HELP_MESSAGE_GM_EMBED_TITLE)
                            .description(crate::strings::HELP_MESSAGE_GM_EMBED_CONTENT(bot_id.0))
                    })
                })
            });
        if is_bot_admin {
            dm_result = dm_result.and_then(|_| {
                msg.author.direct_message(ctx, |message_builder| {
                    message_builder.embed(|embed_builder| {
                        embed_builder
                            .colour(serenity::utils::Colour::from_rgb(255, 23, 68))
                            .title(crate::strings::HELP_MESSAGE_ADMIN_EMBED_TITLE)
                            .description(crate::strings::HELP_MESSAGE_ADMIN_EMBED_CONTENT(bot_id.0))
                    })
                })
            });
        }
        if let Err(err) = dm_result {
            eprintln!("Could not send help message as a DM: {}", err);
        }
        let _ = msg.react(ctx, "\u{2705}");
    }
}

struct ChannelRoles {
    user: u64,
    host: u64,
}
