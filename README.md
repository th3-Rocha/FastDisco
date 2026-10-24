<h1 align="center">FastDisco</h1>

<p align="center"><strong>Discord, native and fast.</strong><br>A lightweight Discord app for Linux, macOS, and Windows.</p>

FastDisco is a Discord client written in Rust with [egui](https://www.egui.rs)
and [fastframe](https://fastframe.dev), the framework behind Spotifast and
ZapFast. No browser engine, no Electron.

The layout follows Discord's desktop client on purpose: the server rail on
the left, channels beside it with your user panel at the foot, the
conversation in the middle and the member list on the right. Every button
sits where Discord users expect it.

> **Status:** early development. Login works against Discord; servers and
> messages still come from sample data until the gateway lands.

Signing in with email and password from outside a browser usually makes
Discord ask for an hCaptcha, which FastDisco cannot show. Use the QR code
with the Discord phone app instead.

> **Warning:** Discord's Terms of Service do not allow third-party clients
> that log in with a user account. Using FastDisco may put your account at
> risk. Use it at your own discretion.

## Build

Requires Rust 1.98+.

```sh
cargo run
```

## MVP

- [x] Discord layout: server rail, channels, chat, member list, user panel
- [x] Unread dots, mention badges, collapsible categories, mute and deafen
- [x] Sending a message shows it at once
- [x] Login: QR code from the phone app, email and password with two-factor code, or token; the token is kept in the OS keyring
- [ ] Gateway: servers, channels, members and presence
- [ ] Message history and sending through the REST API
- [ ] Direct messages
- [ ] Notifications and tray
- [ ] Avatars and server icons

## License

MIT
