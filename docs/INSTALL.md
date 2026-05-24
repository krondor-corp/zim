# Installation Guide

This guide covers installation and system requirements for Zim.

## System Requirements

### Operating Systems

- **Linux**: Any modern distribution (Ubuntu 20.04+, Debian 11+, Fedora 35+, etc.)
- **macOS**: 10.15 (Catalina) or later
- **Windows**: Windows 10/11 with WSL2 recommended (native Windows support is experimental)

### Software Requirements

- **Rust**: Version 1.75 or later (2021 edition)
- **Cargo**: Comes with Rust installation
- **Git**: For cloning the repository

### System Libraries

Zim requires the following system libraries:

#### Linux (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install build-essential pkg-config libssl-dev libsqlite3-dev
```

#### Linux (Fedora/RHEL)
```bash
sudo dnf install gcc pkg-config openssl-devel sqlite-devel
```

#### Linux (Gentoo)
```bash
emerge -av dev-lang/rust dev-libs/openssl dev-db/sqlite sys-fs/fuse:3
```

**Kernel note:** Ensure your kernel has FUSE support enabled (`CONFIG_FUSE_FS=y` or `CONFIG_FUSE_FS=m`). If built as a module, load it with `modprobe fuse`. Your user must also be in the `fuse` group:
```bash
gpasswd -a YOUR_USERNAME fuse
```

#### macOS
```bash
# Install Homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies (most come with Xcode Command Line Tools)
brew install openssl sqlite3
```

#### Windows (WSL2)
Follow the Linux (Ubuntu/Debian) instructions above within your WSL2 environment.

### Hardware Requirements

**Minimum:**
- CPU: 2 cores
- RAM: 2 GB
- Disk: 500 MB for binaries + storage for your encrypted data

**Recommended:**
- CPU: 4+ cores
- RAM: 4+ GB
- Disk: 10+ GB for comfortable operation
- Network: Stable internet connection for P2P sync

## Installation

Zim ships as a single binary: `zim-peer` (CLI + daemon). A read-only web hub (`zim-hub`) ships as a separate binary.

### CLI Installation

For headless servers or if you prefer the command line:

##### Option 1: Install Script (Recommended)

Install or update with a single command (no Rust toolchain required):

```bash
curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh
```

Install with FUSE mount support (macOS Apple Silicon only):

```bash
curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh -s -- --fuse
```

Install a specific version:

```bash
curl -fsSL https://raw.githubusercontent.com/zim/zim/main/install.sh | sh -s -- --version 0.1.9
```

Re-running the script updates to the latest version. The binary is installed to `~/.local/bin` by default (set `ZIM_INSTALL_DIR` to change). On interactive terminals, the script will prompt to install the FUSE variant on supported platforms.

##### Option 2: Install from Crates.io

For Rust developers who prefer cargo:

```bash
cargo install zim-peer
```

This will download, compile, and install the `zim` binary to `~/.cargo/bin/`.

##### Option 3: Install from Git Repository

Install the latest development version:

```bash
cargo install --git https://github.com/zim/zim zim-peer
```

##### Option 4: Build from Source

Clone and build manually for development or customization:

```bash
# Clone the repository
git clone https://github.com/zim/zim.git
cd zim

# Build in release mode
cargo build --release

# Install to ~/.cargo/bin
cargo install --path crates/zim-peer

# Or run directly from the build directory
./target/release/zim --help
```

### Verify Installation

After installation, verify that `zim` is in your PATH:

```bash
zim --help
```

You should see output like:
```
A basic CLI example

Usage: zim [OPTIONS] <COMMAND>

Commands:
  bucket
  init
  daemon
  version
  help     Print this message or the help of the given subcommand(s)
```

If the command is not found, ensure `~/.cargo/bin` is in your PATH:

```bash
# Add to your shell profile (.bashrc, .zshrc, etc.)
export PATH="$HOME/.cargo/bin:$PATH"
```

## Initial Setup

### 1. Initialize Configuration

Create the configuration directory and generate your identity:

```bash
zim init
```

This creates:
- `~/.config/zim/` - Configuration directory (or custom path if specified with `--config-path`)
- `config.toml` - Daemon configuration
- `secret.pem` - Your Ed25519 identity keypair (keep this secure!)
- `zim.db` - SQLite database for bucket metadata
- `blobs/` - Directory for encrypted blob storage

**Security Note:** The `secret.pem` file contains your private key. Keep it secure and back it up safely. Anyone with access to this file can decrypt your buckets and impersonate you.

### 2. Configure Daemon (Optional)

The default configuration works out of the box, but you can customize settings by editing the generated `config.toml`:

```toml
[node]
# Path to your identity key
secret_key_path = "secret.pem"

# Path to blob storage
blobs_path = "blobs"

