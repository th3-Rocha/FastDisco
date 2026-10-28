import type { Metadata } from "next";
import { Bricolage_Grotesque, Inter } from "next/font/google";
import "./globals.css";

const display = Bricolage_Grotesque({ subsets: ["latin"], variable: "--font-display" });
const body = Inter({ subsets: ["latin"], variable: "--font-body" });

export const metadata: Metadata = {
  title: "Discofast: Discord, native and fast",
  description:
    "A lightweight Discord app for Linux and Windows, written in Rust. No browser engine, no Electron, about 170 MB of RAM.",
  metadataBase: new URL("https://discofast.rocks"),
  openGraph: {
    title: "Discofast: Discord, native and fast",
    description: "A lightweight Discord app in Rust. No Electron, about 170 MB of RAM.",
  },
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={`${display.variable} ${body.variable}`}>
      <body>{children}</body>
    </html>
  );
}
