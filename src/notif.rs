use crate::gotify;

pub fn show(msg: gotify::Message) -> anyhow::Result<()> {
    notify_rust::Notification::new()
        .summary(&msg.title)
        .body(&msg.message)
        .show()?;

    Ok(())
}
