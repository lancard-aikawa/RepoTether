// 関連ページ (プロジェクトごとに登録する、サービスの管理画面などの URL)。

/** 入力を登録できる URL にする (http / https だけ。スキームが無ければ https を補う)。だめなら null */
export function normalizeUrl(raw: string): string | null {
  const s = raw.trim();
  if (!s || /\s/.test(s)) return null;
  const withScheme = /^[a-z][a-z0-9+.-]*:/i.test(s) ? s : `https://${s}`;
  try {
    const u = new URL(withScheme);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    // "localhost" のほかは、ドットの無い名前 (打ちかけの語など) を URL とみなさない
    if (!u.hostname.includes(".") && u.hostname !== "localhost") return null;
    return u.href;
  } catch {
    return null;
  }
}

/** URL のホスト名 (名前が決まらないときの代わり) */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

/** ログイン画面のタイトルらしいか (管理画面は、ログインせずに取るとこれが返る) */
function looksLikeLogin(title: string): boolean {
  return /\b(sign[\s-]?in|log[\s-]?in|sign[\s-]?up|login|signin)\b|ログイン|サインイン/i.test(title);
}

/** 取ってきたタイトルから、名前の候補を決める。空やログイン画面のものならホスト名にする */
export function titleCandidate(fetched: string, url: string): string {
  const t = fetched.trim();
  return !t || looksLikeLogin(t) ? hostOf(url) : t;
}
