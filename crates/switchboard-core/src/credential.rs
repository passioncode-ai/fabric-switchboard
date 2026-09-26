use crate::{AuthKind, Provider};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_INPUT: usize = 64 * 1024;
const MAX_TOKEN: usize = 16 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Captured Codex identity token, retained only inside the vault for native export.
    /// Parsing it does not establish a verified identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    pub expires_at: Option<i64>,
    pub account_id: Option<String>,
}

fn token_valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_TOKEN && s.bytes().all(|b| b.is_ascii_graphic())
}
pub(crate) fn identity_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'@' | b':'))
}

impl Credential {
    pub(crate) fn validate(&self, provider: Provider, kind: AuthKind) -> Result<(), String> {
        if provider == Provider::Codex && kind == AuthKind::SetupToken {
            return Err("Codex does not support setup tokens".into());
        }
        if !token_valid(&self.access_token)
            || self.refresh_token.as_ref().is_some_and(|s| !token_valid(s))
            || self.id_token.as_ref().is_some_and(|s| !token_valid(s))
            || (provider != Provider::Codex && self.id_token.is_some())
            || self.account_id.as_ref().is_some_and(|s| !identity_valid(s))
            || self
                .expires_at
                .is_some_and(|t| t <= 0 || t > 253_402_300_799)
            || (kind != AuthKind::OAuth
                && (self.refresh_token.is_some()
                    || self.id_token.is_some()
                    || self.expires_at.is_some()
                    || self.account_id.is_some()))
        {
            return Err("Credential format is invalid".into());
        }
        Ok(())
    }
    pub fn parse(provider: Provider, kind: AuthKind, input: &str) -> Result<Self, String> {
        if input.is_empty() || input.len() > MAX_INPUT {
            return Err("Credential input is empty or too large".into());
        }
        let credential = if kind == AuthKind::OAuth {
            let parsed: Value = serde_json::from_str(input).map_err(|_| "OAuth JSON is invalid")?;
            let (value, access, refresh, expires, account) = match provider {
                Provider::Claude => (
                    parsed.get("claudeAiOauth"),
                    "accessToken",
                    "refreshToken",
                    "expiresAt",
                    "accountUuid",
                ),
                Provider::Codex => (
                    parsed.get("tokens"),
                    "access_token",
                    "refresh_token",
                    "expires_at",
                    "account_id",
                ),
            };
            let value = value
                .and_then(Value::as_object)
                .ok_or("Unsupported OAuth JSON schema")?;
            let access_token = value
                .get(access)
                .and_then(Value::as_str)
                .ok_or("OAuth access token is missing")?
                .to_owned();
            let string = |key: &str| -> Result<Option<String>, String> {
                match value.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::String(s)) => Ok(Some(s.clone())),
                    _ => Err("OAuth field type is invalid".into()),
                }
            };
            let expires_at = match value.get(expires) {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let t = v.as_i64().ok_or("OAuth expiration is invalid")?;
                    Some(if provider == Provider::Claude {
                        t / 1000
                    } else {
                        t
                    })
                }
            };
            // JWT claims are local hints only, never provider verification. Taking exp
            // from a captured Codex JWT avoids treating its auth.json snapshot as eternal.
            let jwt_expiry = access_token
                .split('.')
                .nth(1)
                .and_then(|part| URL_SAFE_NO_PAD.decode(part).ok())
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|claims| claims.get("exp").and_then(Value::as_i64));
            let expires_at = match (expires_at, jwt_expiry) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            Self {
                access_token,
                refresh_token: string(refresh)?,
                id_token: if provider == Provider::Codex {
                    string("id_token")?
                } else {
                    None
                },
                expires_at,
                account_id: string(account)?,
            }
        } else {
            Self {
                access_token: input.into(),
                refresh_token: None,
                id_token: None,
                expires_at: None,
                account_id: None,
            }
        };
        credential.validate(provider, kind)?;
        Ok(credential)
    }
}
