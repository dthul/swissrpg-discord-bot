use command_macro::command;

#[command]
#[regex(r"stop")]
#[level(admin)]
pub fn stop(context: super::CommandContext, _: regex::Captures) -> Result<(), lib::meetup::Error> {
    match std::process::Command::new("sudo")
        .args(&["systemctl", "stop", "bot"])
        .output()
    {
        Ok(_) => {
            eprintln!("STOP command issued");
            let _ = context.msg.channel_id.say(&context.ctx, "Shutting down");
            Ok(())
        }
        Err(err) => {
            eprintln!("Error when trying to issue a STOP command:\n{:#?}", err);
            Err(simple_error::SimpleError::new("Could not shut down the bot").into())
        }
    }
}
