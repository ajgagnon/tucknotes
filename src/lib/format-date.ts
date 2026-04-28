function ordinalSuffix(day: number): string {
  const mod100 = day % 100;
  if (mod100 >= 11 && mod100 <= 13) return "th";
  switch (day % 10) {
    case 1:
      return "st";
    case 2:
      return "nd";
    case 3:
      return "rd";
    default:
      return "th";
  }
}

const monthShortFormatter = new Intl.DateTimeFormat(undefined, {
  month: "short",
});
const weekdayShortFormatter = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
});

/** `"Apr 23rd"` */
export function formatMonthDayOrdinal(ms: number): string {
  const d = new Date(ms);
  const day = d.getDate();
  return `${monthShortFormatter.format(d)} ${day}${ordinalSuffix(day)}`;
}

/** `"Thu, Apr 23rd"` */
export function formatWeekdayMonthDayOrdinal(ms: number): string {
  const d = new Date(ms);
  return `${weekdayShortFormatter.format(d)}, ${formatMonthDayOrdinal(ms)}`;
}

/** `"9:30am"` — lowercase am/pm, no leading zero on hour. */
export function formatClockTime(ms: number): string {
  const d = new Date(ms);
  const hours24 = d.getHours();
  const minutes = d.getMinutes();
  const period = hours24 >= 12 ? "pm" : "am";
  const hours12 = hours24 % 12 === 0 ? 12 : hours24 % 12;
  return `${hours12}:${String(minutes).padStart(2, "0")}${period}`;
}

/** Compact duration: `"45s"` (<1m), `"34m"` (<1h), `"1h 5m"`. */
export function formatDurationShort(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) return `${totalMinutes}m`;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return minutes === 0 ? `${hours}h` : `${hours}h ${minutes}m`;
}
