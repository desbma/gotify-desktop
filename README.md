# Gotify desktop

[![Build status](https://github.com/desbma/gotify-desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/desbma/gotify-desktop/actions)
[![AUR version](https://img.shields.io/aur/version/gotify-desktop.svg?style=flat)](https://aur.archlinux.org/packages/gotify-desktop/)
[![License](https://img.shields.io/github/license/desbma/gotify-desktop.svg?style=flat)](https://github.com/desbma/gotify-desktop/blob/master/LICENSE)

Small [Gotify](https://gotify.net/) daemon to receive messages and forward them as desktop notifications.

## Features

- Read Gotify messages, and forward them as [standard desktop notification](https://specifications.freedesktop.org/notification-spec/notification-spec-latest.html) (works on Linux/MacOS, and likely other Unix flavors)
- Forward message priority
- Auto reconnect if server connection is lost (unreliable network, laptop suspend...), and get missed messages
- Automatically download, cache, and show app icons
- Fast and self contained binary (no runtime dependencies)
- Optional features:
  - ignore messages below a given priority level
  - delete messages once received
  - run command on each received message

## Installation

### From source

You need a Rust build environment for example from [rustup](https://rustup.rs/).

```
cargo build --release
sudo install -Dm 755 -t /usr/local/bin target/release/gotify-desktop
```

If you want to add a [Desktop Entry](https://specifications.freedesktop.org/desktop-entry-spec/desktop-entry-spec-latest.html):

```
sudo install -Dm 644 desktop/gotify-desktop.desktop /usr/share/applications/gotify-desktop.desktop
curl https://raw.githubusercontent.com/gotify/logo/master/gotify-logo-small.svg -o /tmp/gotify-logo-small.svg
sudo install -Dm 644 /tmp/gotify-logo-small.svg /usr/share/icons/hicolor/scalable/apps/gotify-desktop.svg
# update icon cache (may differ depending on Linux distribution)
sudo gtk-update-icon-cache /usr/share/icons/hicolor/
```

And to install the systemd service:

```
sudo install -Dm 644 -t /usr/lib/systemd/user/ gotify-desktop.service
```

### From AUR

Arch Linux users can install the [gotify-desktop AUR package](https://aur.archlinux.org/packages/gotify-desktop/).

## Configuration

Edit `~/.config/gotify-desktop/config.toml` with your server URL and client token, and other settings:

```
[gotify]
# gotify server websocket URL, use wss:// prefix for TLS, or ws:// for unencrypted
url = "wss://SERVER_DOMAIN:SERVER_PORT"

# secret gotify token
token = "YOUR_SECRET_TOKEN"
# if you want to get the token from a password manager, or other external command,
# you can also use for example:
# token = { command = "secret-tool lookup Title 'Gotify token'" }

# optional, if true, deletes messages that have been handled, defaults to false
auto_delete = true

[notification]
# optional, ignores messages with priority lower than given value, defaults to 0
min_priority = 1

[action]
# optional, run the given command for each message, with the following environment variables set: GOTIFY_MSG_PRIORITY, GOTIFY_MSG_TITLE and GOTIFY_MSG_TEXT.
on_msg_command = "/usr/bin/beep"
```

## Usage

Start `gotify-desktop` in the background using your favorite init system, desktop environment or windows manager.

## License

[GPLv3](https://www.gnu.org/licenses/gpl-3.0-standalone.html)
