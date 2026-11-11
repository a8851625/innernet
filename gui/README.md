# innernet GUI

Desktop client for [innernet](https://github.com/tonarino/innernet), built with
Tauri 2. Ships for macOS and Windows; the CLI remains the source of truth for
Linux.

## Design

The GUI does **not** reimplement WireGuard handling. It drives the `innernet`
CLI and renders its state:

```
 GUI (Tauri)  --spawn-->  innernet CLI  ---->  WireGuard
      |                        ^
      +--- `show --json` ------+
```

Two reasons for this boundary:

1. **Privilege scope.** `innernet install/up` need root. If the GUI linked the
   logic in directly, the whole app bundle would need to run as root. Instead
   only the specific action is elevated, through the native macOS authorisation
   prompt (`do shell script ... with administrator privileges`), so the user
   sees a system dialog rather than trusting the app with a password.
2. **One implementation.** The CLI is what admins already use and what the
   server-side tests cover. The GUI cannot drift from it.

## The JSON contract

`innernet show --json` emits a stable document (`client/src/json.rs`). GUIs
should depend on it rather than scraping the coloured table output.

```json
{
  "version": "2.0.0",
  "config_dir": "/etc/innernet",
  "permission_denied": false,
  "interfaces": [
    {
      "name": "heheda-mesh",
      "network_name": "heheda-mesh",
      "is_up": true,
      "listen_port": 51820,
      "address": "10.60.1.2",
      "network_cidr_prefix": 16,
      "public_key": "...",
      "me": null,
      "peers": [
        {
          "name": "mac-office",
          "ip": "10.60.1.1",
          "public_key": "...",
          "cidr_id": 3,
          "is_admin": true,
          "is_disabled": false,
          "is_you": false,
          "is_connected": true,
          "endpoint": "210.22.9.130:3153",
          "endpoint_overridden": false,
          "last_handshake_secs_ago": 12,
          "rx_bytes": 5242880,
          "tx_bytes": 1048576,
          "allowed_subnets": ["192.168.31.0/24"]
        }
      ],
      "cidrs": [{ "id": 3, "name": "humans", "cidr": "10.60.1.0/24", "parent": 1 }],
      "endpoint_overrides": []
    }
  ]
}
```

`permission_denied` exists because without it a GUI cannot distinguish "no
networks configured" from "not enough privileges to read the state" — the CLI
returns an empty list for both, which renders as a misleading empty screen.

## Building

```bash
# macOS
cd gui/src-tauri
cargo build --release          # -> target/release/innernet-gui
cargo tauri build              # -> .app / .dmg (needs `cargo install tauri-cli`)

# Windows (from a Windows host; Tauri needs the platform's WebView)
cd gui\src-tauri
cargo build --release
cargo tauri build --bundles nsis
```

## Locating the CLI

The app probes `/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`,
`/opt/innernet/bin`, then `PATH`. Override with `INNERNET_CLI=/path/to/innernet`.

This matters because a double-clicked `.app` on macOS inherits a minimal PATH
that usually does not include Homebrew.

## Privileged actions

The front-end can only request these, never an arbitrary command string:

| action          | runs                        |
|-----------------|-----------------------------|
| `up`            | `innernet up`               |
| `up_interface`  | `innernet up <iface>`       |
| `fetch`         | `innernet fetch <iface>`    |
| `down`          | `innernet uninstall <iface>`|

Arguments are shell-quoted before being handed to `osascript`, and the quoting
is covered by unit tests (`shell_quoting_handles_quotes`).
