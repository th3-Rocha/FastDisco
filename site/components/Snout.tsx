/** The Discofast mark: a pig's snout. Same drawing as app/icon.svg. */
export function Snout({ size = 32 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <ellipse cx="32" cy="33" rx="28" ry="22" fill="#f7709c" />
      <ellipse cx="32" cy="31" rx="25" ry="19" fill="#ffa3c2" />
      <ellipse cx="25" cy="22" rx="11" ry="4" fill="#ffd1e0" opacity=".8" />
      <ellipse cx="22" cy="33" rx="5.5" ry="8.5" fill="#7a2343" transform="rotate(-8 22 33)" />
      <ellipse cx="42" cy="33" rx="5.5" ry="8.5" fill="#7a2343" transform="rotate(8 42 33)" />
    </svg>
  );
}
