![ProjectAluminum](/docs/assets/logo.png)

<p align="center">
  <img src="/docs/assets/screenshot.png" width="75%">
</p>

# Aluminum - a secure P2P messenger
Aluminum is a secure P2P messenger right in your terminal. Even though user privacy is at the core of its design it's still easy to use.

## Features
- **Unlimited file sharing** - share however much you want. There are no file size limits in the app
- **Sixel image previews - currently this is the only P2P terminal chat app that implements this!**
- **Built-in UDP Hole Punching** support allows you to connect to anyone
- **A Ratatui-based TUI** makes the app much simpler to use - command-line usage is kept to the bare minimum
- **Multiple identities** support allows you to switch out your identities as you wish

## Security measures
- **End-to-end encryption** - All traffic is end-to-end encrypted using a Noise session protocol (tl;dr: Noise protocols are a modern alternative to TLS, for more details see [the Noise protocol framework specification](http://www.noiseprotocol.org/))
- **No registration** - There's no centralized registration service involved anywhere. All message routing is done using public-key cryptography identities. You're never expected to send any passwords anywhere - your private keys stay on your device.
- **Out-of-band identity sharing** - Peer identities are exchanged in an out-of-band fashion to avoid the possibility of a Man-in-the-middle attack
- **State-of-the-art cryptography** - The app uses Ed25519 elliptic curve cryptography for peer identity and the ChaCha20-Poly1305 symmetric cipher for session encryption
- **Reliable QUIC message delivery** - All messages are sent reliably using the QUIC transport protocol

Additionally, the whole app is fully written in Safe Rust.

## Quickstart
Clone this repository and build it with:
```bash
cargo build --release
```
The resulting `aluminum` binary is placed in the `/target/release` directory. This will also produce a `p2p-relay` binary that you can then use to set up your own custom relay server.

> [!NOTE]
> You can access built-in help by typing `aluminum -h`

### First launch
Upon first launch you will be asked some questions about your identity. Don't worry, it's not sent anywhere - this data is used to construct your peer identity that you can share out-of-band with your friends.

### Sharing identities
After launching the app for the first time and closing it you should have a user database generated inside of your Aluminum directory (by default it's placed in `~/.local/share/aluminum/user.db`).

You can now export your identity:
```bash
aluminum --export nickname.usr
```

Share it with someone in an out-of-band fashion. They can then load that file with:
```bash
aluminum --import nickname.usr
```

to add you to their friend list. After exchanging your identities you will be able to connect to each other from the friends list view.

## Custom relay servers
You can launch your own relay server by launching `p2p-relay`. This will create a `server.log` file inside the current directory which you can then tail to view the server logs.

Your friends will need to update their `~/.local/share/aluminum/relay.toml` file. It has a simple structure:
```toml
addr = "<relay_ip_address>:<port_number>"
public_key = "<relay_public_key>"
```

You can obtain your relay public key by typing in
```bash
p2p-relay --print-public
```
