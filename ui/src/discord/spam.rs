use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use redis::Commands;
use serenity::model::{channel::PermissionOverwriteType, id::RoleId, permissions::Permissions};
use serenity::{model::channel::Message, prelude::*};
use std::sync::Arc;

type SpamList = Arc<RwLock<(Vec<String>, AhoCorasick)>>;

pub(crate) struct SpamListKey;
impl TypeMapKey for SpamListKey {
    type Value = SpamList;
}

pub fn message_hook(ctx: &Context, msg: &Message) -> Result<(), lib::meetup::Error> {
    // Check whether this channel is public (otherwise return)
    let is_private_channel = msg
        .channel(&ctx)
        .map(|channel| channel.guild())
        .flatten()
        .map(|channel| {
            let permissions = &channel.read().permission_overwrites;
            permissions
                .iter()
                .find(|perm| {
                    perm.kind
                        == PermissionOverwriteType::Role(RoleId(
                            lib::discord::sync::ids::GUILD_ID.0,
                        ))
                })
                .map_or(false, |perm| perm.deny.contains(Permissions::READ_MESSAGES))
        })
        .unwrap_or(true);
    if is_private_channel {
        return Ok(());
    }
    let spam_list = get_spam_list(ctx)?;
    let (word_list, spam_matcher) = &*spam_list.read();
    if let Some(mat) = spam_matcher.find(&msg.content) {
        let word = &word_list[mat.pattern()];
        for user_id in lib::discord::sync::ids::SPAM_ALERT_USER_IDS {
            user_id.to_user_cached(ctx).map(|user| {
                user.read().direct_message(ctx, |builder| {
                    builder.content(format!(
                        "**Spam Alert**\nTrigger: {trigger}\nUser: <@{user_id}>\nMessage: {msg}\nhttps://discordapp.com/channels/{guild_id}/{channel_id}/{message_id}",
                        trigger=word,
                        user_id=msg.author.id.0,
                        msg=msg.content,
                        guild_id=lib::discord::sync::ids::GUILD_ID.0,
                        channel_id=msg.channel_id,
                        message_id=msg.id.0))
                })
            });
        }
    }
    Ok(())
}

fn get_spam_list(ctx: &Context) -> Result<SpamList, lib::meetup::Error> {
    // Check if the spam list is already in the data map
    if let Some(spam_list) = ctx.data.read().get::<SpamListKey>() {
        return Ok(spam_list.clone());
    }
    // There is no spam list in the data map yet -> get a write lock on the data map
    let mut data_lock = ctx.data.write();
    // Check one more time if the spam list has maybe been loaded in the mean time
    if let Some(spam_list) = data_lock.get::<SpamListKey>() {
        return Ok(spam_list.clone());
    }
    // Still no spam list in the data map -> let's add it!
    // Query the spam list form Redis
    let redis_client = data_lock
        .get_mut::<super::bot::RedisClientKey>()
        .ok_or_else(|| {
            simple_error::SimpleError::new(
                "SpamListKey entry not present. This should never happen.",
            )
        })?;
    let word_list: Vec<String> = redis_client.lrange("spam_word_list", 0, -1)?;
    let word_matcher = AhoCorasickBuilder::new()
        .auto_configure(&word_list)
        .ascii_case_insensitive(true)
        .dfa(true)
        .build(&word_list);
    let spam_list = Arc::new(RwLock::new((word_list, word_matcher)));
    data_lock.insert::<SpamListKey>(spam_list.clone());
    Ok(spam_list)
}
