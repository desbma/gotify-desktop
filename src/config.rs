//! Local configuration

/// Local configuration
#[derive(Debug, serde::Deserialize)]
pub(crate) struct Config {
    /// Gotify specific local configuration
    pub gotify: GotifyConfig,

    /// Notification specific local configuration
    #[serde(default)]
    pub notification: NotificationConfig,

    /// Action specific local configuration
    #[serde(default)]
    pub action: ActionConfig,
}

/// Gotify specific local configuration
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct GotifyConfig {
    /// Gotify base URL
    pub url: url::Url,
    /// Gotify token
    pub token: String,
    /// Should messages be deleted on reception?
    #[serde(default)]
    pub auto_delete: bool,
}

/// Notification specific local configuration
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct NotificationConfig {
    /// Minimum priority below which to disable message notification
    pub min_priority: i64,
}

/// Action specific local configuration
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct ActionConfig {
    /// Command to run on message reception
    pub on_msg_command: Option<String>,
}

/// Parse local configuration
pub(crate) fn parse() -> anyhow::Result<Config> {
    let binary_name = env!("CARGO_PKG_NAME");
    let xdg_dirs = xdg::BaseDirectories::with_prefix(binary_name)?;
    let config_filepath = xdg_dirs
        .find_config_file("config.toml")
        .ok_or_else(|| anyhow::anyhow!("Unable to find config file"))?;
    log::debug!("Config filepath: {:?}", config_filepath);

    let toml_data = std::fs::read_to_string(config_filepath)?;
    log::trace!("Config data: {:?}", toml_data);

    let config = toml::from_str(&toml_data)?;
    log::trace!("Config: {:?}", config);
    Ok(config)
}
