# OpenBroyach

[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows-blue)](#requirements)
[![License: MIT](https://img.shields.io/badge/license-MIT-yellow.svg)](LICENSE.md)

**Your AI quotas deserve a dashboard, not a guessing game.**

OpenBroyach is a terminal dashboard for the AI coding tools already on your machine. It brings session limits, weekly quotas, credits, reset times, and local spend estimates into one fast, keyboard-driven view.

No new account. No copied API keys. Just `openbroyach`.

`Брояч` means _counter_. That is the job.

```text
Session       -> what you are using now
Weekly        -> what remains for the long haul
Credits       -> what is left in the tank
Local spend   -> what your own logs add up to
```

Same subscriptions. Fewer surprises.

## Why This Exists

AI coding tools are good at showing usage inside their own walls. They are less good at answering one ordinary question:

> How much room do I actually have left?

Checking three dashboards breaks flow. Waiting for a rate-limit error is worse. OpenBroyach keeps the answer in the terminal, beside the work.

It borrows the core idea from [OpenUsage](https://github.com/robinebers/openusage), then reshapes it for people who live in shells, tmux sessions, remote machines, and keyboard-first desktops.

## Install

### Cargo

```bash
cargo install --git https://github.com/tokmac/openbroyach
```

### Omarchy / Arch Linux

Download `openbroyach-linux-x86_64.tar.gz` from the latest GitHub release, then install it for your user:

```bash
tar -xzf openbroyach-linux-x86_64.tar.gz
install -Dm755 openbroyach "$HOME/.local/bin/openbroyach"
openbroyach --version
```

Omarchy already includes `xdg-open`, which OpenBroyach uses for dashboard links. Antigravity additionally needs an unlocked Secret Service provider. If credential lookup fails, install and start GNOME Keyring with `omarchy pkg add gnome-keyring libsecret`, then sign out and back in so the graphical session initializes it. Codex and OpenCode do not require GNOME Keyring.

### From Source

```bash
git clone https://github.com/tokmac/openbroyach.git
cd openbroyach
cargo build --release
```

The binary will be at `target/release/openbroyach` (`openbroyach.exe` on Windows).

## Run

```bash
openbroyach
```

OpenBroyach detects supported providers from credentials and local state that their official tools already created. Sign in to at least one supported provider before launching it.

There is no configuration ceremony. Provider order is the only persisted preference.

## Supported Providers

| Provider | What OpenBroyach Shows | Credential Source |
| --- | --- | --- |
| **Antigravity** | Gemini and Claude quota pools, session and weekly windows | System credential store used by Antigravity |
| **Codex** | Session and weekly limits, credits, resets, and local spend | Codex `auth.json` and local session logs |
| **Claude Code** | Five-hour and weekly subscription limits plus extra usage | Claude Code OAuth credentials |
| **OpenCode** | Go session, weekly, and monthly caps plus local usage | OpenCode `auth.json` and local message data |

Only providers with usable local credentials appear. OpenBroyach reads those credentials for the corresponding provider requests; it never asks you to paste them into the app.

### Antigravity On Linux

Antigravity requires an unlocked Secret Service collection, commonly provided by GNOME Keyring. You can verify the same record OpenBroyach reads with:

```bash
secret-tool lookup service gemini username antigravity
```

Do not share that command's output.

## Features

- **One terminal dashboard.** See every detected provider without opening browser tabs.
- **Real quota windows.** Session, weekly, monthly, credits, expiry, and reset timing use provider-native data where available.
- **Local spend estimates.** Codex and OpenCode activity is calculated from local logs with embedded model pricing.
- **Five-minute refresh.** Providers refresh concurrently on launch, on demand, and automatically.
- **Last-good snapshots.** A temporary refresh failure keeps useful data visible and marks it stale instead of blanking the screen.
- **Responsive layout.** Progress bars, compact summaries, scrolling, and fallback logos adapt to the terminal size.
- **Persistent ordering.** Put the providers you care about first and keep them there.
- **Numbered links.** Open a selected provider's status or dashboard link without reaching for the mouse.

## Keybindings

| Key | Action |
| --- | --- |
| `j` / `k`, `Down` / `Up` | Select a provider |
| `Enter` / `Space` | Expand or collapse |
| `Left` / `Right` | Collapse or expand |
| `J` / `K` | Move the selected provider down or up |
| `Home` / `End` | Jump to the first or last provider |
| `1`-`9` | Open the selected provider's numbered link |
| `r` | Refresh all providers |
| `q` / `Esc` | Quit |

## Built With

- [Ratatui](https://ratatui.rs/) for the terminal interface
- [Crossterm](https://github.com/crossterm-rs/crossterm) for cross-platform terminal input
- [Tokio](https://tokio.rs/) for concurrent provider refreshes
- [Reqwest](https://docs.rs/reqwest/) with Rustls for provider HTTP requests
- [rusqlite](https://docs.rs/rusqlite/) for local usage databases

One Rust binary. No Node.js runtime. Pricing snapshots and provider logos are embedded at build time.

## Design Principles

- Glanceable over exhaustive
- Local credentials over another login screen
- Useful stale data over empty failure states
- Keyboard flow over dashboard tourism
- Provider facts over invented estimates

## What OpenBroyach Is Not

- Not a billing system
- Not a credential manager
- Not a replacement for provider dashboards
- Not a promise that local spend estimates match an invoice exactly

It is a quick, honest view of the limits and activity your tools expose.

## Requirements

- Linux, macOS, or Windows on a 64-bit system
- A terminal at least 32 columns by 7 rows
- A local sign-in for at least one supported provider
- Rust 1.88 or later when building from source
- On Linux, a running Secret Service provider for Antigravity credential access

Prebuilt Linux releases target 64-bit glibc systems, including current Arch Linux and Omarchy installations.

## Development

```bash
cargo fmt --check
cargo test
cargo run
```

Release build:

```bash
cargo build --release --locked
```

## Contributing

OpenBroyach is early. Bug reports from different terminals, operating systems, account types, and provider plans are especially useful.

When reporting a problem, include your operating system, terminal, provider, and the exact on-screen error. Never include access tokens, refresh tokens, API keys, or credential-store output.

## Acknowledgements

OpenBroyach is inspired by [OpenUsage](https://github.com/robinebers/openusage), the native macOS menu-bar tracker that established the provider normalization and local-first usage model behind this project.

## License

[MIT](LICENSE.md)

---

_Count the quota. Keep the flow._
