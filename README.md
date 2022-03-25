# Gotify desktop

[![Build status](https://github.com/desbma/gotify-desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/desbma/gotify-desktop/actions)
[![License](https://img.shields.io/github/license/desbma/gotify-desktop.svg?style=flat)](https://github.com/desbma/gotify-desktop/blob/master/LICENSE)

Small [Gotify](https://gotify.net/) daemon to receive messages and forward them as desktop notifications.

## Features

- Read Gotify messages, and forward them as [standard desktop notification](https://www.galago-project.org/specs/notification/0.9/index.html) (should work on must Unix variants)
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
strip --strip-all target/release/gotify-desktop
install -Dm 755 -t /usr/local/bin target/release/gotify-desktop
```

### From AUR

Arch Linux users can install the [gotify-desktop AUR package](https://aur.archlinux.org/packages/gotify-desktop/).

## Configuration

Edit `~/.config/gotify-desktop/config.toml` with your server URL and client token, and other settings:

```
[gotify]
url = "wss://SERVER_DOMAIN:SERVER_PORT"
token = "YOUR_SECRET_TOKEN"
auto_delete = true  # optional, if true, deletes messages that have been handled, defaults to false

[notification]
min_priority = 1  # optional, ignores messages with priority lower than given value

[action]
# optional, run the given command for each message, with the following environment variables set: GOTIFY_MSG_PRIORITY, GOTIFY_MSG_TITLE and GOTIFY_MSG_TEXT.
on_msg_command = "/usr/bin/beep"
```

## Usage

Start `gotify-desktop` in the background using your favorite init system, desktop environment or windows manager.

## License

[GPLv3](https://www.gnu.org/licenses/gpl-3.0-standalone.html)
