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
  `fastdisco-<version>-x86_64.AppImage`, then
  `chmod +x fastdisco-*.AppImage` and run it. Needs Ubuntu 24.04 or newer.
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

## Website

`site/` is the Next.js website, deployed on Vercel with `site` as the
project's root directory. Its download links come from the GitHub
releases API, refreshed every ten minutes, so a new release shows up
without touching the site. A `GITHUB_TOKEN` environment variable is
optional and only raises GitHub's rate limit.

```sh
cd site && npm install && npm run dev
```

## Updates

FastDisco checks GitHub for a newer release when it opens and once a
day, downloads it in the background, and shows a green arrow in the
header when it is ready; clicking it restarts into the new version, and
a version that does not start is rolled back. Only copies that can
replace themselves do this: the AppImage and the unpacked Linux and
Windows archives. The AUR package updates through `yay`, and copies
built from source are left alone. Each release's `checksums.txt` is
signed with the key in `assets/update-public-key.hex`.

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
- [x] Server folders in the rail
- [x] Dragging servers and channels to reorder them
- [x] Quick switcher (Ctrl+K)
- [x] The rest of Discord's keyboard shortcuts
- [x] Friends page: online, all, pending, blocked, add friend
- [x] Inbox: recent mentions and unreads
- [x] Search messages
- [x] Pinned messages panel
- [x] Mark as unread, mark server as read from the rail menu

### Messages

- [x] History, live messages, edits and deletions, older history on scroll
- [x] Sending with optimistic display
- [x] Direct messages and groups
- [x] Markdown: bold, italic, underline, strikethrough, code, code blocks, quotes, headings, lists, subtext, spoilers, masked links, mentions, timestamps
- [x] Custom and Unicode emoji, jumbo emoji, emoji picker, `:shortcode:` on send
- [x] Replies, edits, deletion, reactions, typing indicator
- [x] Discord's limits: character counter, "message too long" (send as file), upload size by plan
- [x] Forwarding messages
- [x] Polls: voting, results, making one
- [x] Slash commands and app commands (attachment options not yet)
- [x] Bot components: buttons, select menus, modals, and the newer layout components
- [x] Message context menu: forward, pin, apps, copy link, copy ID, report (opens Discord's report form)
- [x] Spell checking (Portuguese and English, dictionaries downloaded once)

### Media

- [x] Images, animated GIFs (Tenor and Giphy too), stickers, picture viewer
- [x] Video, audio and voice messages played in the app
- [x] YouTube inline through yt-dlp
- [x] Link previews and bot embeds with fields, images, thumbnails and footers
- [x] Sending files by picker, drag and drop or Ctrl+V of a picture or copied files, with spoilers
- [x] GIF and sticker pickers
- [x] Avatars and server icons
- [x] Recording and sending voice messages, from the desktop too (Discord's own app only records on the phone)

### Channels

- [x] Text channels and announcement channels
- [ ] Threads: list, open, create, reply
- [ ] Forum and media channels
- [ ] Stage channels
- [x] Voice channels: joining, speaking and hearing, mute and deafen, who is talking, with Discord's DAVE end-to-end encryption
- [x] Voice filter: noise suppression, echo cancellation, automatic gain and input sensitivity
- [x] Voice settings: microphone and speaker choice, their volumes, microphone test, input profiles and manual sensitivity
- [x] Age-restricted channels show Discord's notice when the account has not confirmed its age
- [x] Right-click on someone in a voice channel: their volume and a local mute, and for moderators server mute, server deafen, disconnect and move
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
- [x] The app's sounds: joining and leaving calls, people coming and going, mute, deafen, new messages
- [x] Unread count in the window title
- [x] Tray, closing to the tray
- [x] One running copy
- [x] Starting at login
- [x] F12 performance overlay (FPS, frame time, RAM, CPU, threads)
- [x] Automatic releases for Linux, Windows and macOS
- [x] Updating itself from the releases page (the AppImage, and the Linux and Windows archives; the AUR updates through yay)
- [x] AppImage for Ubuntu and other Linux desktops
- [x] AUR package (`fastdisco-bin`)
- [ ] Flatpak
- [ ] Windows installer and macOS app bundle
- [ ] Rich presence (what game or music you are playing)

## License

MIT
