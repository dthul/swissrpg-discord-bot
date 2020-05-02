use command_macro::command;

// TODO: auto-generate the help command
#[command]
#[regex(r"help")]
fn help(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    let is_bot_admin = context.is_admin().unwrap_or(false);
    let bot_id = context.bot_id()?;
    let mut dm_result = context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder
                .content(lib::strings::HELP_MESSAGE_INTRO(bot_id.0))
                .embed(|embed_builder| {
                    embed_builder
                        .colour(serenity::utils::Colour::BLUE)
                        .title(lib::strings::HELP_MESSAGE_PLAYER_EMBED_TITLE)
                        .description(lib::strings::HELP_MESSAGE_PLAYER_EMBED_CONTENT)
                })
        })
        .and_then(|_| {
            context
                .msg
                .author
                .direct_message(context.ctx, |message_builder| {
                    message_builder.embed(|embed_builder| {
                        embed_builder
                            .colour(serenity::utils::Colour::DARK_GREEN)
                            .title(lib::strings::HELP_MESSAGE_GM_EMBED_TITLE)
                            .description(lib::strings::HELP_MESSAGE_GM_EMBED_CONTENT(bot_id.0))
                    })
                })
        });
    if is_bot_admin {
        dm_result = dm_result.and_then(|_| {
            context
                .msg
                .author
                .direct_message(context.ctx, |message_builder| {
                    message_builder.embed(|embed_builder| {
                        embed_builder
                            .colour(serenity::utils::Colour::from_rgb(255, 23, 68))
                            .title(lib::strings::HELP_MESSAGE_ADMIN_EMBED_TITLE)
                            .description(lib::strings::HELP_MESSAGE_ADMIN_EMBED_CONTENT(bot_id.0))
                    })
                })
        });
    }
    Ok(dm_result.map(|_| ())?)
}
