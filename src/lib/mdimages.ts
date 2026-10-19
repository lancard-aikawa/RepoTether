// Markdown の中の画像を、読み込むか・押すまで待つかを決める。
//
// README や会話の全文は人が書いた (書かせた) ものなので、画像の URL は相手が決められる。
// 開いただけで読み込むと、相手のサーバーに「いつ・どこから見たか」が伝わり、
// URL に文字を載せた画像 (![](https://example.com/?d=...)) なら、その文字まで外へ出る。
// だから外の画像は、押すまで読み込まない。

export type ImagePlan =
  /** そのまま出す */
  | { kind: "show"; url: string }
  /** 押すまで読み込まない。host は札に出す行き先 */
  | { kind: "ask"; url: string; host: string }
  /** 出せない (代替文字にする) */
  | { kind: "drop" };

const isHttp = (u: string) => /^https?:\/\//i.test(u);

/**
 * src: Markdown に書かれていた画像の場所。
 * resolve: 相対パスを、そのリポジトリ自身の URL にする (できなければ null)。
 * allowed: 利用者が「画像を表示」を押したあとか。
 */
export function imagePlan(src: string, resolve: (rel: string) => string | null, allowed: boolean): ImagePlan {
  const s = src.trim();
  if (!isHttp(s)) {
    // スキームのあるもの (data: / file: / javascript: など) と、// で始まるものは相対パスではない
    if (!s || /^[a-z][a-z0-9+.-]*:/i.test(s) || s.startsWith("//")) return { kind: "drop" };
    // 相対パスは、そのリポジトリ自身のファイル。行き先は書いた人ではなく RepoTether が決めるので、そのまま出す
    const url = resolve(s);
    return url && isHttp(url) ? { kind: "show", url } : { kind: "drop" };
  }
  if (allowed) return { kind: "show", url: s };
  let host: string;
  try {
    host = new URL(s).host;
  } catch {
    return { kind: "drop" };
  }
  return { kind: "ask", url: s, host };
}
