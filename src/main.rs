use anyhow::Context;

mod config;
mod gotify;
mod notif;

fn handle_message(
    message: gotify::Message,
    min_priority: i64,
    delete: bool,
    client: &mut gotify::Client,
) -> anyhow::Result<()> {
    log::info!("Got {:?}", message);

    if message.priority >= min_priority {
        notif::show(&message)?;
    } else {
        log::debug!("Ignoring message of priority {}", message.priority);
    }

    if delete {
        client.delete_message(message.id)?;
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    // Init logger
    simple_logger::SimpleLogger::new()
        .init()
        .context("Failed to init logger")?;

    // Parse config
    let cfg = config::parse_config().context("Failed to read config")?;

    // Init client
    let mut client = gotify::Client::new(&cfg.gotify).context("Failed to setup client")?;

    // Connect loop
    loop {
        // Connect
        client.connect().context("Failed to connect")?;
        log::info!("Connected to {}", cfg.gotify.url);

        // Handle missed messages
        let missed_messages = client
            .get_missed_messages()
            .context("Failed to get missed messages")?;
        if !missed_messages.is_empty() {
            log::info!("Catching up {} missed message(s)", missed_messages.len());
            for msg in missed_messages {
                handle_message(
                    msg,
                    cfg.notification.min_priority,
                    cfg.gotify.auto_delete,
                    &mut client,
                )
                .context("Failed to handle message")?;
            }
        }

        // Blocking message loop
        loop {
            let res = client.get_message();
            let msg = match res {
                Ok(m) => m,
                Err(ref e) => {
                    if e.downcast_ref::<gotify::NeedsReconnect>().is_some() {
                        break;
                    }
                    res.context("Failed to get message")?;
                    unreachable!();
                }
            };

            handle_message(
                msg,
                cfg.notification.min_priority,
                cfg.gotify.auto_delete,
                &mut client,
            )
            .context("Failed to handle message")?;
        }
    }
}
