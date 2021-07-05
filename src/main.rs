mod config;
mod gotify;
mod notif;

fn handle_message(message: gotify::Message, min_priority: i64) -> anyhow::Result<()> {
    log::info!("Got {:?}", message);

    if message.priority >= min_priority {
        notif::show(message)?;
    } else {
        log::debug!("Ignoring message of priority {}", message.priority);
    }

    Ok(())
}

fn main() {
    // Init logger
    simple_logger::SimpleLogger::new().init().unwrap();

    // Parse config
    let cfg = config::parse_config().expect("Failed to read config");

    // Init client
    let mut client = gotify::Client::new(&cfg.gotify).expect("Failed to setup client");

    // Connect loop
    loop {
        // Connect
        client.connect().expect("Failed to connect");
        log::info!("Connected to {}", cfg.gotify.url);

        // Handle missed messages
        let missed_messages = client
            .get_missed_messages()
            .expect("Failed to get missed messages");
        if !missed_messages.is_empty() {
            log::info!("Catching up {} missed message(s)", missed_messages.len());
            for msg in missed_messages {
                handle_message(msg, cfg.notification.min_priority)
                    .expect("Failed to handle message");
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
                    res.expect("Failed to get message");
                    unreachable!();
                }
            };

            handle_message(msg, cfg.notification.min_priority).expect("Failed to handle message");
        }
    }
}
