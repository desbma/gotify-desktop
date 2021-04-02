use crate::config;

type WebSocket = tungstenite::WebSocket<
    tungstenite::stream::Stream<std::net::TcpStream, native_tls::TlsStream<std::net::TcpStream>>,
>;

lazy_static::lazy_static! {
    static ref USER_AGENT: String =
        format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

pub struct GotifyClient {
    ws: WebSocket,
}

impl GotifyClient {
    pub fn new(config: &config::GotifyConfig) -> anyhow::Result<GotifyClient> {
        let request = tungstenite::handshake::client::Request::builder()
            .uri(&config.url)
            .header("User-Agent", &*USER_AGENT)
            .header("X-Gotify-Key", &config.token)
            .body(())?;

        let (mut ws, response) = tungstenite::connect(request)?;

        let status = response.status();
        if !status.is_informational() && !status.is_success() {
            ws.close(None)?;
            Err(anyhow::anyhow!(
                "Server returned response code {} {}",
                status.as_str(),
                status.canonical_reason().unwrap_or("?")
            ))
        } else {
            Ok(GotifyClient { ws })
        }
    }
}
