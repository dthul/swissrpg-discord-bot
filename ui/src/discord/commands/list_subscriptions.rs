use command_macro::command;

#[command]
#[regex(r"list\s*subscriptions")]
#[level(admin)]
fn list_subscriptions<'a>(
    context: super::CommandContext,
    _: regex::Captures<'a>,
) -> super::CommandResult<'a> {
    context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content("Sure! This might take a moment...")
        })
        .await
        .ok();
    let subscriptions = lib::stripe::list_active_subscriptions(context.stripe_client()?).await?;
    let mut message = String::new();
    for subscription in &subscriptions {
        // First, figure out which product was bought
        // Subscription -> Plan -> Product
        let product = match &subscription.plan {
            Some(plan) => match &plan.product {
                Some(product) => match product {
                    stripe::Expandable::Object(product) => Some(*product.clone()),
                    stripe::Expandable::Id(product_id) => {
                        let product =
                            stripe::Product::retrieve(context.stripe_client()?, product_id, &[])
                                .await?;
                        Some(product)
                    }
                },
                _ => None,
            },
            _ => None,
        };
        // Now, figure out who the customer is
        let customer = match &subscription.customer {
            stripe::Expandable::Object(customer) => *customer.clone(),
            stripe::Expandable::Id(customer_id) => {
                let customer =
                    stripe::Customer::retrieve(context.stripe_client()?, customer_id, &[]).await?;
                customer
            }
        };
        let discord_handle = customer.metadata.get("Discord");
        message.push_str(&format!(
            "Customer: {:?}, Discord: {:?}, Product: {:?}\n",
            &customer.email,
            discord_handle,
            product.map(|p| p.name)
        ));
    }
    let _ = context
        .msg
        .author
        .direct_message(context.ctx, |message_builder| {
            message_builder.content(format!("Active subscriptions:\n{}", message))
        });
    Ok(())
}
