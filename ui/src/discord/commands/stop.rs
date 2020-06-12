use command_macro::command;

#[command]
#[regex(r"stop")]
#[level(admin)]
pub fn stop<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    match std::process::Command::new("sudo")
        .args(&["systemctl", "stop", "bot"])
        .output()
    {
        Ok(_) => {
            eprintln!("STOP command issued");
            context
                .msg
                .channel_id
                .say(&context.ctx, "Shutting down")
                .await
                .ok();
            Ok(())
        }
        Err(err) => {
            eprintln!("Error when trying to issue a STOP command:\n{:#?}", err);
            Err(simple_error::SimpleError::new("Could not shut down the bot").into())
        }
    }
}
