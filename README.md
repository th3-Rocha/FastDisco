<h1 align="center">FastDisco</h1>

<p align="center"><strong>Discord, native and fast.</strong><br>A lightweight Discord app for Linux, macOS, and Windows.</p>

FastDisco is a Discord client written in Rust with [egui](https://www.egui.rs)
and [fastframe](https://fastframe.dev), the framework behind Spotifast and
ZapFast. No browser engine, no Electron.

The layout follows Discord's desktop client on purpose: the server rail on
the left, channels beside it with your user panel at the foot, the
conversation in the middle and the member list on the right. Every button
sits where Discord users expect it.

> **Status:** early development. Login, servers, channels, direct messages
> and chatting work against Discord.

Signing in with email and password from outside a browser usually makes
Discord ask for an hCaptcha, which FastDisco cannot show. Use the QR code
with the Discord phone app instead.

> **Warning:** Discord's Terms of Service do not allow third-party clients
> that log in with a user account. Using FastDisco may put your account at
> risk. Use it at your own discretion.

## Download

Every version is built for Linux, Windows and macOS and published on the
[releases page](https://github.com/th3-Rocha/FastDisco/releases/latest).

- **Arch Linux**: `yay -S fastdisco-bin` (or any other AUR helper).
- **Ubuntu and other Linux desktops**: download
  `FastDisco-<version>-x86_64.AppImage`, then
  `chmod +x FastDisco-*.AppImage` and run it. Needs Ubuntu 24.04 or newer.
- **Linux, by hand**: the `x86_64-unknown-linux-gnu` archive has the
  binary, the launcher entry and the icons.
- **Windows** (x86_64) and **macOS** (Apple Silicon): unpack the archive
  and run `fastdisco`.

## Build

Requires Rust 1.98+. Video, audio and YouTube playback use `ffmpeg`,
`ffprobe` and `yt-dlp` when they are installed; without them the media
opens in the browser.

```sh
cargo run
```

`scripts/install.sh` builds a release and installs it for the current
user, with the launcher entry and icons.

## Releases

The version lives in `Cargo.toml`. Each push to `main` with a version that
has no tag yet builds every platform and publishes `v<version>` on GitHub,
with the commits since the last release as notes. A new feature bumps the
minor version, a fix bumps the patch.

## Roadmap

What works today, and what is still missing for FastDisco to do
everything Discord's desktop app does.

### Account and connection

- [x] Login with the QR code from the phone app
- [x] Login with email and password, with a two-factor code
- [x] Login with a token; the token is kept in the OS keyring
- [x] Gateway with heartbeat, resume and reconnect
- [ ] hCaptcha when Discord asks for one at login
- [ ] Switching between several accounts

### Layout and navigation

- [x] Discord's layout: server rail, channels, chat, member list, user panel
- [x] Unread dots, mention badges, collapsible categories
- [x] Rail tooltips, the pill beside the open server
- [x] Alt+Up and Alt+Down between channels
- [ ] Server folders in the rail
- [ ] Dragging servers and channels to reorder them
- [ ] Quick switcher (Ctrl+K)
- [ ] The rest of Discord's keyboard shortcuts
- [ ] Friends page: online, all, pending, blocked, add friend
- [ ] Inbox: recent mentions and unreads
- [ ] Search messages
- [ ] Pinned messages panel
- [ ] Mark as unread, mark server as read from the rail menu

### Messages

- [x] History, live messages, edits and deletions, older history on scroll
- [x] Sending with optimistic display
- [x] Direct messages and groups
- [x] Markdown: bold, italic, underline, strikethrough, code, code blocks, quotes, headings, lists, subtext, spoilers, masked links, mentions, timestamps
- [x] Custom and Unicode emoji, jumbo emoji, emoji picker, `:shortcode:` on send
- [x] Replies, edits, deletion, reactions, typing indicator
- [x] Discord's limits: character counter, "message too long" (send as file), upload size by plan
- [ ] Forwarding messages
- [ ] Polls
- [ ] Slash commands and app commands
- [ ] Bot components: buttons, select menus, modals
- [ ] Message context menu: copy link, copy ID, report
- [ ] Spell checking

### Media

- [x] Images, animated GIFs (Tenor and Giphy too), stickers, picture viewer
- [x] Video, audio and voice messages played in the app
- [x] YouTube inline through yt-dlp
- [x] Link previews and bot embeds with fields, images, thumbnails and footers
- [x] Sending files by picker or drag and drop, with spoilers
- [x] GIF and sticker pickers
- [x] Avatars and server icons
- [x] Recording and sending voice messages, from the desktop too (Discord's own app only records on the phone)

### Channels

- [x] Text channels and announcement channels
- [ ] Threads: list, open, create, reply
- [ ] Forum and media channels
- [ ] Stage channels
- [ ] Voice channels: joining, speaking, mute and deafen that reach Discord
- [ ] Video calls and screen sharing
- [ ] Direct message calls
- [ ] Scheduled events

### Servers

- [x] Server menu: invite, create channel and category, server nickname, leave
- [x] Server settings: overview, roles (name and colour), emoji, members (kick and ban), invites, bans, audit log, deleting the server
- [ ] Role permissions and channel permission overwrites
- [ ] Uploading server icons, banners, emoji and stickers
- [ ] Creating and joining servers from the rail
- [ ] Server discovery
- [ ] Onboarding and rules screening
- [ ] AutoMod and moderation tools (timeouts, bulk delete)

### Profile and settings

- [x] User settings, as Discord's
- [x] Profiles, notes, mutual servers and friends
- [x] Editing display name and bio
- [x] Light and dark themes, following the Omarchy theme
- [x] Zoom and accessibility switches
- [ ] Changing your status: online, idle, do not disturb, invisible
- [ ] Setting a custom status
- [ ] Avatar, banner and profile colour upload
- [ ] Per-server and per-channel notification settings
- [ ] Privacy and safety settings

### Desktop

- [x] Notifications for direct messages and mentions
- [x] Unread count in the window title
- [x] Tray, closing to the tray
- [x] One running copy
- [x] Starting at login
- [x] F12 performance overlay (FPS, frame time, RAM, CPU, threads)
- [x] Automatic releases for Linux, Windows and macOS
- [ ] Updating itself from the releases page
- [x] AppImage for Ubuntu and other Linux desktops
- [x] AUR package (`fastdisco-bin`)
- [ ] Flatpak
- [ ] Windows installer and macOS app bundle
- [ ] Rich presence (what game or music you are playing)

## License

MIT
