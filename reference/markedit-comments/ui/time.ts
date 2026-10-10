const units: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
];

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });

/** "3 minutes ago", "yesterday", or the raw string when it is not a parsable date. */
export const relativeTime = (iso: string | undefined, now = Date.now()): string => {
  if (!iso) return '';
  const time = Date.parse(iso);
  if (Number.isNaN(time)) return iso;
  const seconds = Math.round((time - now) / 1000);
  if (Math.abs(seconds) < 60) return 'just now';
  for (const [unit, size] of units) {
    if (Math.abs(seconds) >= size) return rtf.format(Math.round(seconds / size), unit);
  }
  return 'just now';
};

export const absoluteTime = (iso: string | undefined): string => {
  if (!iso) return '';
  const time = Date.parse(iso);
  return Number.isNaN(time) ? iso : new Date(time).toLocaleString();
};
