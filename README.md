<h1 align="center">Discofast</h1>

<p align="center"><strong>Discord, native and fast.</strong><br>A lightweight Discord app for Linux, macOS, and Windows.</p>

Discofast is a Discord client written in Rust with [egui](https://www.egui.rs)
and [fastframe](https://fastframe.dev), the framework behind Spotifast and
ZapFast. No browser engine, no Electron.

The layout follows Discord's desktop client on purpose: the server rail on
the left, channels beside it with your user panel at the foot, the
conversation in the middle and the member list on the right. Every button
sits where Discord users expect it.

> **Renamed:** Discofast was called FastDisco up to 0.9. Installed
> copies update onto the new name by themselves and keep their settings
> and login; on Arch, `fastdisco-bin` now pulls in `discofast-bin`.

> **Status:** early development. Login, servers, channels, direct messages
> and chatting work against Discord.

Signing in with email and password from outside a browser usually makes
Discord ask for an hCaptcha, which Discofast cannot show. Use the QR code
with the Discord phone app instead.

> **Warning:** Discord's Terms of Service do not allow third-party clients
> that log in with a user account. Using Discofast may put your account at
> risk. Use it at your own discretion.

## Download

Every version is built for Linux, Windows and macOS and published on the
[releases page](https://github.com/th3-Rocha/FastDisco/releases/latest).

- **Arch Linux**: `yay -S discofast-bin` (or any other AUR helper).
- **Ubuntu and other Linux desktops**: download
  `discofast-<version>-x86_64.AppImage`, then
  `chmod +x discofast-*.AppImage` and run it. Needs Ubuntu 24.04 or newer.
- **Linux, by hand**: the `x86_64-unknown-linux-gnu` archive has the
  binary, the launcher entry and the icons.
- **Flatpak**: download `discofast-<version>-x86_64.flatpak`, then
  `flatpak install --user discofast-*.flatpak`.
- **Windows** (x86_64): run `discofast-<version>-x86_64-setup.exe`, or
  unpack the archive and run `discofast`.
- **macOS** (Apple Silicon): open `discofast-<version>-aarch64.dmg` and
  drag Discofast to Applications. It is signed ad hoc, so the first time
  open it with a right click, Abrir.

## Build

Requires Rust 1.98+. Video, audio and YouTube playback use `ffmpeg`,
`ffprobe` and `yt-dlp` when they are installed; without them the media
opens in the browser.

```sh
cargo run
```

Two optional features: `video` watches Discord screen shares (not yet
tried against Discord), and `selfhosted` sends camera and screen between
friends through a LiveKit server of your own; `contrib/livekit` has the
server and how to set it up. The released builds have `selfhosted`;
`cargo run --features selfhosted` builds it from source.

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

Discofast checks GitHub for a newer release when it opens and once a
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

What works today, and what is still missing for Discofast to do
everything Discord's desktop app does.

### Account and connection

- [x] Login with the QR code from the phone app
- [x] Login with email and password, with a two-factor code
- [x] Login with a token; the token is kept in the OS keyring
- [x] Gateway with heartbeat, resume and reconnect
- [ ] hCaptcha when Discord asks for one at login
- [x] Switching between several accounts: each kept in the keyring, listed under "Mudar de conta" and on the login screen

### Layout and navigation

- [x] Discord's layout: server rail, channels, chat, member list, user panel
- [x] Discord's title bar: back and forward (Alt+←/→), the place in the middle, inbox and help
- [x] Resizable channel list, remembered between runs
- [x] Soft, low, melodic notification and call sounds
- [x] Unread dots, mention badges, collapsible categories
- [x] Rail tooltips, the pill beside the open server, who is in a call on each server
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
- [x] Threads: list, open, create, reply, follow and leave, close and lock; followed threads under their channel
- [x] Forum and media channels: posts as cards or a picture grid, tags to filter by, new posts with tags
- [x] Stage channels: speakers and audience, raising a hand, bringing people up and down, starting and ending the stage with its topic
- [x] Voice channels: joining, speaking and hearing, mute and deafen, who is talking, with Discord's DAVE end-to-end encryption
- [x] Voice filter: DeepFilterNet3 neural noise suppression (or WebRTC's), echo cancellation, automatic gain and input sensitivity
- [x] Received voice as Discord's own client recovers it: a lost packet is rebuilt from the next packet's Opus FEC, and a talker whose stream runs dry is filled in for a moment instead of cut
- [x] Call audio without clicks or drift: lock-free rings, a playback buffer that deepens after sound comes late (60 to 180 ms), clock drift correction both ways, exact packet loss concealment, Opus at 64 kbps mono, filter in WebRTC's order (echo, noise, gain)
- [x] Call audio diagnostics: `DISCOFAST_AUDIO_DEBUG=<folder>` records the raw microphone, the filtered sound and what was received as WAVs, and every ten seconds the packet loss, how late the sound arrived and the buffers' health go to the log and to `~/.cache/discofast/call.log`, with where received packets went missing (late, too far ahead, playout running dry, DAVE failing); `cargo test --release realtime -- --ignored --nocapture` runs the send and receive paths in real time and counts the breaks
- [x] Voice settings: microphone and speaker choice, their volumes, microphone test (heard through the voice filter, so the noise suppression can be switched while listening), input profiles and manual sensitivity
- [x] Age-restricted channels show Discord's notice when the account has not confirmed its age
- [x] Right-click on someone in a voice channel: their volume and a local mute, and for moderators server mute, server deafen, disconnect and move
- [x] Voice channel rows as Discord's: chat, invite and settings buttons, the channel status (and setting it), "Convidar para voz", and the voice channel's text chat
- [x] The call's real duration from Discord, Discord's filled speaker, "Convidar para voz" counting down
- [x] A call taken over by another device: the call ends here and Discord's "Reconectar" bar shows
- [x] Server events: the "Eventos" row and the calendar badge on the rail; rounded-square server icons with the call badge at the top
- [x] Discord's call bar: microphone and its menu, camera, activities, soundboard, more, the red hang-up
- [x] Your own profile card: "No que você está pensando?" (custom status), the call you are in, Editar perfil, status (Disponível, Ausente, Não perturbar, Invisível), Mudar de conta
- [x] Discord's invite dialog: search friends, send the invite to each, copy the link
- [x] Groups show their own picture
- [x] "Criar seu servidor": make your own or start from Discord's templates, the audience question, the name and icon
- [x] "Entrar em um servidor" with an invite (the browser takes over when Discord asks for a captcha)
- [x] Invite links as Discord's cards: the server with its online and member counts and "Entrar", or for a voice channel the call itself, with who is in it, how long it has gone on and "Entrar na chamada de voz"
- [x] The rail's grey add-server tile
- [x] The microphone's and headphones' arrow menus: device, input profile, volume, voice settings
- [x] Account settings as Discord's: account info, password and security (multi-factor, connected devices with sign-out), account status (age group), Family Center, disable and delete the account
- [x] Data and privacy (data consents, requesting your data, activity sharing), message permissions (age-restricted DM commands and iOS servers, DMs from server members, who can send friend requests, blocked accounts with unblock), notifications (sounds, Discord's e-mails, mobile push delay, /tts); each page lists its parts in the side menu
- [x] On a big screen the settings open in a window over the dimmed app, with the page's name and the close button in a bar, as Discord's
- [x] The call panel's noise button: its icon follows the setting, and it opens Discord's card with the switch and a microphone test
- [ ] Video calls and screen sharing
- [x] Who has their camera on and who is live, in the channel list and the call
- [ ] Watching a screen share: written behind the `video` feature (`cargo run --features video`), not yet tried against Discord
- [x] Camera and screen sharing between friends on your own LiveKit server, beside the Discord call (the `selfhosted` feature, in the released builds; see `contrib/livekit`)
- [ ] Video calls and screen sharing through Discord itself: cameras in the call and sending your own camera or screen
- [x] Direct message calls: calling from the header, the ring, answering or turning it down
- [x] Scheduled events: the list with when and where, "Tenho interesse", creating them in a voice channel or elsewhere, starting, ending and cancelling

### Servers

- [x] Server menu: invite, create channel and category, server nickname, leave
- [x] Server settings: overview, roles (name and colour), emoji, members (kick and ban), invites, bans, audit log, deleting the server
- [x] Role permissions (creating and deleting roles too) and a channel's settings with its permission overwrites
- [x] Uploading server icons, banners, emoji and stickers
- [x] Creating and joining servers from the rail
- [x] Server discovery: "Explorar" in the rail, public servers by category, joining them
- [x] Onboarding and rules screening: a new server's rules to agree to, then its questions
- [x] AutoMod rules (list, switch, delete, block words) and moderation: timeouts, bans that delete recent messages

### Profile and settings

- [x] User settings, as Discord's
- [x] Profiles, notes, mutual servers and friends
- [x] Editing display name and bio
- [x] Light and dark themes, following the Omarchy theme; the dark one is Discord's grey theme from its 2025 redesign, with its 40 px server icons
- [x] Colour themes, free: 22 gradients behind see-through surfaces, and "Personalize o seu tema" (dark or light, up to five colours on a saturation square and hue bar, intensity, "Surpreenda-me!"), kept in settings.json
- [x] Discord's redesigned message box: "+" (file, poll, voice message, apps), gift, emoji (with the GIF and sticker tabs) and apps; today's messages show only the time
- [x] Zoom and accessibility switches
- [x] Changing your status: online, idle, do not disturb, invisible
- [x] Setting a custom status
- [x] Avatar, banner and profile colour upload
- [x] Per-server and per-channel notification settings: muting for a while, every message, mentions or nothing, suppressing @everyone and roles; muted channels dimmed
- [x] Privacy and safety settings

### Desktop

- [x] Notifications for direct messages and mentions, with the author's picture
- [x] The app's sounds: joining and leaving calls, people coming and going, mute, deafen, new messages
- [x] Unread count in the window title
- [x] Tray, closing to the tray
- [x] One running copy
- [x] Starting at login
- [x] F12 performance overlay (FPS, frame time, RAM, CPU, threads)
- [x] Automatic releases for Linux, Windows and macOS
- [x] Updating itself from the releases page (the AppImage, and the Linux and Windows archives; the AUR updates through yay)
- [x] AppImage for Ubuntu and other Linux desktops
- [x] AUR package (`discofast-bin`)
- [x] Flatpak (`discofast-<version>-x86_64.flatpak` on each release; `packaging/flatpak` has the manifest)
- [x] Windows installer (`-setup.exe`, for the current user, updates itself) and macOS app bundle (`Discofast.app` on a `.dmg`)
- [x] Rich presence: Discord's local RPC socket for games and apps (music players, editors), and running games recognised from Discord's list (Linux); follows "Compartilhar minha atividade"

## License

MIT
