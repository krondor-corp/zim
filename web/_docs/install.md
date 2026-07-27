---
title: Install
order: 3
---

## System Requirements

### Operating systems

- **Linux** — any modern distribution (Ubuntu 20.04+, Debian 11+, Fedora 35+, etc.)
- **macOS** — 10.15 (Catalina) or later
- **Windows** — Windows 10/11 with WSL2 (native Windows is experimental)

### System libraries

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

Ensure your kernel has FUSE support (`CONFIG_FUSE_FS=y` or `CONFIG_FUSE_FS=m`). If built as a module: `modprobe fuse`. Add your user to the `fuse` group:

```bash
gpasswd -a YOUR_USERNAME fuse
```

#### macOS

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
brew install openssl sqlite3
```

### Hardware

- **Minimum:** 2 cores, 2 GB RAM, 500 MB disk + space for your encrypted data.
- **Recommended:** 4+ cores, 4+ GB RAM, 10+ GB disk, stable internet for P2P sync.

## Install

### From crates.io

```bash
cargo install zim-peer
```

This installs the `zim` binary to `~/.cargo/bin/`.

### From git

```bash
cargo install --git https://github.com/zim/zim zim-peer
```

### From source

```bash
git clone https://github.com/zim/zim.git
cd zim
cargo build --release
cargo install --path crates/zim-peer
```

### Verify

```bash
zim --help
```

If the command isn't found, ensure `~/.cargo/bin` (or `~/.local/bin`) is in your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Initial Setup

### 1. Initialize

```bash
zim init
```

Creates the local state directory with:

- `config.toml` — daemon configuration
- `secret.pem` — your Ed25519 identity keypair (back this up — anyone with this file can decrypt your buckets and impersonate you)
- a SQLite database for bucket metadata
- `blobs/` — directory for encrypted blob storage

### 2. Start the daemon

```bash
zim daemon
```

The daemon starts the HTTP API, the local Web UI, and an iroh P2P endpoint. Keep it running while you use the CLI in another shell.

### 3. (Optional) Configure

The default `config.toml` works out of the box. To customize ports or paths:

```toml
[node]
secret_key_path = "secret.pem"
blobs_path = "blobs"
bind_port = 0  # 0 = random ephemeral port

[database]
path = "db.sqlite"

[http_server]
api_addr = "127.0.0.1:3000"
html_addr = "127.0.0.1:8080"
```

## Running as a Background Service

### Linux (systemd)

`~/.config/systemd/user/zim.service`:

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

```bash
systemctl --user enable zim
systemctl --user start zim
journalctl --user -u zim -f
```

### macOS (launchd)

`~/Library/LaunchAgents/com.zim.daemon.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.zim.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/YOUR_USERNAME/.cargo/bin/zim</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
    <key>StandardOutPath</key><string>/tmp/zim.log</string>
    <key>StandardErrorPath</key><string>/tmp/zim.err</string>
</dict>
</plist>
```

```bash
launchctl load ~/Library/LaunchAgents/com.zim.daemon.plist
```

### Linux (OpenRC / Gentoo)

`/etc/init.d/zim`:

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

```bash
chmod +x /etc/init.d/zim
rc-update add zim default
rc-service zim start
```

## Troubleshooting

### "Command not found: zim"

Ensure `~/.cargo/bin` (or `~/.local/bin`) is in your `PATH`.

### "Permission denied" on the secret key

```bash
chmod 600 <state-dir>/secret.pem
```

### "Database is locked"

Only one `zim daemon` instance can run at a time:

```bash
pkill -f "zim daemon"
```

### "Failed to bind address"

The HTTP port is already in use. Change it in `config.toml` or stop the conflicting service.

### FUSE: "Permission denied" or "Transport endpoint is not connected"

Ensure the FUSE kernel module is loaded and your user is in the `fuse` group:

```bash
modprobe fuse
gpasswd -a YOUR_USERNAME fuse
# Log out and back in for the group change to take effect.
```

On Gentoo, verify your kernel includes `CONFIG_FUSE_FS=y` or `CONFIG_FUSE_FS=m`:

```bash
zgrep FUSE /proc/config.gz
```
