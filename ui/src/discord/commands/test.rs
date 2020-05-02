use command_macro::command;

#[command]
#[regex(r"test")]
#[level(admin)]
pub fn test(context: super::CommandContext, _: regex::Captures) -> Result<(), lib::meetup::Error> {
    context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content(lib::strings::WELCOME_MESSAGE)
        })?;
    Ok(())
}
