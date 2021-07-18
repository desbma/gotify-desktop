use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::time::Duration;

use crate::config;

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub struct NeedsReconnect {
    #[from]
    inner: std::io::Error,
}

pub struct Client {
    config: config::GotifyConfig,

    ws: Option<WebSocket>,
    poller: Option<mio::Poll>,

    http_client: reqwest::blocking::Client,
    http_url: url::Url,

    app_imgs: HashMap<i64, Option<PathBuf>>,
    xdg_dirs: xdg::BaseDirectories,

    last_msg_id: Option<i64>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: i64,
    pub appid: i64,
    pub message: String,
    pub title: String,
    pub priority: i64,
    pub date: String,

    #[serde(skip)]
    pub app_img_filepath: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AllMessages {
    messages: Vec<Message>,
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
        let app_imgs: HashMap<i64, Option<PathBuf>> = HashMap::new();
        let binary_name = env!("CARGO_PKG_NAME");
        let xdg_dirs = xdg::BaseDirectories::with_prefix(binary_name)?;

        // Http client (non WS)
        let mut gotify_header = reqwest::header::HeaderValue::from_str(&config.token)?;
        gotify_header.set_sensitive(true);
        let mut http_headers = reqwest::header::HeaderMap::new();
        http_headers.insert("X-Gotify-Key", gotify_header);
        let http_client = reqwest::blocking::Client::builder()
            .user_agent(&*USER_AGENT)
            .default_headers(http_headers)
            .build()?;
        let mut url = config.url.to_owned();
        let scheme = match url.scheme() {
            "wss" => "https",
            "ws" => "http",
            s => anyhow::bail!("Unexpected scheme {:?}", s),
        };
        url.set_scheme(scheme).unwrap();

        Ok(Client {
            config: config.to_owned(),
            ws: None,
            poller: None,
            http_client,
            http_url: url,
            app_imgs,
            xdg_dirs,
            last_msg_id: None,
        })
    }

    pub fn connect(&mut self) -> anyhow::Result<()> {
        let log_failed_attempt = |err, duration| {
            log::warn!("Connection failed: {}, retrying in {:?}", err, duration);
        };
        let retrier = backoff::ExponentialBackoff {
            current_interval: Duration::from_millis(250),
            initial_interval: Duration::from_millis(250),
            randomization_factor: 0.0,
            multiplier: 1.5,
            max_interval: Duration::from_secs(60),
            max_elapsed_time: None,
            ..backoff::ExponentialBackoff::default()
        };
        backoff::retry_notify(
            retrier,
            || self.try_connect().map_err(backoff::Error::Transient),
            log_failed_attempt,
        )
        .map_err(|e| match e {
            backoff::Error::Permanent(e) => e,
            backoff::Error::Transient(e) => e,
        })
    }

    fn try_connect(&mut self) -> anyhow::Result<()> {
        // WS connect & handshake
        let mut url = self.config.url.to_owned();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid URL {}", self.config.url))?
            .push("stream");
        let request = tungstenite::handshake::client::Request::builder()
            .uri(url.to_string())
            .header("User-Agent", &*USER_AGENT)
            .header("X-Gotify-Key", &self.config.token)
            .body(())?;
        let (mut ws, response) = tungstenite::connect(request)?;

        // Check response
        let status = response.status();
        if !status.is_informational() && !status.is_success() {
            ws.close(None)?;
            anyhow::bail!(
                "Server returned response code {} {}",
                status.as_str(),
                status.canonical_reason().unwrap_or("?")
            );
        }

        // Setup poller
        let poller = mio::Poll::new()?;
        let poller_registry = poller.registry();
        let fd = match &ws.get_ref() {
            tungstenite::stream::Stream::Plain(s) => s.as_raw_fd(),
            tungstenite::stream::Stream::Tls(t) => t.get_ref().as_raw_fd(),
        };
        poller_registry.register(
            &mut mio::unix::SourceFd(&fd),
            mio::Token(0),
            mio::Interest::READABLE,
        )?;

        self.ws = Some(ws);
        self.poller = Some(poller);
        Ok(())
    }

    pub fn get_missed_messages(&mut self) -> anyhow::Result<Vec<Message>> {
        let mut missed_messages: Vec<Message> = if let Some(last_msg_id) = self.last_msg_id {
            // Get all recent messages
            let mut url = self.http_url.to_owned();
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Invalid URL {}", self.http_url))?
                .push("message");
            url.query_pairs_mut().append_pair("limit", "200");
            log::debug!("{}", url);
            let response = self.http_client.get(url).send()?.error_for_status()?;
            let json_data = response.text()?;
            log::trace!("{}", json_data);

            // Parse response & keep the ones we have not yet seen
            let all_messages: AllMessages = serde_json::from_str(&json_data)?;
            all_messages
                .messages
                .into_iter()
                .filter(|m| m.id > last_msg_id)
                .rev()
                .collect()
        } else {
            vec![]
        };

        for missed_message in missed_messages.iter_mut() {
            self.set_app_img(missed_message)?;
        }

        if let Some(last_msg) = missed_messages.iter().last() {
            self.last_msg_id = Some(last_msg.id);
        }

        Ok(missed_messages)
    }

