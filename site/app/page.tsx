import Link from "next/link";
import { DownloadButtons } from "@/components/DownloadButtons";
import { Footer, Nav } from "@/components/Nav";
import { Snout } from "@/components/Snout";
import { REPO_URL, RELEASES_URL, downloadsOf, latestRelease } from "@/lib/releases";

export const revalidate = 600;

// Resident memory with a few servers open, in MB.
const FASTDISCO_RAM = 170;
const DISCORD_RAM = 600;

const FEATURES = [
  { icon: "⚡", title: "Lightweight", text: `Opens in a blink and sits around ${FASTDISCO_RAM} MB of RAM. No browser engine inside.` },
  { icon: "🦀", title: "Native, in Rust", text: "Built with egui and fastframe, the framework behind Spotifast and ZapFast. No Electron." },
  { icon: "🧭", title: "The layout you know", text: "Server rail, channels, chat, member list and user panel, each button where Discord puts it." },
  { icon: "🔊", title: "Voice channels", text: "Talk and listen with noise suppression, echo cancellation and Discord's DAVE end-to-end encryption." },
  { icon: "💬", title: "Everything in chat", text: "Markdown, replies, reactions, polls, forwards, slash commands, bot buttons and modals." },
  { icon: "🖼️", title: "Media inline", text: "Images, GIFs, stickers, video, voice messages and YouTube, played right in the app." },
  { icon: "🎤", title: "Voice messages from the desktop", text: "Record and send them from your computer, which Discord's own app only lets you do on the phone." },
  { icon: "🔄", title: "Updates itself", text: "Checks GitHub for a new version, downloads it in the background and rolls back one that fails." },
  { icon: "⌨️", title: "Keyboard first", text: "Quick switcher on Ctrl+K, Alt+Up and Alt+Down between channels, and Discord's other shortcuts." },
];

export default async function Home() {
  const release = await latestRelease();
  const downloads = release ? downloadsOf(release) : [];

  return (
    <>
      <Nav version={release?.tag} />
      <main>
        <section className="hero wrap">
          <div className="hero-copy">
            <p className="eyebrow">
              <Snout size={18} /> Open source · Linux · Windows
            </p>
            <h1>
              Discord,
              <br />
              <span className="grad">native and fast.</span>
            </h1>
            <p className="lead">
              A lightweight Discord app written in Rust. Servers, DMs, voice channels and media, in about{" "}
              <strong>{FASTDISCO_RAM} MB of RAM</strong>.
            </p>
            <div className="cta">
              <a href="#download" className="btn btn-primary">
                Download {release?.tag}
              </a>
              <a href={REPO_URL} className="btn">
                GitHub
              </a>
            </div>
          </div>
          <AppMock />
        </section>

        <section className="wrap ram">
          <h2>Less memory, same Discord</h2>
          <div className="bars">
            <Bar label="FastDisco" mb={FASTDISCO_RAM} max={DISCORD_RAM} accent />
            <Bar label="Discord desktop (Electron)" mb={DISCORD_RAM} max={DISCORD_RAM} prefix="~" />
          </div>
          <p className="muted small">
            Resident memory with a few servers open. Discord&apos;s figure adds up its Electron processes and varies
            from machine to machine.
          </p>
        </section>

        <section className="wrap">
          <div className="grid">
            {FEATURES.map((f) => (
              <article key={f.title} className="card">
                <span className="card-icon">{f.icon}</span>
                <h3>{f.title}</h3>
                <p>{f.text}</p>
              </article>
            ))}
          </div>
        </section>

        <section id="download" className="wrap download">
          <h2>Download</h2>
          <p className="muted">
            {release ? (
              <>
                Version <strong>{release.tag}</strong>, straight from{" "}
                <a href={release.url}>the GitHub release</a>.
              </>
            ) : (
              <>
                Get the latest build from <a href={`${RELEASES_URL}/latest`}>the GitHub releases page</a>.
              </>
            )}
          </p>
          {downloads.length > 0 && <DownloadButtons downloads={downloads} />}
          <div className="extra">
            <div>
              <h4>Arch Linux</h4>
              <code>yay -S fastdisco-bin</code>
            </div>
            <div>
              <h4>AppImage</h4>
              <code>chmod +x fastdisco-*.AppImage && ./fastdisco-*.AppImage</code>
            </div>
          </div>
          <p className="muted small">
            Looking for an older version? <Link href="/download">Every release</Link> is listed with its files.
          </p>
        </section>

        <section className="wrap warn">
          <p>
            <strong>Heads up:</strong> FastDisco is in early development. Sign in with the QR code from the Discord
            phone app; email and password often trigger an hCaptcha that FastDisco cannot show yet.
          </p>
        </section>
      </main>
      <Footer />
    </>
  );
}

function Bar({ label, mb, max, accent, prefix = "" }: { label: string; mb: number; max: number; accent?: boolean; prefix?: string }) {
  return (
    <div className="bar">
      <div className="bar-head">
        <span>{label}</span>
        <strong>
          {prefix}
          {mb} MB
        </strong>
      </div>
      <div className="bar-track">
        <div className={`bar-fill ${accent ? "bar-accent" : ""}`} style={{ width: `${(mb / max) * 100}%` }} />
      </div>
    </div>
  );
}

/** A drawing of the app's window: rail, channels, chat and members. */
function AppMock() {
  return (
    <div className="mock" aria-hidden="true">
      <div className="mock-rail">
        <div className="mock-home">
          <Snout size={26} />
        </div>
        <i className="sep" />
        {["#f7709c", "#5865f2", "#23a55a", "#f0b232", "#eb459e"].map((c, i) => (
          <i key={c} className={`srv ${i === 1 ? "on" : ""}`} style={{ background: c }} />
        ))}
      </div>
      <div className="mock-channels">
        <b className="mock-title" />
        {["general", "rust", "memes", "voice-chat"].map((ch, i) => (
          <span key={ch} className={i === 1 ? "on" : ""}>
            {i === 3 ? "🔊" : "#"} {ch}
          </span>
        ))}
        <div className="mock-user">
          <i className="av" style={{ background: "#f7709c" }} />
          <b />
        </div>
      </div>
      <div className="mock-chat">
        {[
          ["#5865f2", 70, 45],
          ["#23a55a", 55, 0],
          ["#f7709c", 80, 60],
          ["#f0b232", 40, 0],
        ].map(([c, w1, w2], i) => (
          <div key={i} className="msg">
            <i className="av" style={{ background: c as string }} />
            <div>
              <b style={{ width: 60 + i * 12 }} />
              <s style={{ width: `${w1}%` }} />
              {Number(w2) > 0 && <s style={{ width: `${w2}%` }} />}
            </div>
          </div>
        ))}
        <div className="mock-input">Message #rust</div>
      </div>
      <div className="mock-ram">
        <span className="dot" /> {FASTDISCO_RAM} MB
      </div>
    </div>
  );
}
