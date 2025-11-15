//! Local configuration

use std::{
    fs,
    process::{Command, Stdio},
};

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

/// A token either as a string, or a command to run to get it
#[derive(Clone, Debug, serde::Deserialize)]
#[cfg_attr(test, derive(Eq, PartialEq))]
#[serde(rename_all = "snake_case")]
pub(crate) enum TokenSource {
    /// Command to get token
    Command(String),
    /// Plain token string
    #[serde(untagged)]
    Plain(String),
}

impl TokenSource {
    /// Get token string, by running command if needed
    pub(crate) fn fetch(&self) -> anyhow::Result<String> {
        match self {
            TokenSource::Command(cmd) => {
                log::info!("Running command {cmd:?} to fetch token");
                let cmd = shlex::split(cmd)
                    .ok_or_else(|| anyhow::anyhow!("Failed to parse command {cmd:?}"))?;
                let output = Command::new(
                    cmd.first()
                        .ok_or_else(|| anyhow::anyhow!("Empty command"))?,
                )
                .args(cmd.into_iter().skip(1))
                .stdout(Stdio::piped())
                .output()?;
                anyhow::ensure!(
                    output.status.success(),
                    "Command failed with status {:?}",
                    output.status
                );
                let token = String::from_utf8(output.stdout)?
                    .trim_ascii_end()
                    .to_owned();
                Ok(token)
            }
            TokenSource::Plain(t) => Ok(t.to_owned()),
        }
    }
}

/// Gotify specific local configuration
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct GotifyConfig {
    /// Gotify base URL
    pub url: url::Url,
    /// Gotify token
    pub token: TokenSource,
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
    let xdg_dirs = xdg::BaseDirectories::with_prefix(binary_name);
    let config_filepath = xdg_dirs
        .find_config_file("config.toml")
        .ok_or_else(|| anyhow::anyhow!("Unable to find config file"))?;
    log::debug!("Config filepath: {config_filepath:?}");

    let toml_data = fs::read_to_string(config_filepath)?;
    log::trace!("Config data: {toml_data:?}");

    let config = toml::from_str(&toml_data)?;
    log::trace!("Config: {config:?}");
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize)]
    struct TestConfig {
        token: TokenSource,
    }

    #[test]
    fn parse_plain_token() {
        assert_eq!(
            toml::from_str::<TestConfig>(r#"token = "abcdef1234""#)
                .unwrap()
                .token,
            TokenSource::Plain("abcdef1234".to_owned())
        );
    }

    #[test]
    fn parse_token_command() {
        assert_eq!(
            toml::from_str::<TestConfig>(r#"token = { command = "echo abcdef1234" }"#)
                .unwrap()
                .token,
            TokenSource::Command("echo abcdef1234".to_owned())
        );
    }

    #[test]
    fn fetch_token() {
        assert_eq!(
            TokenSource::Plain("abcdef1234".to_owned()).fetch().unwrap(),
            "abcdef1234"
        );
        assert_eq!(
            TokenSource::Command("echo abcdef1234".to_owned())
                .fetch()
                .unwrap(),
            "abcdef1234"
        );
    }
}
