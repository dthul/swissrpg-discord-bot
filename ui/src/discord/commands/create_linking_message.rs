use command_macro::command;
use serenity::model::interactions::ButtonStyle;

#[command]
#[regex(r"create\s*linking\s*message")]
#[level(admin)]
#[help(
    "create linking message",
    "Creates a message in this channel which users can use to link their Meetup accounts"
)]
fn create_linking_message<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    context
        .msg
        .channel_id
        .send_message(&context.ctx, |m| {
            m.content("Link now with your Meetup account!")
                .components(|c| {
                    c.create_action_row(|a| {
                        a.create_button(|b| {
                            b.label("Link with Meetup")
                                .style(ButtonStyle::Primary)
                                .custom_id("link-meetup")
                        })
                    })
                })
        })
        .await?;
    Ok(())
}
