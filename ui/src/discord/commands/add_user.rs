use command_macro::command;
use lib::discord::CacheAndHttp;
use redis::Commands;
use serenity::model::channel::PermissionOverwriteType;
use serenity::model::id::UserId;
use serenity::model::permissions::Permissions;

#[command]
#[regex(r"add\s+{mention_pattern}", mention_pattern)]
#[level(host)]
fn add_user(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    // Get the Discord ID of the user that is supposed to
    // be added to the channel
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    channel_add_or_remove_user_impl(
        &mut context,
        discord_id,
        /*add*/ true,
        /*as_host*/ false,
    )
}

#[command]
#[regex(r"add\s*host\s+{mention_pattern}", mention_pattern)]
#[level(host)]
fn add_host(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    // Get the Discord ID of the user that is supposed to
    // be added to the channel
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    channel_add_or_remove_user_impl(
        &mut context,
        discord_id,
        /*add*/ true,
        /*as_host*/ true,
    )
}

#[command]
#[regex(r"remove\s+{mention_pattern}", mention_pattern)]
#[level(host)]
fn remove_user(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    // Get the Discord ID of the user that is supposed to
    // be added to the channel
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    channel_add_or_remove_user_impl(
        &mut context,
        discord_id,
        /*add*/ false,
        /*as_host*/ false,
    )
}

#[command]
#[regex(r"remove\s*host\s+{mention_pattern}", mention_pattern)]
#[level(host)]
fn remove_host(
    mut context: super::CommandContext,
    captures: regex::Captures,
) -> Result<(), lib::meetup::Error> {
    // Get the Discord ID of the user that is supposed to
    // be added to the channel
    let discord_id = captures.name("mention_id").unwrap().as_str();
    // Try to convert the specified ID to an integer
    let discord_id = match discord_id.parse::<u64>() {
        Ok(id) => id,
        _ => {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::CHANNEL_ADD_USER_INVALID_DISCORD);
            return Ok(());
        }
    };
    channel_add_or_remove_user_impl(
        &mut context,
        discord_id,
        /*add*/ false,
        /*as_host*/ true,
    )
}

