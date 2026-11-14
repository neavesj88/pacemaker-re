// Small pure formatters shared by every view.

/** 215 -> "3:35", 3725 -> "1:02:05": one length format in every view, hours only when there are some. */
export function mmss(s: number): string {
  const two = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${two(m)}:${two(s % 60)}` : `${m}:${two(s % 60)}`;
}
export const baseName = (p: string) => p.split(/[\\/]/).pop() || p;
export const volumeName = (root: string) => root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root;
