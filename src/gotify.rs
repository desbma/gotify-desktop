//! Gotify network & parsing code

use std::collections::HashMap;
use std::io::ErrorKind;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use reqwest::header::HeaderValue;
use tungstenite::{client::IntoClientRequest, error::ProtocolError};

use crate::config;

/// Error when socket needs reconnect
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub enum NeedsReconnect {
    /// Inner poll error
    Io(#[from] std::io::Error),
    /// Protocol disconnect
    Protocol(#[from] ProtocolError),
}

/// Gotify client state
pub struct Client {
    /// Local config
    config: config::GotifyConfig,

    /// Websocket client, if connected
    ws: Option<WebSocket>,
    /// Socket poller
    poller: Option<mio::Poll>,

    /// HTTP client (non websocket)
    #[allow(clippy::struct_field_names)]
    http_client: reqwest::blocking::Client,
    /// Gotify HTTP(S) URL
    http_url: url::Url,

    /// App image cache
    app_imgs: HashMap<i64, Option<PathBuf>>,
    /// XDG dirs
    xdg_dirs: xdg::BaseDirectories,

    /// Last received Gotify message id
    last_msg_id: Option<i64>,
}

/// Gotify message
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    /// Gotify id
    pub id: i64,
    /// App id
    pub appid: i64,
    /// Message text
    #[serde(rename = "message")]
    pub text: String,
    /// Message title
    pub title: String,
    /// Message priority
    pub priority: i64,
    /// Message date & time
    pub date: String,

    /// App image filepath
    #[serde(skip)]
    pub app_img_filepath: Option<PathBuf>,
}

/// Gotify message bunch
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AllMessages {
    /// The actual messages
    messages: Vec<Message>,
}

/// Gotify app metadata
#[derive(serde::Serialize, serde::Deserialize)]
struct AppInfo {
    /// unused
    description: String,
    /// App id
    id: i64,
    /// Image URL
    image: String,
    /// unused
    internal: bool,
    /// App name
    name: String,
    /// unused
    token: String,
}

/// HTTP or HTTPS websocket
type WebSocket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

lazy_static::lazy_static! {
    static ref USER_AGENT: String =
        format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

impl Client {
    /// Constructor
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
        let mut url = config.url.clone();
        let scheme = match url.scheme() {
            "wss" => "https",
            "ws" => "http",
            s => anyhow::bail!("Unexpected scheme {:?}", s),
        };
        #[allow(clippy::unwrap_used)] // We know the scheme is valid here
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

    /// Connect gotify client, with retries
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
            || self.try_connect().map_err(backoff::Error::transient),
            log_failed_attempt,
        )
        .map_err(|e| match e {
            backoff::Error::Permanent(e) => e,
            backoff::Error::Transient { err, .. } => err,
        })
    }

    /// Connect gotify client
    fn try_connect(&mut self) -> anyhow::Result<()> {
        // WS connect & handshake
        let mut url = self.config.url.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.config.url))?
            .push("stream");
        let mut request = url.into_client_request()?;
        let headers = request.headers_mut();
        headers.insert("User-Agent", HeaderValue::from_str(&USER_AGENT)?);
        headers.insert("X-Gotify-Key", HeaderValue::from_str(&self.config.token)?);
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
        let fd = match ws.get_ref() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => s.as_raw_fd(),
            tungstenite::stream::MaybeTlsStream::NativeTls(t) => t.get_ref().as_raw_fd(),
            _ => unimplemented!(),
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

    /// Catch up missed messages since the last received one
    pub fn get_missed_messages(&mut self) -> anyhow::Result<Vec<Message>> {
        let mut missed_messages: Vec<Message> = if let Some(last_msg_id) = self.last_msg_id {
            // Get all recent messages
            let mut url = self.http_url.clone();
            url.path_segments_mut()
                .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.http_url))?
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

        for missed_message in &mut missed_messages {
            self.set_message_app_img(missed_message)?;
        }

        if let Some(last_msg) = missed_messages.iter().last() {
            self.last_msg_id = Some(last_msg.id);
        }

        Ok(missed_messages)
    }

    /// Get pending gotify messages
    pub fn get_message(&mut self) -> anyhow::Result<Message> {
        let ws = self
            .ws
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        let poller = self
            .poller
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected"))?;

        loop {
            // Poll to detect stale socket, so we can trigger reconnect,
            // this can occur when returning from sleep/hibernation
            // Without this, read_message blocks forever even if server already closed its end
            let mut poller_events = mio::Events::with_capacity(1);
            let poll_res = poller.poll(&mut poller_events, None);
            match poll_res {
                Err(e) if e.kind() == ErrorKind::Interrupted => {
                    return Err(NeedsReconnect::Io(e).into());
                }
                Err(_) => poll_res?,
                _ => {}
            }
            if poller_events.is_empty() {
                continue;
            }
            log::trace!("Event: {:?}", poller_events);

            // Read message
            let read_res = ws.read();
            let ws_msg = match read_res {
                Ok(m) => m,
                Err(tungstenite::Error::Protocol(e))
                    if matches!(e, ProtocolError::ResetWithoutClosingHandshake) =>
                {
                    return Err(NeedsReconnect::Protocol(e).into());
                }
                Err(_) => read_res?,
            };
            log::trace!("Got message: {:?}", ws_msg);

            // Check message type
            let msg_str = match ws_msg {
                tungstenite::protocol::Message::Text(msg_str) => msg_str,
                tungstenite::protocol::Message::Ping(_) => {
                    ws.flush()?;
                    continue;
                }
                _ => anyhow::bail!("Unexpected message type: {:?}", ws_msg),
            };

            // Parse
            log::trace!("{}", msg_str);
            let mut msg: Message = serde_json::from_str(&msg_str)?;

            // Get app image
            self.set_message_app_img(&mut msg)?;

            self.last_msg_id = Some(msg.id);
            return Ok(msg);
        }
    }

    /// Delete gotify message
    pub fn delete_message(&mut self, msg_id: i64) -> anyhow::Result<()> {
        let mut url = self.http_url.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.http_url))?
            .push("message")
            .push(&format!("{msg_id}"));
        log::debug!("{}", url);

        let response = self.http_client.delete(url).send()?.error_for_status()?;
        let json_data = response.text()?;
        log::trace!("{}", json_data);

        Ok(())
    }

    /// Download (or get from cache) and set app image for a message
    fn set_message_app_img(&mut self, msg: &mut Message) -> anyhow::Result<()> {
        msg.app_img_filepath = match self.app_imgs.get(&msg.appid) {
            // Cache hit, has file
            Some(Some(cache_hit_img_filepath)) => {
                if cache_hit_img_filepath.is_file() {
                    // Image file already exists
                    Some(cache_hit_img_filepath.to_owned())
                } else {
                    log::warn!(
                        "File {:?} has been removed, will try to download it again",
                        cache_hit_img_filepath
                    );

                    // Download image file if app has one
                    self.download_app_img(msg.appid, None, cache_hit_img_filepath)?
                        .then(|| cache_hit_img_filepath.to_owned())
                }
            }

            // Cache hit, has no file
            Some(None) => None,

            // Cache miss
            None => {
                // Create cache path
                let new_entry = if let Some(image_rel_url) = self.app_img_url(msg.appid)? {
                    let cache_filename = Path::new(&image_rel_url)
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("Invalid image URL"))?;
                    let img_filepath = self.xdg_dirs.place_cache_file(cache_filename)?;

                    if img_filepath.is_file() {
                        // Image file already exists
                        Some(img_filepath)
                    } else {
                        // Download image file if app has one
                        self.download_app_img(msg.appid, Some(image_rel_url), &img_filepath)?
                            .then_some(img_filepath)
                    }
                } else {
                    None
                };
                self.app_imgs.insert(msg.appid, new_entry.clone());
                new_entry
            }
        };

        Ok(())
    }

    /// Get app image relative URL from server
    fn app_img_url(&self, app_id: i64) -> anyhow::Result<Option<String>> {
        // Get app info
        let mut url = self.http_url.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.http_url))?
            .push("application");
        log::debug!("{}", url);
        let response = self.http_client.get(url).send()?.error_for_status()?;
        let json_data = response.text()?;
        log::trace!("{}", json_data);

        // Parse it
        let apps: Vec<AppInfo> = serde_json::from_str(&json_data)?;
        let matching_app = apps.into_iter().find(|a| a.id == app_id);

        Ok(matching_app.map(|a| a.image).filter(|i| !i.is_empty()))
    }

    /// Download Gotify app image if any, return true if we have downloaded one
    fn download_app_img(
        &self,
        app_id: i64,
        image_rel_url: Option<String>,
        img_filepath: &std::path::Path,
    ) -> anyhow::Result<bool> {
        if let Some(image_rel_url) = image_rel_url.map_or_else(
            || -> anyhow::Result<_> { self.app_img_url(app_id) },
            |v| Ok(Some(v)),
        )? {
            let img_url = self.http_url.clone().join(&image_rel_url)?;
            log::debug!("{}", img_url);
            let mut img_response = self.http_client.get(img_url).send()?.error_for_status()?;
            let mut img_file = std::fs::File::create(img_filepath)?;
            std::io::copy(&mut img_response, &mut img_file)?;
            log::debug!("{:?} written", img_filepath);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
