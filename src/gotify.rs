use std::collections::hash_map::Entry;
use std::collections::HashMap;

use crate::config;

pub struct Client {
    base_url: url::Url,
    ws: WebSocket,
    http: reqwest::blocking::Client,
    app_imgs: HashMap<i64, Option<String>>,
    xdg_dirs: xdg::BaseDirectories,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub appid: i64,
    pub message: String,
    pub title: String,
    pub priority: i64,
    pub date: String,

    #[serde(skip)]
    pub app_img_filepath: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct AppInfo {
    description: String,
    id: i64,
    image: String,
    internal: bool,
    name: String,
    token: String,
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
        // Init app img cache
        let app_imgs: HashMap<i64, Option<String>> = HashMap::new();
        let binary_name = env!("CARGO_PKG_NAME");
        let xdg_dirs = xdg::BaseDirectories::with_prefix(binary_name)?;

        // Http client to get images
        let mut gotify_header = reqwest::header::HeaderValue::from_str(&config.token)?;
        gotify_header.set_sensitive(true);
        let mut http_headers = reqwest::header::HeaderMap::new();
        http_headers.insert("X-Gotify-Key", gotify_header);
        let http = reqwest::blocking::Client::builder()
            .user_agent(&*USER_AGENT)
            .default_headers(http_headers)
            .build()?;

        // WS connect & handshake
        let request = tungstenite::handshake::client::Request::builder()
            .uri(&config.url)
            .header("User-Agent", &*USER_AGENT)
            .header("X-Gotify-Key", &config.token)
            .body(())?;
        let (mut ws, response) = tungstenite::connect(request)?;

        // Check response
        let status = response.status();
        if !status.is_informational() && !status.is_success() {
            ws.close(None)?;
            Err(anyhow::anyhow!(
                "Server returned response code {} {}",
                status.as_str(),
                status.canonical_reason().unwrap_or("?")
            ))
        } else {
            let mut base_url = url::Url::parse(&config.url)?;
            let scheme = match base_url.scheme() {
                "wss" => "https",
                "ws" => "http",
                _ => anyhow::bail!("Unexpected scheme {:?}", base_url.scheme()),
            };
            base_url.set_scheme(scheme).unwrap();
            Ok(Client {
                base_url,
                ws,
                http,
                app_imgs,
                xdg_dirs,
            })
        }
    }

    pub fn get_message(&mut self) -> anyhow::Result<Message> {
        loop {
            // Read message
            let ws_msg = self.ws.read_message()?;
            log::trace!("Got message: {:?}", ws_msg);

            // Check message type
            let msg_str = match ws_msg {
                tungstenite::protocol::Message::Text(msg_str) => msg_str,
                tungstenite::protocol::Message::Ping(_) => continue,
                _ => anyhow::bail!("Unexpected message type: {:?}", ws_msg),
            };

            // Parse
            let mut msg: Message = serde_json::from_str(&msg_str)?;

            // Download image if needed
            msg.app_img_filepath = match self.app_imgs.entry(msg.appid) {
                Entry::Occupied(e) => e.get().clone(),
                Entry::Vacant(e) => {
                    let img_filepath = self
                        .xdg_dirs
                        .place_cache_file(format!("app-{}.png", msg.appid))?;
                    let new_entry = if let Ok(_metadata) = std::fs::metadata(&img_filepath) {
                        // && metadata.is_file()
                        Some(img_filepath.into_os_string().into_string().unwrap())
                    } else {
                        Client::download_app_img(
                            &self.base_url,
                            &self.http,
                            msg.appid,
                            &img_filepath,
                        )?
                    };
                    e.insert(new_entry.clone());
                    new_entry
                }
            };

            return Ok(msg);
        }
    }

    fn download_app_img(
        base_url: &url::Url,
        client: &reqwest::blocking::Client,
        app_id: i64,
        img_filepath: &std::path::Path,
    ) -> anyhow::Result<Option<String>> {
        // Get app info
        let url = base_url.clone().join("/application")?;
        log::debug!("{}", url);
        let response = client.get(url).send()?.error_for_status()?;
        let json_data = response.text()?;

        // Parse it
        let apps: Vec<AppInfo> = serde_json::from_str(&json_data)?;
        let matching_app = apps.iter().find(|a| a.id == app_id).unwrap();

        // Download if we can
        if !matching_app.image.is_empty() {
            let img_url = base_url.clone().join(&matching_app.image)?;
            log::debug!("{}", img_url);
            let mut img_response = client.get(img_url).send()?.error_for_status()?;
            let mut img_file = std::fs::File::create(&img_filepath)?;
            std::io::copy(&mut img_response, &mut img_file)?;
            log::debug!("{:?} written", img_filepath);
            Ok(Some(
                img_filepath
                    .to_owned()
                    .into_os_string()
                    .into_string()
                    .unwrap(),
            ))
        } else {
            Ok(None)
        }
    }
}
