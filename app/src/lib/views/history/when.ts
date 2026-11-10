// Date and time wording for the History view. Times from the device are Unix ms; they are shown
// in the computer's local time zone.

const day = new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" });
const hm = new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit" });

export const dayLabel = (ms: number) => day.format(new Date(ms));
export const timeLabel = (ms: number) => hm.format(new Date(ms));

/** "Wed 8 Nov 2023, 21:04 to 22:30" or, within one minute, "Wed 8 Nov 2023, 21:04". */
export function sessionLabel(start: number, end: number): string {
  const a = timeLabel(start);
  const b = timeLabel(end);
  return a === b ? `${dayLabel(start)}, ${a}` : `${dayLabel(start)}, ${a} to ${b}`;
}

/** "3 days ago" style, for the Last played column; the exact date is the cell's tooltip. */
export function ago(ms: number, now = Date.now()): string {
  if (ms <= 0) return "never";
  const mins = Math.max(0, Math.round((now - ms) / 60000));
  if (mins < 60) return mins <= 1 ? "just now" : `${mins} minutes ago`;
  const hours = Math.round(mins / 60);
  if (hours < 48) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
  const days = Math.round(hours / 24);
  if (days < 60) return `${days} days ago`;
  return dayLabel(ms);
}
