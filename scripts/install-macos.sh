#!/bin/bash
set -e

# gotify-desktop macOS Installer
# Installs gotify-desktop and sets up auto-launch on login

set -e

echo "🐦 Installing gotify-desktop for macOS..."

# Check if Homebrew is installed
if ! command -v brew &> /dev/null; then
    echo "❌ Homebrew is not installed. Please install it first:"
    echo "   /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
    exit 1
fi

# Install Rust if not installed
if ! command -v cargo &> /dev/null; then
    echo "📦 Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Build the application
echo "🔨 Building gotify-desktop..."
cd "$(dirname "$0")"
cargo build --release

# Install binary
echo "📦 Installing binary to /usr/local/bin..."
sudo install -Dm 755 target/release/gotify-desktop /usr/local/bin/gotify-desktop

# Create LaunchAgent directory if it doesn't exist
mkdir -p "$HOME/Library/LaunchAgents"

# Create LaunchAgent plist
echo "📝 Creating LaunchAgent..."
cat > "$HOME/Library/LaunchAgents/com.gotify.desktop.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.gotify.desktop</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/local/bin/gotify-desktop</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>/tmp/gotify-desktop.log</string>
  <key>StandardErrorPath</key>
  <string>/tmp/gotify-desktop.err</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin</string>
  </dict>
</dict>
</plist>
EOF

# Unload existing service if running
echo "🔄 Setting up launch daemon..."
launchctl unload "$HOME/Library/LaunchAgents/com.gotify.desktop.plist" 2>/dev/null || true

# Load the service
launchctl load "$HOME/Library/LaunchAgents/com.gotify.desktop.plist"

# Verify installation
if launchctl list | grep -q com.gotify.desktop; then
    echo "✅ gotify-desktop is now installed and running!"
    echo ""
    echo "📝 Configuration:"
    echo "   Edit ~/.config/gotify-desktop/config.toml to set your server URL and token"
    echo ""
    echo "📋 Service status:"
    launchctl list | grep gotify-desktop
    echo ""
    echo "📜 Logs:"
    echo "   stdout: /tmp/gotify-desktop.log"
    echo "   stderr: /tmp/gotify-desktop.err"
    echo ""
    echo "🔧 Useful commands:"
    echo "   launchctl unload ~/Library/LaunchAgents/com.gotify.desktop.plist  # Stop"
    echo "   launchctl load ~/Library/LaunchAgents/com.gotify.desktop.plist    # Start"
    echo "   launchctl list | grep gotify                                      # Check status"
else
    echo "❌ Installation failed. Check /tmp/gotify-desktop.err for details."
    exit 1
fi