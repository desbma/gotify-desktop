use crate::gotify;

pub fn show(msg: gotify::Message) -> anyhow::Result<()> {
    let urgency = match msg.priority {
        1..=3 => notify_rust::Urgency::Low,
        4..=7 => notify_rust::Urgency::Normal,
        8..=10 => notify_rust::Urgency::Critical,
        _ => anyhow::bail!("Unexpected urgency value"),
    };

    notify_rust::Notification::new()
        .summary(&msg.title)
        .body(&msg.message)
        .urgency(urgency)
        .show()?;

    Ok(())
}
