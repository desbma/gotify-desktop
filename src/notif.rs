//! Desktop notification

#[cfg(all(unix, not(target_os = "macos")))]
use std::{collections::HashMap, sync::Mutex};

use crate::gotify;

/// Name of the XDG Desktop entry, without the .desktop suffix
const DESKTOP_ENTRY_NAME: &str = env!("CARGO_PKG_NAME");

/// Freedesktop notification D-Bus name and interface
#[cfg(all(unix, not(target_os = "macos")))]
const NOTIFICATIONS_DBUS_NAME: &str = "org.freedesktop.Notifications";

/// Freedesktop notification D-Bus object path
#[cfg(all(unix, not(target_os = "macos")))]
const NOTIFICATIONS_DBUS_PATH: &str = "/org/freedesktop/Notifications";

/// Long lived D-Bus session connection, because some servers (GNOME Shell) drop notifications
/// associated with a desktop entry when the sender disconnects
#[cfg(all(unix, not(target_os = "macos")))]
static DBUS_CONNECTION: Mutex<Option<zbus::blocking::Connection>> = Mutex::new(None);

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

    #[cfg(all(unix, not(target_os = "macos")))]
    send(&notif)?;
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    notif.show()?;

    Ok(())
}

/// Send notification over a D-Bus connection kept open for the whole process lifetime
#[cfg(all(unix, not(target_os = "macos")))]
fn send(notif: &notify_rust::Notification) -> anyhow::Result<()> {
    let mut connection_guard = DBUS_CONNECTION
        .lock()
        .map_err(|_| anyhow::anyhow!("D-Bus connection lock poisoned"))?;
    let connection = if let Some(connection) = connection_guard.as_ref() {
        connection
    } else {
        connection_guard.insert(zbus::blocking::Connection::session()?)
    };

    let hints: HashMap<&str, zbus::zvariant::Value<'_>> =
        notif.hints.iter().map(Into::into).collect();
    let id: u32 = connection
        .call_method(
            Some(NOTIFICATIONS_DBUS_NAME),
            NOTIFICATIONS_DBUS_PATH,
            Some(NOTIFICATIONS_DBUS_NAME),
            "Notify",
            &(
                &notif.appname,
                0_u32,
                &notif.icon,
                &notif.summary,
                &notif.body,
                &notif.actions,
                hints,
                i32::from(notif.timeout),
            ),
        )?
        .body()
        .deserialize()?;
    log::debug!("Notification sent with id {id}");

    Ok(())
}
