# Hamachi-TUI

A terminal user interface for managing [LogMeIn Hamachi](https://vpn.net) VPN networks on Linux.

```
┌ Hamachi-TUI ──────────────────────────┐
│ ● Online │ 25.12.34.56 │ 123-456-789 │
│ ▸ alex            [Logout]     [Quit] │
├ Networks (2) ─────────────────────────┤
│ ● GameNight [210-555-001]         2/3 │
│   ● sam direct                        │
│   ● jordan relay                      │
│▸ ● WorkVPN [210-555-002]          1/2 │
│   ● taylor direct                     │
│ + Join Network                        │
│ + Create Network                      │
└───────────────────────────────────────┘
```

## Features

- Browse networks and peers in a single navigable list
- Create, join, leave, and delete networks
- Go online/offline per network
- Set network passwords and access control
- Evict peers from networks
- Change your nickname
- Non-blocking commands with loading indicator
- Vim-style `hjkl` navigation

## Prerequisites

Hamachi must be installed and the daemon running:

```sh
# Install (Arch Linux AUR)
yay -S logmein-hamachi

# Start the daemon
sudo systemctl start logmein-hamachi
```

See [vpn.net](https://vpn.net) for installation on other distros.

## Build & Install

```sh
# Build from source
git clone https://github.com/yourusername/hamachi-tui.git
cd hamachi-tui
cargo build --release

# Run
./target/release/hamachi-tui

# Or install via cargo
cargo install --path .
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `h` / `←` | Move left |
| `l` / `→` | Move right |
| `Tab` | Jump between sections |
| `Enter` | Select / confirm |
| `Esc` | Close popup / cancel |
| `Ctrl+C` | Quit |

## License

MIT
