import type { Metadata } from "next";
import { PlatformIcon } from "@/components/DownloadButtons";
import { Footer, Nav } from "@/components/Nav";
import { RELEASES_URL, allReleases, downloadsOf, formatDate, formatSize } from "@/lib/releases";

export const revalidate = 600;

export const metadata: Metadata = { title: "Download FastDisco" };

export default async function DownloadPage() {
  const releases = await allReleases();

  return (
    <>
      <Nav version={releases[0]?.tag} />
      <main className="wrap releases">
        <h1>Every release</h1>
        <p className="muted">
          Direct links to each version&apos;s files on GitHub. The newest is on top; installed copies update
          themselves to it.
        </p>
        {releases.length === 0 && (
          <p>
            Could not reach GitHub right now. See <a href={RELEASES_URL}>the releases page</a>.
          </p>
        )}
        {releases.map((r, i) => (
          <section key={r.tag} className="release">
            <header>
              <h2>
                <a href={r.url}>{r.tag}</a>
                {i === 0 && <span className="pill">latest</span>}
              </h2>
              <span className="muted">{formatDate(r.date)}</span>
            </header>
            <ul>
              {downloadsOf(r).map((d) => (
                <li key={d.id}>
                  <a href={d.url}>
                    <PlatformIcon id={d.id} />
                    <span>{d.label}</span>
                    <code>{d.file}</code>
                    <small>{formatSize(d.size)}</small>
                  </a>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </main>
      <Footer />
    </>
  );
}
