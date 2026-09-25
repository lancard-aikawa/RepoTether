// 日付・時刻の表示。集計の単位はすべてローカル時刻の日付。

const WEEKDAYS = ["日", "月", "火", "水", "木", "金", "土"];

export function toMs(s: string | null | undefined): number | null {
  if (!s) return null;
  const t = Date.parse(s);
  return Number.isNaN(t) ? null : t;
}

/** ローカル時刻の "YYYY-MM-DD" */
export function dayKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function parseDayKey(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d);
}

/** その日を含む週の月曜 0:00 (ローカル) */
export function weekStart(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  const offset = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - offset);
  return d.getTime();
}

export function startOfDay(ms: number): number {
  const d = new Date(ms);
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function addDays(ms: number, days: number): number {
  const d = new Date(ms);
  d.setDate(d.getDate() + days);
  return d.getTime();
}

export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function formatTime(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** "9/25 (木)" */
export function formatDayLabel(key: string): string {
  const d = parseDayKey(key);
  return `${d.getMonth() + 1}/${d.getDate()} (${WEEKDAYS[d.getDay()]})`;
}

/** "2026-09-25 (木)" */
export function formatDayLong(key: string): string {
  const d = parseDayKey(key);
  return `${key} (${WEEKDAYS[d.getDay()]})`;
}

export function formatDateTime(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${formatTime(ms)}`;
}

/** "3 分前" / "2 日前" / "4 か月前" */
export function relative(ms: number | null, now = Date.now()): string {
  if (ms == null) return "-";
  const diff = now - ms;
  if (diff < 0) return "たった今";
  const min = diff / 60000;
  if (min < 1) return "たった今";
  if (min < 60) return `${Math.floor(min)} 分前`;
  const h = min / 60;
  if (h < 24) return `${Math.floor(h)} 時間前`;
  const d = h / 24;
  if (d < 31) return `${Math.floor(d)} 日前`;
  const mo = d / 30.44;
  if (mo < 12) return `${Math.floor(mo)} か月前`;
  return `${Math.floor(mo / 12)} 年前`;
}

export function daysSince(ms: number | null, now = Date.now()): number | null {
  return ms == null ? null : Math.floor((now - ms) / 86400000);
}

export function formatDuration(ms: number): string {
  const min = Math.round(ms / 60000);
  if (min < 60) return `${min} 分`;
  const h = Math.floor(min / 60);
  const m = min % 60;
  return m ? `${h} 時間 ${m} 分` : `${h} 時間`;
}
