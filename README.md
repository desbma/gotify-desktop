# Gotify desktop

[![Build status](https://github.com/desbma/gotify-desktop/actions/workflows/ci.yml/badge.svg)](https://github.com/desbma/gotify-desktop/actions)
[![License](https://img.shields.io/github/license/desbma/gotify-desktop.svg?style=flat)](https://github.com/desbma/gotify-desktop/blob/master/LICENSE)

Small [Gotify](https://gotify.net/) daemon to receive messages and forward them as desktop notifications.

## Features

- Read Gotify messages, and forward them as [standard desktop notification](https://www.galago-project.org/specs/notification/0.9/index.html) (should work on must Unix variants)
- Forward message priority
- Automatically download, cache, and show app icons
- Fast and self contained binary (no dependencies)

## Installation

You need a Rust build environment for example from [rustup](https://rustup.rs/).

```
cargo build --release
strip --strip-all target/release/gotify-desktop
install -Dm 755 -t /usr/local/bin target/release/gotify-desktop
```

## Configuration

Edit `~/.config/gotify-desktop/config.toml` with your server URL and client token:

```
[gotify]
url = "wss://SERVER_DOMAIN:SERVER_PORT/stream"
token = "YOUR_SECRET_TOKEN"
```

## Usage

Start `gotify-desktop` in the background using your favorite init system, desktop environment or windows manager.

## License

[GPLv3](https://www.gnu.org/licenses/gpl-3.0-standalone.html)
