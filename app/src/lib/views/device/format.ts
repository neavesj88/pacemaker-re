// Size and count formatting for the device view.
export function bytes(n: number): string {
  if (n >= 2 ** 30) return `${(n / 2 ** 30).toFixed(1)} GB`;
  if (n >= 2 ** 20) return `${(n / 2 ** 20).toFixed(1)} MB`;
  if (n >= 2 ** 10) return `${(n / 2 ** 10).toFixed(1)} KB`;
  return `${n} B`;
}

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
