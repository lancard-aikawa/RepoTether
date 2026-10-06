//! Web ページのタイトル (<title>) を取る。関連ページを登録するときの名前の候補に使う。
//!
//! ログインの要るページ (管理画面など) はログイン画面のタイトルが返るので、
//! 取れたものをそのまま使うかは画面の側で決める。

use std::path::Path;
use std::time::Duration;

/// 読むのは先頭のこれだけ (title は head にあるので、全部は読まない)
const MAX_BYTES: usize = 512 * 1024;

/// url のページのタイトル。http / https だけ。title が無ければ空文字
pub async fn fetch(url: &str) -> Result<String, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("http:// か https:// で始まる URL にしてください".into());
    }
    let mut resp = get_page(&client()?, url).await.map_err(|e| format!("ページを取れません: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ページを取れません (HTTP {})", resp.status().as_u16()));
    }
    let title = extract_title(&String::from_utf8_lossy(&read_capped(&mut resp).await?));
    // UTF-8 でないページ (Shift_JIS など) は化けるので、タイトルなしとして扱う
    Ok(if title.contains('\u{fffd}') { String::new() } else { title })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        // 既定の User-Agent だと、ブラウザ向けのページを返さないサイトがある
        .user_agent("Mozilla/5.0 (compatible; RepoTether)")
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
}

async fn get_page(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, reqwest::Error> {
    client
        .get(url)
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "ja,en;q=0.8")
        .send()
        .await
}

/// 本文を先頭の MAX_BYTES あたりまで読む
async fn read_capped(resp: &mut reqwest::Response) -> Result<Vec<u8>, String> {
    let mut buf: Vec<u8> = vec![];
    while buf.len() < MAX_BYTES {
        match resp.chunk().await.map_err(|e| e.to_string())? {
            Some(c) => buf.extend_from_slice(&c),
            None => break,
        }
    }
    Ok(buf)
}

/// url のサイトのアイコン (favicon) の中身。無い・取れないときは空。
/// サイト (スキーム・ホスト・ポート) ごとに cache_dir に残し、次からはネットワークにつながない
pub async fn icon(url: &str, cache_dir: &Path) -> Vec<u8> {
    let Ok(page) = reqwest::Url::parse(url.trim()) else { return vec![] };
    let (true, Some(host)) = (matches!(page.scheme(), "http" | "https"), page.host_str()) else { return vec![] };
    let name: String = format!("{}_{}_{}", page.scheme(), host, page.port_or_known_default().unwrap_or(0))
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    let file = cache_dir.join(name);
    if let Some(b) = std::fs::read(&file).ok().filter(|b| is_image(b)) {
        return b;
    }
    let Some(bytes) = fetch_icon(&page).await else { return vec![] };
    if std::fs::create_dir_all(cache_dir).is_ok() {
        let _ = std::fs::write(&file, &bytes);
    }
    bytes
}

async fn fetch_icon(page: &reqwest::Url) -> Option<Vec<u8>> {
    let client = client().ok()?;
    // 先にサイト直下の favicon.ico。ログインの要る管理画面でも、たいていここは取れる
    if let Some(b) = get_image(&client, page.join("/favicon.ico").ok()?).await {
        return Some(b);
    }
    // 無ければページの <link rel="icon">。別のホストのログイン画面へ飛ばされたときは、
    // そのサービスのアイコンではないので使わない
    if let Some(b) = linked_icon(&client, page).await {
        return Some(b);
    }
    // それも無ければ、1 つ上のドメインの favicon.ico (console.firebase.google.com → firebase.google.com)
    let host = page.host_str()?;
    let (_, parent) = host.split_once('.')?;
    if parent.matches('.').count() < 1 || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    get_image(&client, reqwest::Url::parse(&format!("https://{parent}/favicon.ico")).ok()?).await
}

async fn linked_icon(client: &reqwest::Client, page: &reqwest::Url) -> Option<Vec<u8>> {
    let mut resp = get_page(client, page.as_str()).await.ok()?;
    if !resp.status().is_success() || resp.url().host_str() != page.host_str() {
        return None;
    }
    let base = resp.url().clone();
    let html = read_capped(&mut resp).await.ok()?;
    let href = icon_href(&String::from_utf8_lossy(&html))?;
    get_image(client, base.join(&href).ok()?).await
}

async fn get_image(client: &reqwest::Client, url: reqwest::Url) -> Option<Vec<u8>> {
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let mut resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = read_capped(&mut resp).await.ok()?;
    // 無いアイコンに HTML (トップページや 404 の画面) を返すサイトがあるので、中身で確かめる
    is_image(&bytes).then_some(bytes)
}

