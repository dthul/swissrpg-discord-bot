use command_macro::command;

#[command]
#[regex(r"test")]
#[level(admin)]
pub fn test<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content(lib::strings::WELCOME_MESSAGE)
        })
        .await?;
    Ok(())
}
