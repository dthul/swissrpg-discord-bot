use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use redis::AsyncCommands;
use serenity::prelude::*;
use std::sync::Arc;

type SpamList = Arc<(Vec<String>, AhoCorasick)>;
struct GameChannelsList {
    channel_ids: Vec<u64>,
    last_updated: std::time::Instant,
}

struct SpamListKey;
impl TypeMapKey for SpamListKey {
    type Value = SpamList;
}

struct GameChannelsListKey;
impl TypeMapKey for GameChannelsListKey {
    type Value = Arc<GameChannelsList>;
}

pub async fn message_hook(
    cmdctx: &mut super::commands::CommandContext,
) -> Result<(), lib::meetup::Error> {
    // Ignore messages from game channels
    let game_channels = get_game_channels_list(cmdctx).await?;
    if game_channels.channel_ids.contains(&cmdctx.msg.channel_id.0) {
        return Ok(());
    }
    let spam_list = get_spam_list(cmdctx).await?;
    let (word_list, spam_matcher) = &*spam_list;
    if let Some(mat) = spam_matcher.find(&cmdctx.msg.content) {
        let word = &word_list[mat.pattern()];
        let msg = format!(
            "**Spam Alert**\nTrigger: {trigger}\nUser: <@{user_id}>\nMessage: {msg}\nhttps://discordapp.com/channels/{guild_id}/{channel_id}/{message_id}",
            trigger=word,
            user_id=cmdctx.msg.author.id.0,
            msg=cmdctx.msg.content,
            guild_id=lib::discord::sync::ids::GUILD_ID.0,
            channel_id=cmdctx.msg.channel_id,
            message_id=cmdctx.msg.id.0);
        for user_id in lib::discord::sync::ids::SPAM_ALERT_USER_IDS {
            if let Some(user) = user_id.to_user_cached(&cmdctx.ctx).await {
                user.direct_message(&cmdctx.ctx, |builder| builder.content(&msg))
                    .await
                    .ok();
            }
        }
    }
    Ok(())
}

async fn get_spam_list(
    cmdctx: &mut super::commands::CommandContext,
) -> Result<SpamList, lib::meetup::Error> {
    // Check if the spam list is already in the data map
    if let Some(spam_list) = cmdctx.ctx.data.read().await.get::<SpamListKey>() {
        return Ok(spam_list.clone());
    }
    // There is no spam list in the data map yet -> get a write lock on the data map
    let data_map = cmdctx.ctx.data.clone();
    let mut data_lock = data_map.write().await;
    // Check one more time if the spam list has maybe been loaded in the mean time
    if let Some(spam_list) = data_lock.get::<SpamListKey>() {
        return Ok(spam_list.clone());
    }
    // Still no spam list in the data map -> let's add it!
    // Query the spam list form Redis
    let redis_connection = cmdctx.async_redis_connection().await?;
    let word_list: Vec<String> = redis_connection.lrange("spam_word_list", 0, -1).await?;
    let word_matcher = AhoCorasickBuilder::new()
        .auto_configure(&word_list)
        .ascii_case_insensitive(true)
        .dfa(true)
        .build(&word_list);
    let spam_list = Arc::new((word_list, word_matcher));
    data_lock.insert::<SpamListKey>(spam_list.clone());
    Ok(spam_list)
}

async fn get_game_channels_list(
    cmdctx: &mut super::commands::CommandContext,
) -> Result<Arc<GameChannelsList>, lib::meetup::Error> {
    // Check if the channels list is already in the data map and up-to-date
    if let Some(games_list) = cmdctx.ctx.data.read().await.get::<GameChannelsListKey>() {
        if (std::time::Instant::now() - games_list.last_updated)
            < std::time::Duration::from_secs(5 * 60)
        {
            return Ok(games_list.clone());
        }
    }
    // There is no games list in the data map yet or it is outdated -> get a write lock on the data map
    let data_map = cmdctx.ctx.data.clone();
    let mut data_lock = data_map.write().await;
    // Query the channel list form Redis
    let redis_connection = cmdctx.async_redis_connection().await?;
    let channel_ids: Vec<u64> = redis_connection.smembers("discord_channels").await?;
    let channels_list = Arc::new(GameChannelsList {
        channel_ids,
        last_updated: std::time::Instant::now(),
    });
    data_lock.insert::<GameChannelsListKey>(channels_list.clone());
    Ok(channels_list)
}
