mod config;
mod gotify;
mod notif;

fn main() {
    // Init logger
    simple_logger::SimpleLogger::new().init().unwrap();

    // Parse config
    let cfg = config::parse_config().expect("Failed to read config");

    // Init client
    let mut client = gotify::Client::new(&cfg.gotify).expect("Failed to setup client");

    // Connect
    client.connect().expect("Failed to connect");
    log::info!("Connected to {}", cfg.gotify.url);

    // Main loop
    loop {
        let msg = client.get_message().expect("Failed to get message");
        log::info!("Parsed {:?}", msg);

        if msg.priority >= cfg.notification.min_priority {
            notif::show(msg).expect("Failed to show notification");
        } else {
            log::debug!("Ignoring message of priority {}", msg.priority);
        }
    }
}
