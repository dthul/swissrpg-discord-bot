use command_macro::command;

#[command]
#[regex(r"sync\s*subscriptions")]
#[level(admin)]
fn sync_subscriptions<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    let stripe_client = context.stripe_client()?.clone();
    let discord_api = (&context.ctx).into();
    tokio::spawn(async move {
        lib::tasks::subscription_roles::update_roles(&discord_api, &stripe_client).await
    });
    let _ = context.msg.channel_id.say(context.ctx, "Copy that");
    Ok(())
}
