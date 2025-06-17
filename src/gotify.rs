//! Gotify network & parsing code

use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::{ErrorKind, Write as _},
    os::unix::io::AsRawFd as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, LazyLock},
    time::Duration,
};

use backon::BlockingRetryable as _;
use tungstenite::{client::IntoClientRequest as _, error::ProtocolError, http::HeaderValue};

use crate::config;

/// Error when socket needs reconnect
#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub(crate) enum NeedsReconnect {
    /// Inner poll error
    Io(#[from] std::io::Error),
    /// Protocol disconnect
    Protocol(#[from] ProtocolError),
}

/// Gotify client state
pub(crate) struct Client {
    /// Websocket client, if connected
    ws: WebSocket,
    /// Socket poller
    poller: mio::Poll,
    /// Gotify API token
    token: String,
    /// HTTP client (non websocket)
    #[expect(clippy::struct_field_names)]
    http_client: ureq::Agent,
    /// Gotify HTTP(S) URL
    http_url: url::Url,
    /// App image cache
    app_imgs: HashMap<i64, Option<PathBuf>>,
    /// XDG dirs
    xdg_dirs: xdg::BaseDirectories,
    /// Last received Gotify message id
    last_msg_id: Rc<RefCell<Option<i64>>>,
}

/// Gotify message
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Message {
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
pub(crate) struct AllMessages {
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

/// HTTP User-Agent string
static USER_AGENT: LazyLock<String> =
    LazyLock::new(|| format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")));

impl Client {
    /// Get a connected Gotify client
    pub(crate) fn connect(
        cfg: &config::GotifyConfig,
        token: &str,
        last_msg_id: Rc<RefCell<Option<i64>>>,
    ) -> anyhow::Result<Self> {
        // Init app img cache
        let app_imgs: HashMap<i64, Option<PathBuf>> = HashMap::new();
        let binary_name = env!("CARGO_PKG_NAME");
        let xdg_dirs = xdg::BaseDirectories::with_prefix(binary_name)?;

        // HTTP client (non WS)
        let http_client = ureq::AgentBuilder::new()
            .tls_connector(Arc::new(ureq::native_tls::TlsConnector::new()?))
            .user_agent(&USER_AGENT)
            .build();
        let mut http_url = cfg.url.clone();
        let scheme = match http_url.scheme() {
            "wss" => "https",
            "ws" => "http",
            s => anyhow::bail!("Unexpected scheme {:?}", s),
        };
        #[expect(clippy::unwrap_used)] // We know the scheme is valid here
        http_url.set_scheme(scheme).unwrap();

        // Connect gotify client, with retries
        let (ws, poller) = (|| Self::try_connect(&cfg.url, token))
            .retry(
                backon::ExponentialBuilder::default()
                    .with_factor(1.5)
                    .with_min_delay(Duration::from_millis(250))
                    .with_max_delay(Duration::from_secs(60))
                    .without_max_times(),
            )
            .notify(|err, dur| {
                log::warn!("Connection failed: {err}, retrying in {dur:?}");
            })
            .call()?;

        Ok(Self {
            ws,
            poller,
            token: token.to_owned(),
            http_client,
            http_url,
            app_imgs,
            xdg_dirs,
            last_msg_id,
        })
    }

    /// Build request with auth header, send it, check status code, and return response
    fn send_request(&self, method: &'static str, url: &url::Url) -> anyhow::Result<Vec<u8>> {
        log::debug!("{method} {url}");
        let request = self.http_client.request_url(method, url);
        let response = request.set("X-Gotify-Key", &self.token).call()?;
        anyhow::ensure!(
            response.status() >= 200 && response.status() < 300,
            "HTTP response {}: {}",
            response.status(),
            response.status_text()
        );
        let mut buf = if let Some(content_len) = response
            .header("Content-Length")
            .and_then(|h| h.parse::<usize>().ok())
        {
            Vec::with_capacity(content_len)
        } else {
            Vec::new()
        };
        response.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Add auth header to request, send it, check status code, and parse JSON response
    fn send_api_request<T: serde::de::DeserializeOwned>(
        &self,
        method: &'static str,
        url: &url::Url,
    ) -> anyhow::Result<T> {
        let json_data = String::from_utf8(self.send_request(method, url)?)?;
        log::trace!("{json_data}");
        Ok(serde_json::from_str(&json_data)?)
    }

    /// Connect gotify client
    fn try_connect(url: &url::Url, token: &str) -> anyhow::Result<(WebSocket, mio::Poll)> {
        // WS connect & handshake
        let mut url = url.to_owned();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL"))?
            .push("stream");
        let mut request = url.into_client_request()?;
        let headers = request.headers_mut();
        headers.insert("User-Agent", HeaderValue::from_str(&USER_AGENT)?);
        headers.insert("X-Gotify-Key", HeaderValue::from_str(token)?);
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

        Ok((ws, poller))
    }

    /// Catch up missed messages since the last received one
    pub(crate) fn get_missed_messages(&mut self) -> anyhow::Result<Vec<Message>> {
        let mut missed_messages: Vec<Message> =
            if let Some(last_msg_id) = *self.last_msg_id.borrow() {
                // Get all recent messages
                let mut url = self.http_url.clone();
                url.path_segments_mut()
                    .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.http_url))?
                    .push("message");
                url.query_pairs_mut().append_pair("limit", "200");
                let all_messages: AllMessages = self.send_api_request("GET", &url)?;

                // Keep the ones we have not yet seen
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
            self.last_msg_id.replace(Some(last_msg.id));
        }

        Ok(missed_messages)
    }

    /// Get pending gotify messages
    pub(crate) fn get_message(&mut self) -> anyhow::Result<Message> {
        loop {
            // Poll to detect stale socket, so we can trigger reconnect,
            // this can occur when returning from sleep/hibernation
            // Without this, read_message blocks forever even if server already closed its end
            let mut poller_events = mio::Events::with_capacity(1);
            let poll_res = self.poller.poll(&mut poller_events, None);
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
            log::trace!("Event: {poller_events:?}");

            // Read message
            let read_res = self.ws.read();
            let ws_msg = match read_res {
                Ok(m) => m,
                Err(tungstenite::Error::Protocol(e))
                    if matches!(e, ProtocolError::ResetWithoutClosingHandshake) =>
                {
                    return Err(NeedsReconnect::Protocol(e).into());
                }
                Err(_) => read_res?,
            };
            log::trace!("Got message: {ws_msg:?}");

            // Check message type
            let msg_str = match ws_msg {
                tungstenite::protocol::Message::Text(msg_str) => msg_str,
                tungstenite::protocol::Message::Ping(_) => {
                    self.ws.flush()?;
                    continue;
                }
                _ => anyhow::bail!("Unexpected message type: {:?}", ws_msg),
            };

            // Parse
            log::trace!("{msg_str}");
            let mut msg: Message = serde_json::from_str(&msg_str)?;

            // Get app image
            self.set_message_app_img(&mut msg)?;

            self.last_msg_id.replace(Some(msg.id));
            return Ok(msg);
        }
    }

    /// Delete gotify message
    pub(crate) fn delete_message(&mut self, msg_id: i64) -> anyhow::Result<()> {
        let mut url = self.http_url.clone();
        url.path_segments_mut()
            .map_err(|()| anyhow::anyhow!("Invalid URL {}", self.http_url))?
            .push("message")
            .push(&format!("{msg_id}"));
        let _ = self.send_request("DELETE", &url)?;
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
                        "File {cache_hit_img_filepath:?} has been removed, will try to download it again"
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
        let apps: Vec<AppInfo> = self.send_api_request("GET", &url)?;

        // Parse it
        let matching_app = apps.into_iter().find(|a| a.id == app_id);

        Ok(matching_app.map(|a| a.image).filter(|i| !i.is_empty()))
    }

    /// Download Gotify app image if any, return true if we have downloaded one
    fn download_app_img(
        &self,
        app_id: i64,
        image_rel_url: Option<String>,
        img_filepath: &Path,
    ) -> anyhow::Result<bool> {
        if let Some(image_rel_url) = image_rel_url.map_or_else(
            || -> anyhow::Result<_> { self.app_img_url(app_id) },
            |v| Ok(Some(v)),
        )? {
            let img_url = self.http_url.clone().join(&image_rel_url)?;
            let img_data = self.send_request("GET", &img_url)?;
            let mut img_file = File::create(img_filepath)?;
            img_file.write_all(&img_data)?;
            log::debug!("{img_filepath:?} written");
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