# Network bind port (0 = random ephemeral port)
bind_port = 0

[database]
# SQLite database path
path = "db.sqlite"

[http_server]
# API server listen address
api_addr = "127.0.0.1:3000"

# Web UI listen address
html_addr = "127.0.0.1:8080"
```

### 3. Start the Daemon

```bash
zim daemon
```

The daemon will:
- Start the HTTP API server on `http://localhost:3000`
- Start the Web UI server on `http://localhost:8080`
- Initialize the Iroh P2P node
- Begin listening for sync events
- Display your Node ID (public key)

Keep this running in a terminal, or run it as a background service (see below).

### zim-hub (Gateway / Relay)

For serving published content and relaying browser-signed writes, deploy `zim-hub`:

```bash
make hub
```

The hub provides:
- P2P peer syncing as a mirror (pins ciphertext, no decryption)
- `/gw/:bucket_id/published/*path` endpoint for serving per-file/folder published content
- `POST /api/v0/buckets/:id/append` — Relay endpoint for browser-signed manifest updates
- Google OAuth identity vault for multi-tenant web access
- `/_status/*` health endpoints
- `?download=true` query param for raw file downloads

Use this when you need a minimal content server without the full daemon features.

### 4. Access the Web UI

Open your browser and navigate to:
```
http://localhost:8080
```

You should see the Zim dashboard.

## Running as a Background Service

### Linux (systemd)

Create a systemd service file at `~/.config/systemd/user/zim.service`:

```ini
[Unit]
Description=Zim P2P Storage Daemon
After=network.target

[Service]
Type=simple
ExecStart=%h/.cargo/bin/zim daemon
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=default.target
```

Enable and start the service:
```bash
systemctl --user enable zim
systemctl --user start zim

# Check status
systemctl --user status zim

# View logs
journalctl --user -u zim -f
```

### Linux (OpenRC / Gentoo)

Create an init script at `/etc/init.d/zim-peer`:

```bash
#!/sbin/openrc-run

description="Zim P2P Storage Daemon"

command="/home/YOUR_USERNAME/.cargo/bin/zim"
command_args="daemon"
command_user="YOUR_USERNAME:YOUR_USERNAME"
command_background=true
pidfile="/run/${RC_SVCNAME}.pid"

output_log="/var/log/${RC_SVCNAME}.log"
error_log="/var/log/${RC_SVCNAME}.err"

depend() {
    need net
    after firewall
}
```

Install and start the service:
```bash
# Make the script executable
chmod +x /etc/init.d/zim-peer

# Add to default runlevel
rc-update add zim-peer default

# Start the service
rc-service zim-peer start

# Check status
rc-service zim-peer status

# View logs
tail -f /var/log/zim-peer.log
```

### macOS (launchd)

Create a launch agent at `~/Library/LaunchAgents/com.zim.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.zim.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/YOUR_USERNAME/.cargo/bin/zim</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/zim.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/zim.err</string>
</dict>
</plist>
```

Load the daemon:
```bash
launchctl load ~/Library/LaunchAgents/com.zim.daemon.plist

# Check status
launchctl list | grep zim

# View logs
tail -f /tmp/zim.log
```

## Troubleshooting

### "Command not found: zim"

Ensure `~/.cargo/bin` is in your PATH:
```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### "Permission denied" on secret.pem

Fix file permissions:
```bash
chmod 600 ~/.config/zim/secret.pem
```

### "Database is locked"

Only one instance of `zim daemon` can run at a time. Stop any existing instances:
```bash
pkill -f "zim daemon"
```

### "Failed to bind address"

The HTTP port is already in use. Change it in `config.toml` or stop the conflicting service.

### FUSE: "Permission denied" or "Transport endpoint is not connected"

Ensure the FUSE kernel module is loaded and your user is in the `fuse` group:
```bash
# Load the FUSE module (if built as a module)
modprobe fuse

# Add your user to the fuse group
gpasswd -a YOUR_USERNAME fuse

# Log out and back in for the group change to take effect
```

On Gentoo, also verify your kernel config includes `CONFIG_FUSE_FS=y` or `CONFIG_FUSE_FS=m`. You can check with:
```bash
zgrep FUSE /proc/config.gz
```

### Reset Configuration

To start fresh:
```bash
# Backup first if needed
mv ~/.config/zim ~/.config/zim.backup

# Reinitialize
zim init
```

## Next Steps

- Read [concepts/](./concepts/) to understand how Zim works internally
- Check [DEVELOPMENT.md](./DEVELOPMENT.md) for development and contribution guidelines

## Getting Help

- **Documentation**: https://docs.rs/zim-peer
- **Issues**: https://github.com/zim/zim/issues
- **Discussions**: https://github.com/zim/zim/discussions
