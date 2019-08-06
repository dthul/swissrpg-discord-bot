use redis::{Commands, PipelineCommands};
use regex::Regex;
use serenity::{model::channel::Message, prelude::*};
use simple_error::SimpleError;
use std::borrow::Cow;

const MENTION_PATTERN: &'static str = r"<@(?P<mention_id>[0-9]+)>";

pub struct Regexes {
    pub bot_mention: String,
    pub link_meetup_dm: Regex,
    pub link_meetup_mention: Regex,
    pub link_meetup_organizer_dm: Regex,
    pub link_meetup_organizer_mention: Regex,
    pub unlink_meetup_dm: Regex,
    pub unlink_meetup_mention: Regex,
    pub unlink_meetup_organizer_dm: Regex,
    pub unlink_meetup_organizer_mention: Regex,
    pub sync_meetup_mention: Regex,
    pub sync_discord_mention: Regex,
    pub add_user_mention: Regex,
    pub add_host_mention: Regex,
    pub remove_user_mention: Regex,
    pub remove_host_mention: Regex,
    pub stop_organizer_dm: Regex,
    pub stop_organizer_mention: Regex,
}

impl Regexes {
    pub fn link_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_dm
        } else {
            &self.link_meetup_mention
        }
    }

    pub fn link_meetup_organizer(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.link_meetup_organizer_dm
        } else {
            &self.link_meetup_organizer_mention
        }
    }

    pub fn unlink_meetup(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_dm
        } else {
            &self.unlink_meetup_mention
        }
    }

    pub fn unlink_meetup_organizer(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.unlink_meetup_organizer_dm
        } else {
            &self.unlink_meetup_organizer_mention
        }
    }

    pub fn stop_organizer(&self, is_dm: bool) -> &Regex {
        if is_dm {
            &self.stop_organizer_dm
        } else {
            &self.stop_organizer_mention
        }
    }
}

pub fn compile_regexes(bot_id: u64) -> Regexes {
    let bot_mention = format!(r"<@{}>", bot_id);
    let link_meetup_dm = r"^link[ -]?meetup\s*$";
    let link_meetup_mention = format!(
        r"^{bot_mention}\s+link[ -]?meetup\s*$",
        bot_mention = bot_mention
    );
    let link_meetup_organizer = format!(
        r"link[ -]?meetup\s+{mention_pattern}\s+(?P<meetupid>[0-9]+)",
        mention_pattern = MENTION_PATTERN
    );
    let link_meetup_organizer_dm = format!(
        r"^{link_meetup_organizer}\s*$",
        link_meetup_organizer = link_meetup_organizer
    );
    let link_meetup_organizer_mention = format!(
        r"^{bot_mention}\s+{link_meetup_organizer}\s*$",
        bot_mention = bot_mention,
        link_meetup_organizer = link_meetup_organizer
    );
    let unlink_meetup = r"unlink[ -]?meetup";
    let unlink_meetup_dm = format!(r"^{unlink_meetup}\s*$", unlink_meetup = unlink_meetup);
    let unlink_meetup_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup}\s*$",
        bot_mention = bot_mention,
        unlink_meetup = unlink_meetup
    );
    let unlink_meetup_organizer = format!(
        r"unlink[ -]?meetup\s+{mention_pattern}",
        mention_pattern = MENTION_PATTERN
    );
    let unlink_meetup_organizer_dm = format!(
        r"^{unlink_meetup_organizer}\s*$",
        unlink_meetup_organizer = unlink_meetup_organizer
    );
    let unlink_meetup_organizer_mention = format!(
        r"^{bot_mention}\s+{unlink_meetup_organizer}\s*$",
        bot_mention = bot_mention,
        unlink_meetup_organizer = unlink_meetup_organizer
    );
    let sync_meetup_mention = format!(
        r"^{bot_mention}\s+sync\s+meetup\s*$",
        bot_mention = bot_mention
    );
    let sync_discord_mention = format!(
        r"^{bot_mention}\s+sync\s+discord\s*$",
        bot_mention = bot_mention
    );
    let add_user_mention = format!(
        r"^{bot_mention}\s+add\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let add_host_mention = format!(
        r"^{bot_mention}\s+add\s+host\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let remove_user_mention = format!(
        r"^{bot_mention}\s+remove\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let remove_host_mention = format!(
        r"^{bot_mention}\s+remove\s+host\s+{mention_pattern}\s*$",
        bot_mention = bot_mention,
        mention_pattern = MENTION_PATTERN,
    );
    let stop_organizer_dm = r"^(?i)stop\s*$";
    let stop_organizer_mention =
        format!(r"^{bot_mention}\s+(?i)stop\s*$", bot_mention = bot_mention);
    Regexes {
        bot_mention: bot_mention,
        link_meetup_dm: Regex::new(link_meetup_dm).unwrap(),
        link_meetup_mention: Regex::new(link_meetup_mention.as_str()).unwrap(),
        link_meetup_organizer_dm: Regex::new(link_meetup_organizer_dm.as_str()).unwrap(),
        link_meetup_organizer_mention: Regex::new(link_meetup_organizer_mention.as_str()).unwrap(),
        unlink_meetup_dm: Regex::new(unlink_meetup_dm.as_str()).unwrap(),
        unlink_meetup_mention: Regex::new(unlink_meetup_mention.as_str()).unwrap(),
        unlink_meetup_organizer_dm: Regex::new(unlink_meetup_organizer_dm.as_str()).unwrap(),
        unlink_meetup_organizer_mention: Regex::new(unlink_meetup_organizer_mention.as_str())
            .unwrap(),
        sync_meetup_mention: Regex::new(sync_meetup_mention.as_str()).unwrap(),
        sync_discord_mention: Regex::new(sync_discord_mention.as_str()).unwrap(),
        add_user_mention: Regex::new(add_user_mention.as_str()).unwrap(),
        add_host_mention: Regex::new(add_host_mention.as_str()).unwrap(),
        remove_user_mention: Regex::new(remove_user_mention.as_str()).unwrap(),
        remove_host_mention: Regex::new(remove_host_mention.as_str()).unwrap(),
        stop_organizer_dm: Regex::new(stop_organizer_dm).unwrap(),
        stop_organizer_mention: Regex::new(stop_organizer_mention.as_str()).unwrap(),
    }
}

