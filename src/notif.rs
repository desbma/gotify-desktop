//! Desktop notification

use crate::gotify;
use std::process::Command;

/// Name of the XDG Desktop entry, without the .desktop suffix
const DESKTOP_ENTRY_NAME: &str = env!("CARGO_PKG_NAME");

/// Show notification
pub(crate) fn show(msg: &gotify::Message) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        // Use osascript for macOS notifications
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            msg.text.replace('"', "\\\"").replace('\n', "\\n"),
            msg.title.replace('"', "\\\"").replace('\n', "\\n")
        );

        let output = Command::new("osascript").arg("-e").arg(script).output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "AppleScript notification failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        // Use notify_rust for other platforms
        #[cfg(all(unix, not(target_os = "macos")))]
        let urgency = match msg.priority {
            0..=3 => notify_rust::Urgency::Low,
            4..=7 => notify_rust::Urgency::Normal,
            8..=10 => notify_rust::Urgency::Critical,
            v => {
                log::warn!("Unexpected urgency value {v}");
                notify_rust::Urgency::Normal
            }
        };

        let mut notif = notify_rust::Notification::new();
        notif.summary(&msg.title).body(&msg.text);
        #[cfg(all(unix, not(target_os = "macos")))]
        notif
            .urgency(urgency)
            .appname("Gotify Desktop")
            .hint(notify_rust::Hint::DesktopEntry(
                DESKTOP_ENTRY_NAME.to_owned(),
            ));
        if let Some(img_filepath) = &msg.app_img_filepath.as_ref() {
            notif.icon(
                img_filepath
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Unable to convert path to string"))?,
            );
        } else {
            notif.icon(DESKTOP_ENTRY_NAME);
        }

        notif.show()?;
    }

    Ok(())
}
