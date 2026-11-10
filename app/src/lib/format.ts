// Small pure formatters shared by every view.

export const mmss = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
export const baseName = (p: string) => p.split(/[\\/]/).pop() || p;
export const volumeName = (root: string) => root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || root;
