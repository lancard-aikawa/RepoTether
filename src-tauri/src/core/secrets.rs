//! トークンを OS の資格情報の保管庫に置く。
//! - Windows: 資格情報マネージャー (汎用資格情報)。DPAPI で Windows のログインに結びつけて暗号化される。
//!   「RepoTether:<アカウント ID>」として出る
//! - macOS: ログインキーチェーン
//! - それ以外: 未対応 (保存しようとするとエラー)

pub fn target(account_id: &str) -> String {
    format!("RepoTether:{account_id}")
}

#[cfg(windows)]
mod imp {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn last_error(what: &str) -> String {
        let code = unsafe { GetLastError() };
        format!("資格情報マネージャーの{what}に失敗しました (エラー {code})")
    }

    pub fn set(target: &str, user: &str, secret: &str) -> Result<(), String> {
        let mut t = wide(target);
        let mut u = wide(user);
        let mut blob = secret.as_bytes().to_vec();
        let mut cred: CREDENTIALW = unsafe { std::mem::zeroed() };
        cred.Type = CRED_TYPE_GENERIC;
        cred.TargetName = t.as_mut_ptr();
        cred.UserName = u.as_mut_ptr();
        cred.CredentialBlobSize = blob.len() as u32;
        cred.CredentialBlob = blob.as_mut_ptr();
        // この PC のこのユーザーだけ (ドメインのローミングはしない)
        cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
        if unsafe { CredWriteW(&cred, 0) } == 0 {
            return Err(last_error("書き込み"));
        }
        Ok(())
    }

    pub fn get(target: &str) -> Result<Option<String>, String> {
        let t = wide(target);
        let mut p: *mut CREDENTIALW = null_mut();
        if unsafe { CredReadW(t.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) } == 0 {
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(last_error("読み取り"));
        }
        let bytes = unsafe {
            let c = &*p;
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        };
        unsafe { CredFree(p as *const _) };
        String::from_utf8(bytes).map(Some).map_err(|_| "資格情報の中身を読めません".into())
    }

    pub fn delete(target: &str) -> Result<(), String> {
        let t = wide(target);
        if unsafe { CredDeleteW(t.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 && unsafe { GetLastError() } != ERROR_NOT_FOUND {
            return Err(last_error("削除"));
        }
        Ok(())
    }
}

/// macOS はログインキーチェーンに「サービス RepoTether / アカウント RepoTether:<ID>」で置く。
/// キーチェーンアクセス.app で見られる。
#[cfg(target_os = "macos")]
mod imp {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    const SERVICE: &str = "RepoTether";
    /// errSecItemNotFound
    const NOT_FOUND: i32 = -25300;

    pub fn set(target: &str, _user: &str, secret: &str) -> Result<(), String> {
        set_generic_password(SERVICE, target, secret.as_bytes())
            .map_err(|e| format!("キーチェーンへの書き込みに失敗しました: {e}"))
    }

    pub fn get(target: &str) -> Result<Option<String>, String> {
        match get_generic_password(SERVICE, target) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| "キーチェーンの中身を読めません".into()),
            Err(e) if e.code() == NOT_FOUND => Ok(None),
            Err(e) => Err(format!("キーチェーンの読み取りに失敗しました: {e}")),
        }
    }

    pub fn delete(target: &str) -> Result<(), String> {
        match delete_generic_password(SERVICE, target) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == NOT_FOUND => Ok(()),
            Err(e) => Err(format!("キーチェーンからの削除に失敗しました: {e}")),
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub fn set(_: &str, _: &str, _: &str) -> Result<(), String> {
        Err("この OS では資格情報の保存に対応していません".into())
    }
    pub fn get(_: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
    pub fn delete(_: &str) -> Result<(), String> {
        Ok(())
    }
}

pub use imp::{delete, get, set};

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let t = target("test-roundtrip");
        set(&t, "tester", "ghp_日本語も通る").unwrap();
        assert_eq!(get(&t).unwrap().as_deref(), Some("ghp_日本語も通る"));
        delete(&t).unwrap();
        assert_eq!(get(&t).unwrap(), None);
        // 無いものを消してもエラーにしない
        delete(&t).unwrap();
    }
}