/// 画像らしい中身か (PNG / JPEG / GIF / ICO / WebP / SVG)
fn is_image(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
        || b.starts_with(&[0xff, 0xd8, 0xff])
        || b.starts_with(b"GIF8")
        || b.starts_with(&[0, 0, 1, 0])
        || (b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP"))
        || String::from_utf8_lossy(&b[..b.len().min(512)]).to_lowercase().contains("<svg")
}

/// HTML の <link rel="icon" href="..."> の href (最初のもの)。"shortcut icon" も含む
fn icon_href(html: &str) -> Option<String> {
    let link = regex::Regex::new(r"(?is)<link\b[^>]*>").unwrap();
    let rel = regex::Regex::new(r#"(?is)\brel\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap();
    let href = regex::Regex::new(r#"(?is)\bhref\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap();
    let value = |re: &regex::Regex, tag: &str| {
        re.captures(tag).and_then(|c| c.get(1).or(c.get(2)).or(c.get(3))).map(|m| m.as_str().to_string())
    };
    let found = link.find_iter(html).find_map(|m| {
        let tag = m.as_str();
        let is_icon = value(&rel, tag)?.split_whitespace().any(|t| t.eq_ignore_ascii_case("icon"));
        let h = value(&href, tag).filter(|h| !h.trim().is_empty())?;
        is_icon.then(|| decode_entities(h.trim()))
    });
    found
}

/// HTML から最初の <title> の中身を取り出す (実体参照を戻し、空白をまとめる)。無ければ空文字
pub fn extract_title(html: &str) -> String {
    let re = regex::Regex::new(r"(?is)<title(?:\s[^>]*)?>(.*?)</title>").unwrap();
    let Some(m) = re.captures(html).and_then(|c| c.get(1)) else {
        return String::new();
    };
    decode_entities(m.as_str()).split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(s: &str) -> String {
    let re = regex::Regex::new(r"&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z]+);").unwrap();
    re.replace_all(s, |c: &regex::Captures| {
        let name = &c[1];
        let ch = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "middot" => Some('·'),
            "ndash" => Some('–'),
            "mdash" => Some('—'),
            "raquo" => Some('»'),
            "laquo" => Some('«'),
            _ => {
                let n = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    name.strip_prefix('#').and_then(|d| d.parse().ok())
                };
                n.and_then(char::from_u32)
            }
        };
        // 知らない実体参照はそのまま残す
        ch.map(String::from).unwrap_or_else(|| c[0].to_string())
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_first_title_and_tidies_it() {
        assert_eq!(extract_title("<html><head><title>Resend</title></head>"), "Resend");
        // 属性つき・大文字・改行や連続した空白
        assert_eq!(extract_title("<TITLE data-x=\"1\">\n  Cloudflare\n   Dashboard </TITLE>"), "Cloudflare Dashboard");
        // 実体参照
        assert_eq!(extract_title("<title>A &amp; B &#8211; C &#x30c6;&unknown;</title>"), "A & B – C テ&unknown;");
        // SVG の中の <title> より前にある本物を使う
        assert_eq!(extract_title("<title>本物</title><svg><title>アイコン</title></svg>"), "本物");
        assert_eq!(extract_title("<html><body>なし</body></html>"), "");
    }

    #[test]
    fn finds_the_icon_link_and_tells_images_from_html() {
        let html = r#"<link rel="stylesheet" href="a.css"><link href='/static/fav.png?v=1&amp;x' rel='shortcut icon'>"#;
        assert_eq!(icon_href(html).as_deref(), Some("/static/fav.png?v=1&x"));
        assert_eq!(icon_href(r#"<LINK REL=icon HREF=/i.svg>"#).as_deref(), Some("/i.svg"));
        // apple-touch-icon やスタイルシートは対象外
        assert_eq!(icon_href(r#"<link rel="apple-touch-icon" href="/a.png"><link rel="stylesheet" href="icon.css">"#), None);

        assert!(is_image(&[0x89, b'P', b'N', b'G', 0x0d]));
        assert!(is_image(&[0, 0, 1, 0, 1, 0]));
        assert!(is_image(b"<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
        assert!(!is_image(b"<!doctype html><html><head><title>Not Found</title>"));
        assert!(!is_image(b""));
    }

    /// ネットワークにつなぐので普段は動かさない: `cargo test --lib -- --ignored live_icon --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_icon() {
        let dir = std::env::temp_dir().join("repotether-favicon-test");
        let _ = std::fs::remove_dir_all(&dir);
        for url in [
            "https://github.com/lancard-aikawa/LockWatch",
            "https://dash.cloudflare.com/",
            "https://resend.com/emails",
            "https://console.firebase.google.com/",
        ] {
            println!("{url}: {} バイト", icon(url, &dir).await.len());
        }
        assert!(!icon("https://github.com/", &dir).await.is_empty());
        // 2 回目は残したものを読む
        assert!(std::fs::read_dir(&dir).unwrap().count() >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn rejects_anything_but_http() {
        assert!(fetch("file:///C:/Windows/win.ini").await.is_err());
        assert!(fetch("javascript:alert(1)").await.is_err());
    }

    /// ネットワークにつなぐので普段は動かさない: `cargo test --lib -- --ignored live_title`
    #[tokio::test]
    #[ignore]
    async fn live_title() {
        let t = fetch("https://github.com/lancard-aikawa/LockWatch").await.unwrap();
        println!("タイトル: {t}");
        assert!(t.contains("LockWatch"));
    }
}
