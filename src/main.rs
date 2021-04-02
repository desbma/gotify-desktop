mod config;
mod gotify;

fn main() {
    simple_logger::SimpleLogger::new().init().unwrap();

    let cfg = config::parse_config().expect("Failed to read config");

    let client = gotify::GotifyClient::new(&cfg.gotify).expect("Failed to connect");
    log::info!("Connected to {}", cfg.gotify.url);
}
