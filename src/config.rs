#[derive(Debug, serde::Deserialize)]
pub struct Config {
    pub gotify: GotifyConfig,

    #[serde(default)]
    pub notification: NotificationConfig,

    #[serde(default)]
    pub action: ActionConfig,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct GotifyConfig {
    pub url: url::Url,
    pub token: String,
    #[serde(default)]
    pub auto_delete: bool,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct NotificationConfig {
    pub min_priority: i64,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct ActionConfig {
    pub on_msg_command: Option<String>,
}

pub fn parse_config() -> anyhow::Result<Config> {
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
