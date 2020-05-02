use command_macro::command;

#[command]
#[regex(r"sync\s*subscriptions")]
#[level(admin)]
fn sync_subscriptions(
    context: super::CommandContext<'_>,
    _: regex::Captures<'_>,
) -> Result<(), lib::meetup::Error> {
    let runtime_mutex = context.async_runtime()?;
    let runtime_guard = futures::executor::block_on(runtime_mutex.read());
    if let Some(ref async_runtime) = *runtime_guard {
        let stripe_client = context.stripe_client()?.clone();
        let discord_api = context.ctx.into();
        async_runtime.enter(|| {
            async_runtime.spawn(async move {
                lib::tasks::subscription_roles::update_roles(&discord_api, &stripe_client).await
            })
        });
        let _ = context.msg.channel_id.say(context.ctx, "Copy that");
    }
    Ok(())
}
