//! Desktop notification

use crate::gotify;

/// Name of the XDG Desktop entry, without the .desktop suffix
const DESKTOP_ENTRY_NAME: &str = env!("CARGO_PKG_NAME");

/// Show notification
pub(crate) fn show(msg: &gotify::Message) -> anyhow::Result<()> {
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

    Ok(())
}
