//! Removes credentials from text that is shown or kept: URLs may carry a
//! user and password or token (`https://user:token@host/…`) or a token in the
//! query string (`?access_token=…`).

use std::borrow::Cow;

/// What replaces a removed secret.
pub const REDACTED: &str = "***";

/// Query parameters whose value is a secret.
const SECRET_PARAMS: &[&str] = &[
    "token",
    "access_token",
    "private_token",
    "oauth_token",
    "password",
    "passwd",
    "secret",
    "client_secret",
    "api_key",
    "apikey",
    "key",
    "auth",
    "sig",
    "signature",
];

/// `text` with the credentials of every URL in it replaced by [`REDACTED`].
pub fn redact_credentials(text: &str) -> Cow<'_, str> {
    if !text.contains("://") {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(scheme_end) = rest.find("://") {
        let body_start = scheme_end + 3;
        let url_end = rest[body_start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '<' | '>' | '`'))
            .map_or(rest.len(), |i| body_start + i);
        out.push_str(&rest[..body_start]);
        out.push_str(&redact_url_body(&rest[body_start..url_end]));
        rest = &rest[url_end..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

/// The part of a URL after `scheme://`.
fn redact_url_body(body: &str) -> String {
    let authority_end = body.find(['/', '?', '#']).unwrap_or(body.len());
    let (authority, path) = body.split_at(authority_end);
    let mut out = String::with_capacity(body.len());
    match authority.rfind('@') {
        Some(at) => {
            out.push_str(REDACTED);
            out.push_str(&authority[at..]);
        }
        None => out.push_str(authority),
    }
    match path.find('?') {
        Some(q) => {
            out.push_str(&path[..q + 1]);
            let (query, fragment) = match path[q + 1..].find('#') {
                Some(f) => path[q + 1..].split_at(f),
                None => (&path[q + 1..], ""),
            };
            let params: Vec<String> = query
                .split('&')
                .map(|param| match param.split_once('=') {
                    Some((name, _)) if is_secret_param(name) => format!("{name}={REDACTED}"),
                    _ => param.to_owned(),
                })
                .collect();
            out.push_str(&params.join("&"));
            out.push_str(fragment);
        }
        None => out.push_str(path),
    }
    out
}

fn is_secret_param(name: &str) -> bool {
    SECRET_PARAMS
        .iter()
        .any(|secret| name.eq_ignore_ascii_case(secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_user_and_password_from_urls() {
        assert_eq!(
            redact_credentials("git clone -- https://ana:s3cret@example.com/a/b.git b"),
            "git clone -- https://***@example.com/a/b.git b"
        );
        // A token alone in the user part is a secret too.
        assert_eq!(
            redact_credentials("fatal: unable to access 'https://ghp_abc123@github.com/x/y.git/'"),
            "fatal: unable to access 'https://***@github.com/x/y.git/'"
        );
        assert_eq!(
            redact_credentials("ssh://git@example.com:22/repo.git"),
            "ssh://***@example.com:22/repo.git"
        );
    }

    #[test]
    fn removes_tokens_from_query_strings() {
        assert_eq!(
            redact_credentials("https://example.com/r.git?access_token=abc&x=1#top"),
            "https://example.com/r.git?access_token=***&x=1#top"
        );
        assert_eq!(
            redact_credentials("https://u:p@h/r?Token=abc"),
            "https://***@h/r?Token=***"
        );
    }

    #[test]
    fn leaves_everything_else_alone() {
        for text in [
            "git log --format=%H%x1f%an -- src/a@b.txt",
            "git push --porcelain origin main",
            "file:///Users/ana/gitcraft-remoto.git",
            "https://example.com/a/b.git",
            "git@example.com:a/b.git",
        ] {
            assert_eq!(redact_credentials(text), text);
        }
        assert!(matches!(redact_credentials("no urls"), Cow::Borrowed(_)));
    }

    #[test]
    fn handles_several_urls() {
        assert_eq!(
            redact_credentials("from https://a:b@x/1 to http://c@y/2."),
            "from https://***@x/1 to http://***@y/2."
        );
    }
}
