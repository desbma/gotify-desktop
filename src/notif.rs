use crate::gotify;

// Show notification
pub fn show(msg: gotify::Message) -> anyhow::Result<()> {
    let urgency = match msg.priority {
        1..=3 => notify_rust::Urgency::Low,
        4..=7 => notify_rust::Urgency::Normal,
        8..=10 => notify_rust::Urgency::Critical,
        _ => anyhow::bail!("Unexpected urgency value"),
    };

    let mut notif = notify_rust::Notification::new();
    notif
        .summary(&msg.title)
        .body(&msg.message)
        .urgency(urgency);
    if let Some(img_filepath) = msg.app_img_filepath {
        notif.icon(&img_filepath);
    }

    notif.show()?;

    Ok(())
}
