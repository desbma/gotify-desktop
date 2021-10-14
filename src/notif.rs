use crate::gotify;

// Show notification
pub fn show(msg: &gotify::Message) -> anyhow::Result<()> {
    let urgency = match msg.priority {
        0..=3 => notify_rust::Urgency::Low,
        4..=7 => notify_rust::Urgency::Normal,
        8..=10 => notify_rust::Urgency::Critical,
        v => anyhow::bail!("Unexpected urgency value {}", v),
    };

    let mut notif = notify_rust::Notification::new();
    notif.summary(&msg.title).body(&msg.message);
    #[cfg(all(unix, not(target_os = "macos")))]
    notif.urgency(urgency);
    if let Some(img_filepath) = &msg.app_img_filepath.as_ref() {
        notif.icon(
            img_filepath
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Unable to convert path to string"))?,
        );
    }

    notif.show()?;

    Ok(())
}
