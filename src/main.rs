mod config;
mod gotify;
mod notif;

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    // Parse config
    let cfg = config::parse_config().expect("Failed to read config");

    // Connect
    // TODO retry connection with increasing delay
    let mut client = gotify::Client::new(&cfg.gotify).expect("Failed to setup client");
    client.connect().expect("Failed to connect");
    log::info!("Connected to {}", cfg.gotify.url);

    // Main loop
    loop {
        let msg = client.get_message().expect("Failed to get message");
        log::info!("Parsed messaged {:?}", msg);

        notif::show(msg).expect("Failed to show notification");
    }
}