impl crate::discord_bot::Handler {
    pub fn link_meetup(
        ctx: &Context,
        msg: &Message,
        regexes: &Regexes,
        user_id: u64,
    ) -> crate::Result<()> {
        let redis_key_d2m = format!("discord_user:{}:meetup_user", user_id);
        let (redis_connection_mutex, meetup_client_mutex) = {
            let data = ctx.data.read();
            (
                data.get::<crate::discord_bot::RedisConnectionKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Redis connection was not set")))?
                    .clone(),
                data.get::<crate::discord_bot::MeetupClientKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Meetup client was not set")))?
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
            match *meetup_client_mutex.read() {
                Some(ref meetup_client) => {
                    match meetup_client.get_member_profile(Some(linked_meetup_id))? {
                        Some(user) => {
                            let _ = msg.author.direct_message(ctx, |message| {
                                message.content(format!(
                                    "You are already linked to {}'s Meetup account. \
                                     If you really want to change this, unlink your currently \
                                     linked meetup account first by writing:\n\
                                     {} unlink meetup",
                                    user.name, regexes.bot_mention
                                ))
                            });
                            let _ = msg.react(ctx, "\u{2705}");
                        }
                        _ => {
                            let _ = msg.author.direct_message(ctx, |message| {
                                message.content(format!(
                                    "You are linked to a seemingly non-existent Meetup account. \
                                     If you want to change this, unlink the currently \
                                     linked meetup account by writing:\n\
                                     {} unlink meetup",
                                    regexes.bot_mention
                                ))
                            });
                            let _ = msg.react(ctx, "\u{2705}");
                        }
                    }
                }
                _ => {
                    let _ = msg.author.direct_message(ctx, |message| {
                        message.content(format!(
                            "You are already linked to a Meetup account. \
                             If you really want to change this, unlink your currently \
                             linked meetup account first by writing:\n\
                             {} unlink meetup",
                            regexes.bot_mention
                        ))
                    });
                    let _ = msg.react(ctx, "\u{2705}");
                }
            }
            return Ok(());
        }
        let url =
            crate::meetup_oauth2::generate_meetup_linking_link(&redis_connection_mutex, user_id)?;
        let dm = msg.author.direct_message(ctx, |message| {
            message.content(format!(
                "Visit the following website to link your Meetup profile: {}\n\
                 ***This is a private, ephemeral, one-time use link and meant just for you.***\n\
                 Don't share it with others or they might link your Discord account to their Meetup profile.",
                url
            ))
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

    pub fn link_meetup_organizer(
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
                    .ok_or_else(|| Box::new(SimpleError::new("Redis connection was not set")))?
                    .clone(),
                data.get::<crate::discord_bot::MeetupClientKey>()
                    .ok_or_else(|| Box::new(SimpleError::new("Meetup client was not set")))?
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
                        "<@{discord_id}> is already linked to a different Meetup account. \
                         If you want to change this, unlink the currently \
                         linked Meetup account first by writing:\n\
                         {bot_mention} unlink meetup <@{discord_id}>",
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
                    "This Meetup account is alread linked to <@{linked_discord_id}>. \
                     If you want to change this, unlink the Meetup account first \
                     by writing\n\
                     {bot_mention} unlink meetup <@{linked_discord_id}>",
                    linked_discord_id = linked_discord_id,
                    bot_mention = regexes.bot_mention
                ))
            });
            return Ok(());
        }
        // The user has not yet linked their meetup account.
        // Test whether the specified Meetup user actually exists.
        let meetup_user = meetup_client_mutex
            .read()
            .as_ref()
            .ok_or_else(|| SimpleError::new("Meetup API unavailable"))?
            .get_member_profile(Some(meetup_id))?;
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
        is_organizer_command: bool,
        user_id: u64,
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
                let message = if is_organizer_command {
                    Cow::Owned(format!("Unlinked <@{}>'s Meetup account", user_id))
                } else {
                    Cow::Borrowed("Unlinked your Meetup account")
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
            None => {
                let message = if is_organizer_command {
                    Cow::Owned(format!(
                        "There was seemingly no meetup account linked to <@{}>",
                        user_id
                    ))
                } else {
                    Cow::Borrowed("There was seemingly no meetup account linked to you")
                };
                let _ = msg.channel_id.say(&ctx.http, message);
            }
        }
        Ok(())
    }

    pub fn channel_add_or_remove_user(
        ctx: &Context,
        msg: &Message,
        discord_id: u64,
        add: bool,
        as_host: bool,
        mut redis_client: redis::Client,
    ) {
        // Check that this message came from a bot controlled channel
        let (channel_role, channel_host_role) = {
            let redis_channel_role_key = format!("discord_channel:{}:discord_role", msg.channel_id);
            let redis_channel_host_role_key =
                format!("discord_channel:{}:discord_host_role", msg.channel_id);
            let channel_roles: redis::RedisResult<(Option<u64>, Option<u64>)> = redis::pipe()
                .get(redis_channel_role_key)
                .get(redis_channel_host_role_key)
                .query(&mut redis_client);
            match channel_roles {
                Ok((Some(role), Some(host_role))) => (role, host_role),
                Ok((None, None)) => {
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        "This channel does not seem to be under my control",
                    );
                    return;
                }
                Ok(_) => {
                    let _ = msg
                        .channel_id
                        .say(&ctx.http, "It seems like this channel is broken");
                    return;
                }
                Err(err) => {
                    eprintln!("{}", err);
                    let _ = msg.channel_id.say(&ctx.http, "Something went wrong");
                    return;
                }
            }
        };
        // This is only for organizers and channel hosts
        let is_organizer = msg
            .author
            .has_role(
                ctx,
                crate::discord_sync::GUILD_ID,
                crate::discord_sync::ORGANIZER_ID,
            )
            .unwrap_or(false);
        let is_host = msg
            .author
            .has_role(ctx, crate::discord_sync::GUILD_ID, channel_host_role)
            .unwrap_or(false);
        if !is_organizer && !is_host {
            let _ = msg
                .channel_id
                .say(&ctx.http, "Only channel hosts and organizers can do that");
            return;
        }
        if add {
            // Try to add the user to the channel
            match ctx.http.add_member_role(
                crate::discord_sync::GUILD_ID.0,
                discord_id,
                channel_role,
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
                        .say(&ctx.http, "Something went wrong assigning the channel role");
                }
            }
            if as_host {
                match ctx.http.add_member_role(
                    crate::discord_sync::GUILD_ID.0,
                    discord_id,
                    channel_host_role,
                ) {
                    Ok(()) => {
                        let _ = msg.channel_id.say(
                            &ctx.http,
                            format!("<@{}> is now a host of this channel", discord_id),
                        );
                    }
                    Err(err) => {
                        eprintln!("Could not assign channel role: {}", err);
                        let _ = msg.channel_id.say(
                            &ctx.http,
                            "Something went wrong assigning the channel host role",
                        );
                    }
                }
            }
        } else {
            // Try to remove the user from the channel
            match ctx.http.remove_member_role(
                crate::discord_sync::GUILD_ID.0,
                discord_id,
                channel_host_role,
            ) {
                Err(err) => {
                    eprintln!("Could not remove host channel role: {}", err);
                    let _ = msg.channel_id.say(
                        &ctx.http,
                        "Something went wrong removing the channel host role",
                    );
                }
                _ => (),
            }
            if !as_host {
                match ctx.http.remove_member_role(
                    crate::discord_sync::GUILD_ID.0,
                    discord_id,
                    channel_role,
                ) {
                    Err(err) => {
                        eprintln!("Could not remove channel role: {}", err);
                        let _ = msg
                            .channel_id
                            .say(&ctx.http, "Something went wrong removing the channel role");
                    }
                    _ => (),
                }
            }
            // Remember which users were removed manually
            if as_host {
                let redis_channel_removed_hosts_key =
                    format!("discord_channel:{}:removed_hosts", msg.channel_id.0);
                match redis_client.sadd(redis_channel_removed_hosts_key, discord_id) {
                    Ok(()) => (),
                    Err(err) => {
                        eprintln!("Redis error when trying to record removed host: {}", err);
                    }
                }
            } else {
                let redis_channel_removed_users_key =
                    format!("discord_channel:{}:removed_users", msg.channel_id.0);
                match redis_client.sadd(redis_channel_removed_users_key, discord_id) {
                    Ok(()) => (),
                    Err(err) => {
                        eprintln!("Redis error when trying to record removed user: {}", err);
                    }
                }
            }
        }
    }
}
