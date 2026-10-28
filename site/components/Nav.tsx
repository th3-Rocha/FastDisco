import Link from "next/link";
import { Snout } from "./Snout";
import { REPO_URL, RELEASES_URL } from "@/lib/releases";

export function Nav({ version }: { version?: string }) {
  return (
    <header className="nav">
      <div className="wrap nav-inner">
        <Link href="/" className="brand">
          <Snout size={30} />
          <span>FastDisco</span>
        </Link>
        <nav className="nav-links">
          <Link href="/download">Download</Link>
          {version && (
            <a href={RELEASES_URL} className="pill">
              {version}
            </a>
          )}
          <a href={REPO_URL} className="gh">
            GitHub
          </a>
        </nav>
      </div>
    </header>
  );
}

export function Footer() {
  return (
    <footer className="footer">
      <div className="wrap">
        <p>
          <Snout size={20} /> FastDisco is MIT-licensed and open source on{" "}
          <a href={REPO_URL}>GitHub</a>. Built with <a href="https://www.egui.rs">egui</a> and{" "}
          <a href="https://fastframe.dev">fastframe</a>.
        </p>
        <p className="muted">
          FastDisco is an independent project, not affiliated with or endorsed by Discord Inc. Discord is a trademark
          of Discord Inc. Discord&apos;s Terms of Service do not allow third-party clients that log in with a user
          account; use FastDisco at your own discretion.
        </p>
      </div>
    </footer>
  );
}
