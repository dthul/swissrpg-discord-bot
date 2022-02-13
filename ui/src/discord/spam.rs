use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use redis::AsyncCommands;
use serenity::{model::id::ChannelId, prelude::*};
use std::sync::Arc;

type SpamList = Arc<(Vec<String>, AhoCorasick)>;
struct GameChannelsList {
    channel_ids: Vec<ChannelId>,
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
    let alert_channel_id = if let Some(channel_id) = lib::discord::sync::ids::BOT_ALERTS_CHANNEL_ID
    {
        channel_id
    } else {
        return Ok(());
    };
    // Ignore messages from game channels
    let game_channels = get_game_channels_list(cmdctx).await?;
    if game_channels.channel_ids.contains(&cmdctx.msg.channel_id) {
        return Ok(());
    }
    let spam_list = get_spam_list(cmdctx).await?;
    let (word_list, spam_matcher) = &*spam_list;
    if let Some(mat) = spam_matcher.find(&cmdctx.msg.content) {
        let word = &word_list[mat.pattern()];
        let mut msg = serenity::utils::MessageBuilder::new();
        msg.push_bold("Spam Alert ");
        if let Some(admin_role_id) = lib::discord::sync::ids::ADMIN_ROLE_ID {
            msg.mention(&admin_role_id);
        }
        msg.push("\nTrigger: ");
        msg.push_line_safe(word);
        msg.push("User: ");
        msg.mention(&cmdctx.msg.author.id);
        msg.push("\nMessage: ");
        msg.push_line_safe(&cmdctx.msg.content);
        msg.push(format!(
            "https://discordapp.com/channels/{guild_id}/{channel_id}/{message_id}",
            guild_id = lib::discord::sync::ids::GUILD_ID.0,
            channel_id = cmdctx.msg.channel_id,
            message_id = cmdctx.msg.id.0
        ));
        alert_channel_id.say(&cmdctx.ctx, &msg).await?;
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
    // There is no spam list in the data map yet -> query it and store it in the data map
    // Query the spam list from Redis
    let redis_connection = cmdctx.async_redis_connection().await?;
    let word_list: Vec<String> = redis_connection.lrange("spam_word_list", 0, -1).await?;
    let word_matcher = AhoCorasickBuilder::new()
        .auto_configure(&word_list)
        .ascii_case_insensitive(true)
        .dfa(true)
        .build(&word_list);
    let spam_list = Arc::new((word_list, word_matcher));
    cmdctx
        .ctx
        .data
        .write()
        .await
        .insert::<SpamListKey>(spam_list.clone());
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
    // There is no games list in the data map yet or it is outdated -> query it and store it in the data map
    // Query the channel list from the database
    let pool = cmdctx.pool().await?;
    let channel_ids = sqlx::query!(r#"SELECT discord_id FROM event_series_text_channel"#)
        .map(|row| ChannelId(row.discord_id as u64))
        .fetch_all(&pool)
        .await?;
    let channels_list = Arc::new(GameChannelsList {
        channel_ids,
        last_updated: std::time::Instant::now(),
    });
    cmdctx
        .ctx
        .data
        .write()
        .await
        .insert::<GameChannelsListKey>(channels_list.clone());
    Ok(channels_list)
}
