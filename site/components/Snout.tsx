/** The Discofast mark: a pig's snout, the app's own icon (assets/icon.svg). */
export function Snout({ size = 32 }: { size?: number }) {
  // eslint-disable-next-line @next/next/no-img-element
  return <img src="/snout.svg" width={size} height={size} alt="" aria-hidden="true" />;
}
