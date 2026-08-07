//! Exact Stage 22A passkey HTTP contract. Secret-bearing values are never formatted or logged.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::{AccountAccessToken, AccountApiError};

const PREFIX: &str = "/api/v1/auth/web/passkeys";
const WEB_SESSION_HEADER: &str = "X-RestOS-Web-Session";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DeviceContext {
    pub app_instance_id: Uuid,
    pub platform: String,
    pub display_name: Option<String>,
    pub public_key: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct PasskeyOptionsResponse {
    pub challenge_id: Uuid,
    pub public_key: Value,
    pub expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegistrationVerifyRequest {
    pub challenge_id: Uuid,
    pub credential: Value,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RegistrationVerifyResponse {
    pub identity_id: Uuid,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AuthenticationOptionsRequest {
    pub device: DeviceContext,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuthenticationVerifyRequest {
    pub challenge_id: Uuid,
    pub credential: Value,
    pub device: DeviceContext,
    pub device_signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AuthenticationVerifyResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PasskeySummary {
    pub identity_id: Uuid,
    pub display_name: Option<String>,
    pub transports: Vec<String>,
    pub backup_eligible: bool,
    pub backup_state: bool,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub status: String,
    pub revoked_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevokePasskeyOutcome {
    Revoked,
    AlreadyRemoved,
}

#[derive(Clone, Debug)]
pub struct PasskeyApiClient {
    base_url: String,
}

impl PasskeyApiClient {
    pub fn new(base_url: String) -> Result<Self, AccountApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(AccountApiError::ConfigurationUnavailable);
        }
        Ok(Self { base_url })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn registration_options(
        &self,
        token: &AccountAccessToken,
    ) -> Result<PasskeyOptionsResponse, AccountApiError> {
        self.authenticated_json_post("/registration/options", token, &serde_json::json!({}))
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn registration_verify(
        &self,
        token: &AccountAccessToken,
        input: &RegistrationVerifyRequest,
    ) -> Result<RegistrationVerifyResponse, AccountApiError> {
        self.authenticated_json_post("/registration/verify", token, input)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn authentication_options(
        &self,
        input: &AuthenticationOptionsRequest,
    ) -> Result<PasskeyOptionsResponse, AccountApiError> {
        let response = Request::post(&self.url("/authentication/options"))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .json(input)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn authentication_verify(
        &self,
        input: &AuthenticationVerifyRequest,
    ) -> Result<AuthenticationVerifyResponse, AccountApiError> {
        let response = Request::post(&self.url("/authentication/verify"))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .json(input)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn list(
        &self,
        token: &AccountAccessToken,
    ) -> Result<Vec<PasskeySummary>, AccountApiError> {
        let response = Request::get(&self.url(""))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .header("Authorization", &authorization(token))
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn revoke(
        &self,
        token: &AccountAccessToken,
        identity_id: Uuid,
    ) -> Result<RevokePasskeyOutcome, AccountApiError> {
        let response = Request::delete(&self.url(&format!("/{identity_id}")))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .header("Authorization", &authorization(token))
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        match response.status() {
            204 => Ok(RevokePasskeyOutcome::Revoked),
            404 => Ok(RevokePasskeyOutcome::AlreadyRemoved),
            status => Err(map_status(status)),
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn authenticated_json_post<B: Serialize, T: DeserializeOwned>(
        &self,
        suffix: &str,
        token: &AccountAccessToken,
        input: &B,
    ) -> Result<T, AccountApiError> {
        let response = Request::post(&self.url(suffix))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .header("Authorization", &authorization(token))
            .json(input)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    fn url(&self, suffix: &str) -> String {
        format!("{}{}{}", self.base_url, PREFIX, suffix)
    }
}

#[cfg(target_arch = "wasm32")]
fn authorization(token: &AccountAccessToken) -> String {
    format!("Bearer {}", token.authorization_value())
}

#[cfg(target_arch = "wasm32")]
async fn parse_json<T: DeserializeOwned>(response: Response) -> Result<T, AccountApiError> {
    if !response.ok() {
        return Err(map_status(response.status()));
    }
    response
        .json::<T>()
        .await
        .map_err(|_| AccountApiError::InternalError)
}

fn map_status(status: u16) -> AccountApiError {
    match status {
        400 | 404 | 409 | 422 => AccountApiError::InvalidRequest,
        429 => AccountApiError::RateLimited,
        401 => AccountApiError::AuthenticationRequired,
        403 => AccountApiError::PermissionDenied,
        503 => AccountApiError::ConfigurationUnavailable,
        _ => AccountApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn authentication_options_request_has_no_account_identifier() {
        let body = AuthenticationOptionsRequest {
            device: DeviceContext {
                app_instance_id: Uuid::from_u128(1),
                platform: "web".into(),
                display_name: Some("Browser".into()),
                public_key: "c3BraQ".into(),
            },
        };
        let json = serde_json::to_value(body).unwrap();
        assert!(json.get("account_id").is_none());
        assert!(json.get("email").is_none());
        assert!(json.get("phone").is_none());
    }

    #[wasm_bindgen_test]
    fn dto_field_names_match_backend_contract() {
        let value = serde_json::to_value(AuthenticationVerifyRequest {
            challenge_id: Uuid::from_u128(2),
            credential: serde_json::json!({"type": "public-key"}),
            device: DeviceContext {
                app_instance_id: Uuid::from_u128(3),
                platform: "web".into(),
                display_name: None,
                public_key: "c3BraQ".into(),
            },
            device_signature: "c2ln".into(),
        })
        .unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ["challenge_id", "credential", "device", "device_signature"]
        );
        let device = value.get("device").unwrap().as_object().unwrap();
        assert_eq!(device.get("display_name"), Some(&Value::Null));
        for forbidden in [
            "account_id",
            "email",
            "phone",
            "private_key",
            "refresh_token",
            "challenge_digest",
            "credential_public_key",
        ] {
            assert!(value.get(forbidden).is_none());
            assert!(device.get(forbidden).is_none());
        }
    }

    #[wasm_bindgen_test]
    fn webauthn_credential_payload_keeps_browser_camel_case_fields() {
        let value = serde_json::to_value(RegistrationVerifyRequest {
            challenge_id: Uuid::from_u128(4),
            credential: serde_json::json!({
                "id": "credential",
                "rawId": "Y3JlZGVudGlhbA",
                "response": {
                    "clientDataJSON": "Y2xpZW50",
                    "attestationObject": "YXR0ZXN0YXRpb24",
                    "transports": ["internal"],
                },
                "authenticatorAttachment": null,
                "type": "public-key",
            }),
            display_name: None,
        })
        .unwrap();
        assert_eq!(value.get("display_name"), Some(&Value::Null));
        let credential = value.get("credential").unwrap();
        assert!(credential.get("rawId").is_some());
        assert!(credential.get("authenticatorAttachment").is_some());
        let response = credential.get("response").unwrap();
        assert!(response.get("clientDataJSON").is_some());
        assert!(response.get("attestationObject").is_some());
        assert!(response.get("client_data_json").is_none());
        assert!(response.get("attestation_object").is_none());
    }
}
