use crate::config;

pub struct Client {
    ws: WebSocket,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub appid: i64,
    pub message: String,
    pub title: String,
    pub priority: i64,
    pub date: String,
}

type WebSocket = tungstenite::WebSocket<
    tungstenite::stream::Stream<std::net::TcpStream, native_tls::TlsStream<std::net::TcpStream>>,
>;

lazy_static::lazy_static! {
    static ref USER_AGENT: String =
        format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

impl Client {
    pub fn new(config: &config::GotifyConfig) -> anyhow::Result<Client> {
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
            Ok(Client { ws })
        }
    }

    pub fn get_message(&mut self) -> anyhow::Result<Message> {
        loop {
            let ws_msg = self.ws.read_message()?;
            log::trace!("Got message: {:?}", ws_msg);

            let msg_str = match ws_msg {
                tungstenite::protocol::Message::Text(msg_str) => msg_str,
                tungstenite::protocol::Message::Ping(_) => continue,
                _ => anyhow::bail!("Unexpected message type: {:?}", ws_msg),
            };

            let msg: Message = serde_json::from_str(&msg_str)?;
            return Ok(msg);
        }
    }
}
