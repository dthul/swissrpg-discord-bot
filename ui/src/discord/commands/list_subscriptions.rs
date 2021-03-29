use command_macro::command;

#[command]
#[regex(r"list\s*subscriptions")]
#[level(admin)]
#[help("list subscriptions", "returns a list of active Stripe subscriptions")]
fn list_subscriptions<'a>(
    context: &'a mut super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    context
        .msg
        .author
        .direct_message(&context.ctx, |message_builder| {
            message_builder.content("Sure! This might take a moment...")
        })
        .await
        .ok();
    let stripe_client = context.stripe_client().await?;
    let subscriptions = lib::stripe::list_active_subscriptions(&stripe_client).await?;
    let mut message = String::new();
    for subscription in &subscriptions {
        let (customer, product) =
            lib::tasks::subscription_roles::get_customer_and_product(&stripe_client, subscription)
                .await?;
        let discord_handle = customer.metadata.get("Discord");
        message.push_str(&format!(
            "Customer: {:?}, Discord: {:?}, Product: {:?}\n",
            &customer.email,
            discord_handle,
            product
                .name
                .as_ref()
                .map(String::as_str)
                .unwrap_or("Unknown product")
        ));
    }
    context
        .msg
        .author
        .direct_message(&context.ctx, |message_builder| {
            message_builder.content(format!("Active subscriptions:\n{}", message))
        })
        .await?;
    Ok(())
}