fn channel_add_or_remove_user_impl(
    context: &mut super::CommandContext,
    discord_id: u64,
    add: bool,
    as_host: bool,
) -> Result<(), lib::meetup::Error> {
    // Check whether this is a bot controlled channel
    let is_game_channel = context.is_game_channel()?;
    let is_managed_channel = context.is_managed_channel()?;
    let is_bot_admin = context.is_admin()?;
    // Only bot admins can add/remove hosts
    if !is_bot_admin && as_host {
        let _ = context
            .msg
            .channel_id
            .say(context.ctx, lib::strings::NOT_A_BOT_ADMIN);
        return Ok(());
    }
    // Managed channels and hosts don't use roles but user-specific permission overwrites
    let is_host = context.is_host()?;
    let discord_api: CacheAndHttp = context.ctx.into();
    if is_game_channel && !is_managed_channel {
        let channel_roles =
            lib::get_channel_roles(context.msg.channel_id.0, context.redis_connection()?)?;
        let channel_roles = match channel_roles {
            Some(roles) => roles,
            None => {
                let _ = context
                    .msg
                    .channel_id
                    .say(context.ctx, lib::strings::CHANNEL_NOT_BOT_CONTROLLED);
                return Ok(());
            }
        };
        // Only bot admins can add users
        if !is_bot_admin && add {
            let _ = context
                .msg
                .channel_id
                .say(context.ctx, lib::strings::NOT_A_BOT_ADMIN);
            return Ok(());
        }
        // Figure out whether there is a voice channel
        let voice_channel_id = match lib::get_channel_voice_channel(
            context.msg.channel_id,
            context.redis_connection()?,
        ) {
            Ok(id) => id,
            Err(err) => {
                eprintln!(
                    "Could not figure out whether this channel has a voice channel:\n{:#?}",
                    err
                );
                None
            }
        };
        if add {
            // Try to add the user to the channel
            match context.ctx.http.add_member_role(
                lib::discord::sync::ids::GUILD_ID.0,
                discord_id,
                channel_roles.user,
            ) {
                Ok(()) => {
                    let _ = context.msg.react(context.ctx, "\u{2705}");
                    let _ = context
                        .msg
                        .channel_id
                        .say(context.ctx, format!("Welcome <@{}>!", discord_id));
                }
                Err(err) => {
                    eprintln!("Could not assign channel role: {}", err);
                    let _ = context
                        .msg
                        .channel_id
                        .say(context.ctx, lib::strings::CHANNEL_ROLE_ADD_ERROR);
                }
            }
            if as_host {
                // Grant direct permissions
                let new_permissions = Permissions::READ_MESSAGES
                    | Permissions::MENTION_EVERYONE
                    | Permissions::MANAGE_MESSAGES;
                match lib::discord::add_channel_user_permissions(
                    &discord_api,
                    context.msg.channel_id,
                    UserId(discord_id),
                    new_permissions,
                ) {
                    Err(err) => {
                        eprintln!("Could not assign channel permissions:\n{:#?}", err);
                        let _ = context.msg.channel_id.say(
                            context.ctx,
                            "Something went wrong assigning the channel permissions",
                        );
                    }
                    Ok(true) => {
                        let _ = context.msg.channel_id.say(
                            context.ctx,
                            lib::strings::CHANNEL_ADDED_NEW_HOST(discord_id),
                        );
                    }
                    Ok(false) => (),
                }
                // Also grant permissions in a possibly existing voice channel
                if let Some(voice_channel_id) = voice_channel_id {
                    let new_permissions = Permissions::READ_MESSAGES
                        | Permissions::CONNECT
                        | Permissions::MUTE_MEMBERS
                        | Permissions::DEAFEN_MEMBERS
                        | Permissions::MOVE_MEMBERS
                        | Permissions::PRIORITY_SPEAKER;
                    if let Err(err) = lib::discord::add_channel_user_permissions(
                        &discord_api,
                        voice_channel_id,
                        UserId(discord_id),
                        new_permissions,
                    ) {
                        eprintln!("Could not assign voice channel permissions:\n{:#?}", err);
                        let _ = context.msg.channel_id.say(
                            context.ctx,
                            "Something went wrong assigning the voice channel permissions",
                        );
                    }
                }
            }
        } else {
            // Try to remove the user from the channel
            if let Some(host_role) = channel_roles.host {
                if let Err(err) = context.ctx.http.remove_member_role(
                    lib::discord::sync::ids::GUILD_ID.0,
                    discord_id,
                    host_role,
                ) {
                    eprintln!("Could not remove host channel role:\n{:#?}", err);
                    let _ = context
                        .msg
                        .channel_id
                        .say(context.ctx, lib::strings::CHANNEL_ROLE_REMOVE_ERROR);
                }
            }
            if as_host {
                // Reduce direct permissions
                let permissions_to_remove =
                    Permissions::MANAGE_MESSAGES | Permissions::MENTION_EVERYONE;
                if let Err(err) = lib::discord::remove_channel_user_permissions(
                    &discord_api,
                    context.msg.channel_id,
                    UserId(discord_id),
                    permissions_to_remove,
                ) {
                    eprintln!("Could not reduce channel permissions:\n{:#?}", err);
                    let _ = context.msg.channel_id.say(
                        context.ctx,
                        "Something went wrong reducing the channel permissions",
                    );
                }
                // Also reduce direct permissions in a possibly existing voice channel
                if let Some(voice_channel_id) = voice_channel_id {
                    let permissions_to_remove = Permissions::MUTE_MEMBERS
                        | Permissions::DEAFEN_MEMBERS
                        | Permissions::MOVE_MEMBERS
                        | Permissions::PRIORITY_SPEAKER;
                    if let Err(err) = lib::discord::remove_channel_user_permissions(
                        &discord_api,
                        voice_channel_id,
                        UserId(discord_id),
                        permissions_to_remove,
                    ) {
                        eprintln!("Could not reduce voice channel permissions:\n{:#?}", err);
                        let _ = context.msg.channel_id.say(
                            context.ctx,
                            "Something went wrong reducing the voice channel permissions",
                        );
                    }
                }
            } else {
                // Remove user completely
                // Remove direct permissions
                if let Err(err) = context.msg.channel_id.delete_permission(
                    context.ctx,
                    PermissionOverwriteType::Member(UserId(discord_id)),
                ) {
                    eprintln!("Could not remove channel permissions:\n{:#?}", err);
                    let _ = context.msg.channel_id.say(
                        context.ctx,
                        "Something went wrong revoking the channel permissions",
                    );
                }
                // Also remove permissions from a possibly existing voice channel
                if let Some(voice_channel_id) = voice_channel_id {
                    if let Err(err) = voice_channel_id.delete_permission(
                        context.ctx,
                        PermissionOverwriteType::Member(UserId(discord_id)),
                    ) {
                        eprintln!("Could not revoke voice channel permissions:\n{:#?}", err);
                        let _ = context.msg.channel_id.say(
                            context.ctx,
                            "Something went wrong revoking the voice channel permissions",
                        );
                    }
                }
                match context.ctx.http.remove_member_role(
                    lib::discord::sync::ids::GUILD_ID.0,
                    discord_id,
                    channel_roles.user,
                ) {
                    Err(err) => {
                        eprintln!("Could not remove channel role: {}", err);
                        let _ = context
                            .msg
                            .channel_id
                            .say(context.ctx, lib::strings::CHANNEL_ROLE_REMOVE_ERROR);
                    }
                    _ => (),
                }
            }
            let _ = context.msg.react(context.ctx, "\u{2705}");
            // Remember which users were removed manually
            if as_host {
                let redis_channel_removed_hosts_key =
                    format!("discord_channel:{}:removed_hosts", context.msg.channel_id.0);
                context
                    .redis_connection()?
                    .sadd(redis_channel_removed_hosts_key, discord_id)?;
            } else {
                let redis_channel_removed_users_key =
                    format!("discord_channel:{}:removed_users", context.msg.channel_id.0);
                context
                    .redis_connection()?
                    .sadd(redis_channel_removed_users_key, discord_id)?
            }
        }
    } else if is_managed_channel && !is_game_channel {
        if add {
            let new_permissions = if as_host {
                // Add normal and host specific permissions
                Permissions::READ_MESSAGES
                    | Permissions::MANAGE_MESSAGES
                    | Permissions::MENTION_EVERYONE
            } else {
                // Add only user permissions
                Permissions::READ_MESSAGES
            };
            let permissions_changed = lib::discord::add_channel_user_permissions(
                &discord_api,
                context.msg.channel_id,
                UserId(discord_id),
                new_permissions,
            )?;
            if permissions_changed {
                let _ = context
                    .msg
                    .channel_id
                    .say(context.ctx, format!("Welcome <@{}>!", discord_id));
            }
            let _ = context.msg.react(context.ctx, "\u{2705}");
        } else {
            // Assume that users with the READ_MESSAGES, MANAGE_MESSAGES and
            // MENTION_EVERYONE permission are channel hosts
            let target_is_host = lib::discord::is_host(
                &discord_api,
                context.msg.channel_id,
                UserId(discord_id),
                context.redis_connection()?,
            )?;
            if target_is_host && !is_bot_admin {
                let _ = context
                    .msg
                    .channel_id
                    .say(context.ctx, lib::strings::NOT_A_BOT_ADMIN);
                return Ok(());
            }
            let permissions_to_remove = if as_host {
                // Remove only host specific permissions
                Permissions::MANAGE_MESSAGES | Permissions::MENTION_EVERYONE
            } else {
                // Remove host and user permissions
                Permissions::READ_MESSAGES
                    | Permissions::MANAGE_MESSAGES
                    | Permissions::MENTION_EVERYONE
            };
            lib::discord::remove_channel_user_permissions(
                &discord_api,
                context.msg.channel_id,
                UserId(discord_id),
                permissions_to_remove,
            )?;
            let _ = context.msg.react(context.ctx, "\u{2705}");
        }
    }
    Ok(())
}
