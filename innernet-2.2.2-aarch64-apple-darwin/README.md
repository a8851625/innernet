# innernet

[![Actively Maintained](https://img.shields.io/badge/Maintenance%20Level-Actively%20Maintained-green.svg)](https://gist.github.com/cheerfulstoic/d107229326a01ff0f333a1d3476e068d)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/tonarino/innernet/blob/master/LICENSE)

A private network system that uses [WireGuard](https://wireguard.com) under the hood. See the [announcement blog post](https://blog.tonari.no/introducing-innernet) for a longer-winded explanation.

> **This is a fork** of [tonarino/innernet](https://github.com/tonarino/innernet) that adds
> subnet routing, a [web admin UI](#web-admin-ui), and a desktop GUI. Prebuilt binaries are on the
> [releases page](https://github.com/a8851625/innernet/releases); see
> [Getting started](#getting-started) for install options.

<img src="https://user-images.githubusercontent.com/373823/118917068-09ae7700-b96b-11eb-80f4-6860072d504d.gif" width="600" height="370">

`innernet` is similar in its goals to Slack's [nebula](https://github.com/slackhq/nebula) or [Tailscale](https://tailscale.com/), but takes a bit of a different approach. It aims to take advantage of existing networking concepts like CIDRs and the security properties of WireGuard to turn your computer's basic IP networking into more powerful ACL primitives.

`innernet` is not an official WireGuard project, and WireGuard is a registered trademark of Jason A. Donenfeld.

This has not received an independent security audit, and should be considered experimental software at this early point in its lifetime.

## Getting started

Prebuilt binaries for Linux and macOS (x86_64 and arm64) are published on the
[releases page](https://github.com/a8851625/innernet/releases).

### Homebrew (macOS)

```sh
brew tap a8851625/innernet
brew install innernet
```

This installs the prebuilt binaries, not a source build, so no Rust toolchain
is needed.

### Keeping the interface up (Linux and macOS)

`innernet up` runs once: it fetches the peer list, attempts NAT traversal, and
exits. Since a peer behind a translating NAT may not answer within that window,
a single `up` can leave the interface without connectivity even though a
connection is possible. Install a service so the interface is brought up at
boot and keeps retrying:

```sh
sudo innernet install-service heheda-mesh
```

On Linux this writes `/etc/systemd/system/innernet@heheda-mesh.service`, runs
`systemctl daemon-reload`, and enables plus starts the unit. On macOS it writes
`/Library/LaunchDaemons/com.innernet.heheda-mesh.plist` and bootstraps it via
`launchctl`. Either way the generated unit points at the binary you invoked, so
a Homebrew install (`/opt/homebrew/bin`) or a custom prefix works — the
`systemd` template in this repo hardcodes `/usr/bin` and is only a reference
for packagers.

```sh
# follow what it is doing
systemctl status innernet@heheda-mesh      # Linux
sudo launchctl print system/com.innernet.heheda-mesh   # macOS
tail -f /var/log/innernet-heheda-mesh.log  # macOS log

# remove it
sudo innernet uninstall-service heheda-mesh
```

`uninstall-service` removes the unit and stops the daemon, but leaves
`/etc/innernet` and `/var/lib/innernet` untouched, so the network membership
survives and you can bring the interface back up with `innernet up`.

### Install script (Linux and macOS)

```sh
curl -fsSL https://raw.githubusercontent.com/a8851625/innernet/main/install.sh | sh
```

The script detects your platform, verifies the release checksum, and installs
`innernet` and `innernet-server`. Options are passed after `sh -s --`:

```sh
# Pin a version
curl -fsSL .../install.sh | sh -s -- --version v2.1.0

# Install somewhere else (e.g. no sudo)
curl -fsSL .../install.sh | sh -s -- --bin-dir ~/.local/bin

# Remove the binaries
curl -fsSL .../install.sh | sh -s -- --uninstall
```

Installing leaves `/etc/innernet` and `/var/lib/innernet` in place on
uninstall: they hold network membership, and re-joining needs a fresh
invitation.

### Building from source

```sh
git clone https://github.com/a8851625/innernet
cd innernet
cargo build --release -p innernet -p innernet-server
```

On macOS the client needs a userspace WireGuard implementation
(`brew install wireguard-go`). Linux uses the kernel module.

### Windows

Not supported yet. The client depends on Unix domain sockets and Linux
netlink for interface and route management, so it does not build for Windows.
See [#1](https://github.com/a8851625/innernet/issues) for status.

## Web admin UI

`innernet-server` serves a small admin interface so a network can be managed
without shelling into the server. It listens on loopback only, by default at
`127.0.0.1:8380`:

```sh
sudo innernet-server serve <interface>                       # http://127.0.0.1:8380/web
sudo innernet-server serve <interface> --web-listen 0.0.0.0:8380   # expose it yourself
sudo innernet-server serve <interface> --web-listen ""       # turn it off
```

On first start the server creates an `admin` account and prints the generated
password once:

```
  ┌──────────────────────────────────────────────────────┐
  │ web admin UI enabled                                 │
  │   url      : http://127.0.0.1:8380/web               │
  │   username : admin                                   │
  │   password : <generated, shown only this time>       │
  └──────────────────────────────────────────────────────┘
```

Only a hash is stored. To reset the password, delete the `web_accounts` row and
restart the server; a fresh password is generated and printed.

Because it binds to loopback, reach it over an SSH tunnel:

```sh
ssh -N -L 8380:127.0.0.1:8380 root@<server>
# then open http://127.0.0.1:8380/web
```

What it can do:

| Section | Actions |
| --- | --- |
| Peers | invite a device, rename, enable/disable, delete, edit subnet routes |
| CIDRs | add, rename, delete |
| Associations | list, add, remove |

Inviting a peer generates the invitation and shows it once, with copy and
download buttons. The private key is never written to the server's disk — it
exists only in that response — so if you lose it, delete the peer and invite
again. Route edits, deletions and revocation all reach the running WireGuard
interface immediately; there is no restart step.

Every action is admin-only, authenticated by a session cookie, and the API
rejects unauthenticated requests with `401`. The listener speaks plain HTTP and
has no TLS of its own, which is why it defaults to loopback — put a reverse
proxy in front of it if you need remote access, and do not expose it directly.

## Usage

### Server Creation

Every `innernet` network needs a coordination server to manage peers and provide endpoint information so peers can directly connect to each other. Create a new one with

```sh
sudo innernet-server new
```

The init wizard will ask you questions about your network and give you some reasonable defaults. It's good to familiarize yourself with [network CIDRs](https://en.wikipedia.org/wiki/Classless_Inter-Domain_Routing) as a lot of innernet's access control is based upon them. As an example, let's say the root CIDR for this network is `10.60.0.0/16`. Server initialization creates a special "infra" CIDR which contains the `innernet` server itself and is reachable from all CIDRs on the network.

Next we'll also create a `humans` CIDR where we can start adding some peers.

```sh
sudo innernet-server add-cidr <interface>
```

For the parent CIDR, you can simply choose your network's root CIDR. The name will be `humans`, and the CIDR will be `10.60.64.0/24` (not a great example unless you only want to support 256 humans, but it works for now...).

By default, peers which exist in this new CIDR will only be able to contact peers in the same CIDR, and the special "infra" CIDR which was created when the server was initialized.

A typical workflow for creating a new network is to create an admin peer from the `innernet-server` CLI, and then continue using that admin peer via the `innernet` client CLI to add any further peers or network CIDRs.

```sh
sudo innernet-server add-peer <interface>
```

Select the `humans` CIDR, and the CLI will automatically suggest the next available IP address. Any name is fine, just answer "yes" when asked if you would like to make the peer an admin. The process of adding a peer results in an invitation file. This file contains just enough information for the new peer to contact the `innernet` server and redeem its invitation. It should be transferred securely to the new peer, and it can only be used once to initialize the peer.

You can run the server with `innernet-server serve <interface>`, or if you're on Linux and want to run it via `systemctl`, run `systemctl enable --now innernet-server@<interface>`. If you're on a home network, don't forget to configure port forwarding to the `Listen Port` you specified when creating the `innernet` server.

### Peer Initialization

Let's assume the invitation file generated in the steps above have been transferred to the machine a network admin will be using.

You can initialize the client with

```sh
sudo innernet install /path/to/invitation.toml
```

You can customize the network name if you want to, or leave it at the default. `innernet` will then connect to the `innernet` server via WireGuard, generate a new key pair, and register that pair with the server. The private key in the invitation file can no longer be used.

If everything was successful, the new peer is on the network. You can run things like

```sh
sudo innernet list
```

or

```sh
sudo innernet list --tree
```

to view the current network and all CIDRs visible to this peer.

Since we created an admin peer, we can also add new peers and CIDRs from this peer via `innernet` instead of having to always run commands on the server.

### Adding Associations between CIDRs

In order for peers from one CIDR to be able to contact peers in another CIDR, those two CIDRs must be "associated" with each other.

With the admin peer we created above, let's add a new CIDR for some theoretical CI servers we have.

```sh
sudo innernet add-cidr <interface>
```

The name is `ci-servers` and the CIDR is `10.60.64.0/24`, but for this example it can be anything.

For now, we want peers in the `humans` CIDR to be able to access peers in the `ci-servers` CIDR.

```sh
sudo innernet add-association <interface>
```

The CLI will ask you to select the two CIDRs you want to associate. That's all it takes to allow peers in two different CIDRs to communicate!

You can verify the association with

```sh
sudo innernet list-associations <interface>
```

and associations can be deleted with

```sh
sudo innernet delete-associations <interface>
```

### Enabling/Disabling Peers

A peer can be *disabled*, which revokes its access while keeping its address
reserved. Disabled peers are left out of the config that other peers fetch, and
their key is removed from the WireGuard interface.

Disable a peer with

```sh
sudo innernet disable-peer <interface>
```

Or re-enable a peer with

```sh
sudo innernet enable-peer <interface>
```

Disabling is the reversible option and is what you want for a device that is
merely offline. To free the address instead, remove the peer outright — see
[Deleting a Peer](#deleting-a-peer).

### Deleting a Peer

Disabling a peer keeps its address reserved forever. When a device is gone for
good, delete the peer instead: its key is detached from the WireGuard interface
(so access stops immediately) and its row is dropped from the database, which
returns the address to the free pool.

Deleting is only available through the [web admin UI](#web-admin-ui) or the
admin HTTP API — the `innernet` CLI intentionally keeps only the reversible
disable. The server peer itself cannot be deleted.

### Advertising Subnet Routes

A peer can act as a gateway for a network behind it. For example, a machine at
home that can reach `192.168.1.0/24` can advertise that subnet, and every other
peer then routes those addresses through it.

```sh
sudo innernet-server add-peer <interface> --advertise-routes 192.168.1.0/24
```

Routes are stored per peer and pushed to the other peers as extra WireGuard
allowed-ips, so the traffic is encrypted and authenticated like any other
peer-to-peer packet. Several routes can be given comma-separated:

```sh
--advertise-routes 192.168.1.0/24,10.20.0.0/16
```

For an existing peer, edit the routes in the [web admin UI](#web-admin-ui) —
changes are applied to the running interface immediately, with no restart. To
revoke them, remove the routes or disable the peer; disabling clears the routes
as well, so a compromised gateway stops advertising at once.

The gateway machine must also forward and masquerade the traffic itself, for
example:

```sh
sudo sysctl -w net.ipv4.ip_forward=1
sudo iptables -t nat -A POSTROUTING -o <upstream-iface> -j MASQUERADE
```

### NAT Traversal Candidates

For two peers to talk directly, each has to learn an address that reaches the
other through its NAT. A peer reports candidates — its local interface
addresses, plus its public address as seen from outside — and the server hands
each side the list for the other. The server also injects the source address it
actually observes for a peer, which is usually the most accurate mapping.

Reporting the public address normally works by asking Quad9 (`whatismyip.on.quad9.net`)
over DNS. Behind a transparent proxy or a DNS fake-IP resolver that answer is
wrong (typically a reserved `198.18.0.0/15` address), so it is dropped with a
warning and the peer falls back to local candidates only. Supply the address
explicitly in that case:

```sh
sudo innernet fetch <interface> --public-endpoint 203.0.113.7:51820
```

A single `up` gives each candidate only a few seconds, which is often too short
when a NAT mapping is still being established. Run the interface as a daemon so
traversal is retried — see [Keeping the interface up](#keeping-the-interface-up-linux-and-macos).
If direct connectivity is impossible (symmetric NAT on both ends, for instance)
there is no relay to fall back on: innernet is WireGuard peer-to-peer, so the
workaround is to override the endpoint manually.

### Specifying a Manual Endpoint

The `innernet` server will try to use the internet endpoint it sees from a peer so other peers can connect to that peer as well. This doesn't always work and you may want to set an endpoint explicitly. To set an endpoint, use

```sh
sudo innernet override-endpoint <interface>
```

You can go back to automatic endpoint discovery with

```sh
sudo innernet override-endpoint -u <interface>
```

### Setting the Local WireGuard Listen Port

If you want to change the port which WireGuard listens on, use

```sh
sudo innernet set-listen-port <interface>
```

or unset the port and use a randomized port with

```sh
sudo innernet set-listen-port -u <interface>
```

### Remove Network

To permanently uninstall a created network, use

```sh
sudo innernet-server uninstall <interface>
```

Use with care!

## Security recommendations

If you're running a service on innernet, there are some important security considerations.

### Enable strict Reverse Path Filtering ([RFC 3704](https://tools.ietf.org/html/rfc3704))

Strict RPF prevents packets from _other_ interfaces from having internal source IP addresses. This is _not_ the default on Linux, even though it is the right choice for 99.99% of situations. You can enable it by adding the following to a `/etc/sysctl.d/60-network-security.conf`:

```
net.ipv4.conf.all.rp_filter = 1
net.ipv4.conf.default.rp_filter = 1

# Also apply to interfaces that exist at the time this sysctl is applied, as they might have been
# created with value 2 (loose), which takes precedence even when all.rp_filter is 1.
# Note that using the * match does not apply to the special `all` and `default` knobs.
net.ipv4.conf.*.rp_filter = 1
```

### Bind to the WireGuard device

If possible, to _ensure_ that packets are only ever transmitted over the WireGuard interface, it's recommended that you use `SO_BINDTODEVICE` on Linux or `IP_BOUND_IF` on macOS/BSDs. If you have strict reverse path filtering, though, this is less of a concern.

### IP addresses alone often aren't enough authentication

Even following all the above precautions, rogue applications on a peer's machines could be able to make requests on their behalf unless you add extra layers of authentication to mitigate this CSRF-type vector.

It's recommended that you carefully consider this possibility before deciding that the source IP is sufficient for your authentication needs on a service.

## Installation

`innernet` has only been officially tested on Linux and macOS. Experimental OpenBSD support has been added recently. Feedback from early testers is most welcome. Please report any issues you may encounter.
We hope to support more platforms in the future.

### Runtime Dependencies

It's assumed that WireGuard is present on your system, either via kernel support (`Linux 5.6` and later or `OpenBSD 6.8` and later) or by installing the [`wireguard-go`](https://git.zx2c4.com/wireguard-go/about/) userspace implementation.

[WireGuard Installation Instructions](https://www.wireguard.com/install/)

### Arch Linux

```sh
pacman -S innernet
```

### Debian and Ubuntu

[**@tommie**](https://github.com/tommie) is kindly providing Debian/Ubuntu innernet builds in the https://github.com/tommie/innernet-debian repository.

### Other Linux Distributions

We're looking for volunteers who are able to set up external builds for popular distributions. Please see issue [#203](https://github.com/tonarino/innernet/issues/203).

### macOS

```sh
brew install tonarino/innernet/innernet
```

### Cargo

```sh
# to install the latest released innernet:
cargo install --locked innernet

# to install the latest released innernet-server:
cargo install --locked innernet-server
```

Note that you'll be responsible for updating manually or through tools like [`cargo-update`](https://crates.io/crates/cargo-update).
You can omit `--locked` to use newer (but compatible) dependency versions, losing determinism and testing by our CI.

## Development

### Cargo build feature for SELinux

If your target system uses SELinux, you will want to enable the 'selinux' feature when building the innernet binary.
This will ensure that innernet maintains the correct selinux context on the /etc/hosts file when adding hosts.  To do so add ```--features selinux``` to the ```cargo build``` options.
The `selinux-devel` package will need to be installed for the correct headers.

### `innernet-server` Build dependencies

- `rustc` / `cargo` (version 1.50.0 or higher)
- `libclang` (see more info at [https://crates.io/crates/clang-sys](https://crates.io/crates/clang-sys))
- `libsqlite3`

Build:

```sh
cargo build --release --bin innernet-server
```

The resulting binary will be located at `./target/release/innernet-server`

### `innernet` Client CLI Build dependencies

- `rustc` / `cargo` (version 1.50.0 or higher)
- `libclang` (see more info at [https://crates.io/crates/clang-sys](https://crates.io/crates/clang-sys))

Build:

```sh
cargo build --release --bin innernet
```

The resulting binary will be located at `./target/release/innernet`

### Testing

You can manually invoke Docker-based tests assuming you have Docker daemon running. If you specify
`--interactive` flag, it allows you to attach to the server and client innernet Docker
containers, so you can test various innernet commands inside a sandboxed environment.

```
docker-tests/build-docker-images.sh
docker-tests/run-docker-tests.sh [--interactive]
```

If you are developing a new feature, please consider adding a new test case to `run-docker-tests.sh` ([example PR](https://github.com/tonarino/innernet/pull/310/files#diff-5d47ea0d379e45f35471e2afa48fdba09d2ca4bcf090e16bb1adccebea080081)).

### Releases

Please run the release script from a Linux machine: generated shell completions depend on available wireguard backends and Mac doesn't support the `kernel` backend. 

1. Ensure you have the release script dependencies `help2man` and `cargo-release` installed.
1. Create a new branch on top of fresh `main`.
1. Run `./release.sh [patch|major|minor|rc]` and submit its results as a PR.
1. Once the PR is merged, switch to `main` that contains it.
1. Publish to crates.io using `cargo release publish` (pass `--execute` after a dry run).
1. Tag the commit using `git tag -f -a v<version> -m "release v<version>`.
1. Push the created tag to the repo using `git push origin v<version>`.
1. Create a GitHub release from the tag, generate release notes and highlight important changes.
1. Bump the [homebrew-innernet](https://github.com/tonarino/homebrew-innernet) formula version.
