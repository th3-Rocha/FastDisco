"use client";

import { useEffect, useState } from "react";
import { type Download, type Os, type PlatformId, formatSize } from "@/lib/releases";

function detectOs(): Os | null {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "macos";
  if (/Linux|X11/i.test(ua) && !/Android/i.test(ua)) return "linux";
  return null;
}

const ICONS: Record<PlatformId, React.ReactNode> = {
  windows: (
    <path d="M3 5.5 10.5 4.4v7.1H3zm8.5-1.2L21 3v8.5h-9.5zM3 12.5h7.5v7.1L3 18.5zm8.5 0H21V21l-9.5-1.3z" fill="currentColor" />
  ),
  appimage: (
    <path d="M12 2.5 20.5 7v10L12 21.5 3.5 17V7zm0 2.3L6 8l6 3.2L18 8zM5.5 9.7v6.1l5.5 2.9v-6.1zm13 0L13 12.6v6.1l5.5-2.9z" fill="currentColor" />
  ),
  linux: (
    <path d="M4 4h16v4H4zm1 5h14v11H5zm4 3v2h6v-2z" fill="currentColor" />
  ),
  macos: (
    <path d="M4 5h16v11H4zm-2 12h20l-1.5 2.5h-17z" fill="currentColor" />
  ),
};

export function PlatformIcon({ id }: { id: PlatformId }) {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden="true">
      {ICONS[id]}
    </svg>
  );
}

export function DownloadButtons({ downloads }: { downloads: Download[] }) {
  const [os, setOs] = useState<Os | null>(null);
  useEffect(() => setOs(detectOs()), []);

  // The visitor's own system goes first and gets the bright button.
  const sorted = os ? [...downloads].sort((a, b) => Number(b.os === os) - Number(a.os === os)) : downloads;
  const primary = os ? sorted.find((d) => d.os === os) : undefined;

  return (
    <div className="downloads">
      {sorted.map((d) => (
        <a key={d.id} href={d.url} className={`dl ${d === primary ? "dl-primary" : ""}`}>
          <PlatformIcon id={d.id} />
          <span className="dl-text">
            <strong>{d.label}</strong>
            <small>
              {d.detail} · {formatSize(d.size)}
            </small>
          </span>
          <span className="dl-arrow" aria-hidden="true">↓</span>
        </a>
      ))}
    </div>
  );
}
