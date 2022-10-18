use command_macro::command;

#[command]
#[regex(r"stop")]
#[level(admin)]
#[help("stop", "shuts down Hyperion")]
pub fn stop<'a>(
    _context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    eprintln!("STOP command issued");
    std::process::exit(0);
}
