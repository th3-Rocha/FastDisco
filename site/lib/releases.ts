const REPO = "th3-Rocha/FastDisco";

export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases`;

export type Asset = { name: string; url: string; size: number };
export type Release = { tag: string; date: string; url: string; assets: Asset[] };

export type PlatformId = "windows" | "appimage" | "linux" | "macos";
export type Os = "windows" | "linux" | "macos";

export type Platform = {
  id: PlatformId;
  os: Os;
  label: string;
  detail: string;
  match: RegExp;
};

/** One entry per file the release workflow publishes, in download order. */
export const PLATFORMS: Platform[] = [
  { id: "windows", os: "windows", label: "Windows", detail: "x86_64 · unzip and run", match: /windows-msvc\.zip$/ },
  { id: "appimage", os: "linux", label: "Linux AppImage", detail: "Ubuntu 24.04+ and most desktops", match: /\.AppImage$/ },
  { id: "linux", os: "linux", label: "Linux .tar.gz", detail: "binary, launcher entry and icons", match: /unknown-linux-gnu\.tar\.gz$/ },
  { id: "macos", os: "macos", label: "macOS", detail: "Apple Silicon", match: /apple-darwin\.tar\.gz$/ },
];

export type Download = Omit<Platform, "match"> & { url: string; size: number; file: string };

type GhRelease = {
  tag_name: string;
  published_at: string;
  html_url: string;
  draft: boolean;
  assets: { name: string; browser_download_url: string; size: number }[];
};

async function gh<T>(path: string): Promise<T | null> {
  const headers: Record<string, string> = { Accept: "application/vnd.github+json" };
  // Optional: lifts the 60 requests an hour GitHub allows without one.
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}${path}`, {
      headers,
      next: { revalidate: 600 },
    });
    if (!res.ok) return null;
    return (await res.json()) as T;
  } catch {
    return null;
  }
}

function toRelease(r: GhRelease): Release {
  return {
    tag: r.tag_name,
    date: r.published_at,
    url: r.html_url,
    assets: r.assets.map((a) => ({ name: a.name, url: a.browser_download_url, size: a.size })),
  };
}

export async function latestRelease(): Promise<Release | null> {
  const r = await gh<GhRelease>("/releases/latest");
  return r ? toRelease(r) : null;
}

export async function allReleases(): Promise<Release[]> {
  const list = await gh<GhRelease[]>("/releases?per_page=30");
  return (list ?? []).filter((r) => !r.draft).map(toRelease);
}

export function downloadsOf(release: Release): Download[] {
  return PLATFORMS.flatMap(({ match, ...platform }) => {
    const asset = release.assets.find((a) => match.test(a.name));
    return asset ? [{ ...platform, url: asset.url, size: asset.size, file: asset.name }] : [];
  });
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-GB", { day: "numeric", month: "short", year: "numeric" });
}