    pub fn get_message(&mut self) -> anyhow::Result<Message> {
        let ws = self.ws.as_mut().unwrap();
        let poller = self.poller.as_mut().unwrap();

        loop {
            // Poll to detect stale socket, so we can trigger reconnect,
            // this can occur when returning from sleep/hibernation
            // Without this, read_message blocks forever even if server already closed its end
            let mut poller_events = mio::Events::with_capacity(1);
            let poll_res = poller.poll(&mut poller_events, None);
            match poll_res {
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    return Err(NeedsReconnect { inner: e }.into());
                }
                Err(_) => poll_res.unwrap(),
                _ => {}
            }
            if poller_events.is_empty() {
                continue;
            }

            // Read message
            let ws_msg = ws.read_message()?;
            log::trace!("Got message: {:?}", ws_msg);

            // Check message type
            let msg_str = match ws_msg {
                tungstenite::protocol::Message::Text(msg_str) => msg_str,
                tungstenite::protocol::Message::Ping(_) => {
                    ws.write_pending()?;
                    continue;
                }
                _ => anyhow::bail!("Unexpected message type: {:?}", ws_msg),
            };

            // Parse
            log::trace!("{}", msg_str);
            let mut msg: Message = serde_json::from_str(&msg_str)?;

            // Get app image
            self.set_app_img(&mut msg)?;

            self.last_msg_id = Some(msg.id);
            return Ok(msg);
        }
    }

    fn set_app_img(&mut self, msg: &mut Message) -> anyhow::Result<()> {
        msg.app_img_filepath = match self.app_imgs.entry(msg.appid) {
            // Cache hit
            Entry::Occupied(e) => match e.get() {
                None => None,
                Some(img_filepath) => {
                    if let Ok(_metadata) = std::fs::metadata(&img_filepath) {
                        // Image file already exists
                        Some(img_filepath.to_owned())
                    } else {
                        log::warn!(
                            "File {:?} has been removed, will try to download it again",
                            img_filepath
                        );

                        // Create cache path
                        let img_filepath = self
                            .xdg_dirs
                            .place_cache_file(format!("app-{}.png", msg.appid))?;

                        // Download image file if app has one
                        Self::download_app_img(
                            &self.http_url,
                            &self.http_client,
                            msg.appid,
                            &img_filepath,
                        )?
                    }
                }
            },
            // Cache miss
            Entry::Vacant(e) => {
                // Create cache path
                let img_filepath = self
                    .xdg_dirs
                    .place_cache_file(format!("app-{}.png", msg.appid))?;

                let new_entry = if let Ok(_metadata) = std::fs::metadata(&img_filepath) {
                    // && metadata.is_file()
                    // Image file already exists
                    Some(img_filepath)
                } else {
                    // Download image file if app has one
                    Self::download_app_img(
                        &self.http_url,
                        &self.http_client,
                        msg.appid,
                        &img_filepath,
                    )?
                };
                e.insert(new_entry.to_owned());
                new_entry
            }
        };

        Ok(())
    }

    fn download_app_img(
        http_url: &url::Url,
        client: &reqwest::blocking::Client,
        app_id: i64,
        img_filepath: &std::path::Path,
    ) -> anyhow::Result<Option<PathBuf>> {
        // Get app info
        let mut url = http_url.to_owned();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Invalid URL {}", http_url))?
            .push("application");
        log::debug!("{}", url);
        let response = client.get(url).send()?.error_for_status()?;
        let json_data = response.text()?;
        log::trace!("{}", json_data);

        // Parse it
        let apps: Vec<AppInfo> = serde_json::from_str(&json_data)?;
        let matching_app = apps.iter().find(|a| a.id == app_id).unwrap();

        // Download if we can
        if !matching_app.image.is_empty() {
            let img_url = http_url.to_owned().join(&matching_app.image)?;
            log::debug!("{}", img_url);
            let mut img_response = client.get(img_url).send()?.error_for_status()?;
            let mut img_file = std::fs::File::create(&img_filepath)?;
            std::io::copy(&mut img_response, &mut img_file)?;
            log::debug!("{:?} written", img_filepath);
            Ok(Some(img_filepath.to_owned()))
        } else {
            Ok(None)
        }
    }
}
