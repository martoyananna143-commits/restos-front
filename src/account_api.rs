//! Isolated Account API client. It never accepts a legacy JWT or legacy organization id.

use std::fmt;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::device_identity::{
    canonical_account_registration_device_proof_message, canonical_device_proof_message,
    CanonicalAccountRegistrationDeviceProof, CanonicalDeviceProof, DeviceIdentityAdapter,
};

const WEB_SESSION_HEADER: &str = "X-RestOS-Web-Session";

#[derive(Clone, PartialEq, Eq)]
pub struct AccountAccessToken(String);

impl AccountAccessToken {
    pub fn from_server(value: String) -> Result<Self, AccountApiError> {
        if value.trim().is_empty() {
            return Err(AccountApiError::InternalError);
        }
        Ok(Self(value))
    }

    fn expose_to_authorization_header(&self) -> &str {
        &self.0
    }

    pub(crate) fn authorization_value(&self) -> &str {
        self.expose_to_authorization_header()
    }
}

impl fmt::Debug for AccountAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AccountAccessToken(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct LegacyAccessToken(String);

impl LegacyAccessToken {
    pub fn new(value: String) -> Self {
        Self(value)
    }
}

impl fmt::Debug for LegacyAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LegacyAccessToken(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedCompanyId(pub Uuid);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountApiError {
    AuthenticationRequired,
    PermissionDenied,
    InvalidRequest,
    ConfigurationUnavailable,
    NetworkUnavailable,
    RateLimited,
    InternalError,
    ReauthenticationRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapAccount {
    pub id: Uuid,
    pub status: String,
    pub security_version: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapCompany {
    pub company_id: Uuid,
    pub company_name: String,
    pub employee_profile_id: Option<Uuid>,
    pub relationship: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountBootstrap {
    pub account: BootstrapAccount,
    pub companies: Vec<BootstrapCompany>,
}

#[derive(Deserialize)]
struct WebRefreshWire {
    access_token: String,
    expires_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshedAccountSession {
    pub access_token: AccountAccessToken,
    pub expires_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SmsRequestInput {
    pub invitation_code: String,
    pub phone: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SmsRequested {
    pub challenge_id: Uuid,
    pub expires_at: String,
    pub resend_available_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SmsVerifyInput {
    #[serde(rename = "challenge_id")]
    pub phone_verification_challenge_id: Uuid,
    pub phone: String,
    pub code: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StandaloneSmsRequestInput {
    pub phone: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct StandaloneSmsVerifyInput {
    pub challenge_id: Uuid,
    pub phone: String,
    pub code: String,
}

#[derive(Clone, Debug)]
pub struct StandaloneRegistrationInput {
    pub phone_verification_challenge_id: Uuid,
    pub phone: String,
    pub display_name: String,
    pub password: String,
    pub platform: String,
    pub device_display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PasswordLoginInput {
    pub phone: String,
    pub password: String,
    pub platform: String,
    pub device_display_name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PasswordResetCompleteInput {
    pub challenge_id: Uuid,
    pub phone: String,
    pub new_password: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreateFirstCompanyInput {
    pub company_name: String,
    pub venue_name: Option<String>,
    pub timezone: String,
    pub locale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct FirstCompanyResponse {
    pub created: bool,
    pub company_id: Uuid,
    pub company_name: String,
    pub company_code: String,
    pub employee_profile_id: Uuid,
    pub position_id: Uuid,
    pub access_profile_id: Uuid,
    pub employee_assignment_id: Uuid,
    pub venue_id: Option<Uuid>,
    pub relationship: String,
}

#[derive(Serialize)]
struct AccountDeviceChallengeRequest<'a> {
    phone_verification_challenge_id: Uuid,
    phone: &'a str,
    app_instance_id: Uuid,
    platform: &'a str,
    public_key: String,
}

#[derive(Deserialize)]
struct AccountDeviceChallengeResponse {
    device_challenge_id: Uuid,
    nonce: String,
    algorithm: String,
    protocol: String,
}

#[derive(Serialize)]
struct StandaloneRegistrationRequest<'a> {
    phone_verification_challenge_id: Uuid,
    phone: &'a str,
    display_name: &'a str,
    password: &'a str,
    app_instance_id: Uuid,
    platform: &'a str,
    device_display_name: &'a Option<String>,
    device_challenge_id: Uuid,
    device_challenge_nonce: &'a str,
    device_challenge_signature: String,
}

#[derive(Serialize)]
struct PasswordLoginRequest<'a> {
    phone: &'a str,
    password: &'a str,
    app_instance_id: Uuid,
    platform: &'a str,
    device_display_name: &'a Option<String>,
    public_key: String,
}

#[derive(Deserialize)]
struct AccountAuthResponse {
    access_token: String,
    expires_at: String,
    token_type: String,
}

#[derive(Clone, Debug, Serialize)]
struct DeviceChallengeRequest<'a> {
    invitation_code: &'a str,
    phone_verification_challenge_id: Uuid,
    phone: &'a str,
    app_instance_id: Uuid,
    platform: &'a str,
    public_key: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceChallengeResponse {
    pub device_challenge_id: Uuid,
    pub invitation_id: Uuid,
    pub nonce: String,
    pub algorithm: String,
    pub expires_at: String,
}

#[derive(Clone, Debug)]
pub struct WebRegistrationInput {
    pub invitation_code: String,
    pub phone_verification_challenge_id: Uuid,
    pub phone: String,
    pub display_name: String,
    pub password: String,
    pub platform: String,
    pub device_display_name: Option<String>,
}

#[derive(Serialize)]
struct WebRegistrationRequest<'a> {
    invitation_code: &'a str,
    phone_verification_challenge_id: Uuid,
    phone: &'a str,
    display_name: &'a str,
    password: &'a str,
    app_instance_id: Uuid,
    platform: &'a str,
    device_display_name: &'a Option<String>,
    device_challenge_id: Uuid,
    device_challenge_nonce: &'a str,
    device_challenge_signature: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebRegistrationResponse {
    pub account_id: Uuid,
    pub employee_profile_id: Uuid,
    pub employee_assignment_id: Uuid,
    pub company_id: Uuid,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub access_token: String,
    pub expires_at: String,
    pub token_type: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
pub struct RegisteredAccountSession {
    pub access_token: AccountAccessToken,
    pub expires_at: String,
    pub bootstrap: AccountBootstrap,
}

#[derive(Clone, Debug)]
pub struct AccountApiClient {
    base_url: String,
}

impl AccountApiClient {
    #[cfg(test)]
    pub const fn standalone_auth_routes() -> [&'static str; 8] {
        [
            "/api/v1/auth/account/registration/sms/request",
            "/api/v1/auth/account/registration/sms/verify",
            "/api/v1/auth/account/registration/device/challenge",
            "/api/v1/auth/account/registration/complete",
            "/api/v1/auth/account/login",
            "/api/v1/auth/account/password-reset/sms/request",
            "/api/v1/auth/account/password-reset/sms/verify",
            "/api/v1/auth/account/password-reset/complete",
        ]
    }

    pub fn new(base_url: String) -> Result<Self, AccountApiError> {
        let trimmed = base_url.trim_end_matches('/').to_string();
        if trimmed.is_empty() {
            return Err(AccountApiError::ConfigurationUnavailable);
        }
        Ok(Self { base_url: trimmed })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn refresh(&self) -> Result<RefreshedAccountSession, AccountApiError> {
        let response = Request::post(&format!(
            "{}/api/v1/auth/web/sessions/refresh",
            self.base_url
        ))
        .credentials(RequestCredentials::Include)
        .header(WEB_SESSION_HEADER, "1")
        .send()
        .await
        .map_err(|_| AccountApiError::NetworkUnavailable)?;
        let wire: WebRefreshWire = parse_json(response).await?;
        Ok(RefreshedAccountSession {
            access_token: AccountAccessToken::from_server(wire.access_token)?,
            expires_at: wire.expires_at,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn logout(&self) -> Result<(), AccountApiError> {
        let response = Request::post(&format!(
            "{}/api/v1/auth/web/sessions/logout",
            self.base_url
        ))
        .credentials(RequestCredentials::Include)
        .header(WEB_SESSION_HEADER, "1")
        .send()
        .await
        .map_err(|_| AccountApiError::NetworkUnavailable)?;
        if response.status() == 204 {
            Ok(())
        } else {
            Err(map_status(response.status()))
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn bootstrap(
        &self,
        token: &AccountAccessToken,
    ) -> Result<AccountBootstrap, AccountApiError> {
        self.authenticated_get("/api/v1/account/bootstrap", token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn authenticated_get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, AccountApiError> {
        let response = Request::get(&format!("{}{}", self.base_url, path))
            .header(
                "Authorization",
                &format!("Bearer {}", token.expose_to_authorization_header()),
            )
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn request_sms(
        &self,
        input: &SmsRequestInput,
    ) -> Result<SmsRequested, AccountApiError> {
        self.public_json_post("/api/v1/auth/invitations/sms/request", input)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn verify_sms(&self, input: &SmsVerifyInput) -> Result<(), AccountApiError> {
        let response = Request::post(&format!(
            "{}/api/v1/auth/invitations/sms/verify",
            self.base_url
        ))
        .json(input)
        .map_err(|_| AccountApiError::InvalidRequest)?
        .send()
        .await
        .map_err(|_| AccountApiError::NetworkUnavailable)?;
        if response.status() == 204 {
            Ok(())
        } else {
            Err(map_status(response.status()))
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn register_web(
        &self,
        identity_adapter: &DeviceIdentityAdapter,
        input: &WebRegistrationInput,
    ) -> Result<RegisteredAccountSession, AccountApiError> {
        let identity = identity_adapter
            .get_or_create()
            .await
            .map_err(|_| AccountApiError::InternalError)?;
        let app_instance_id = identity.metadata().app_instance_id.as_uuid();
        let challenge: DeviceChallengeResponse = self
            .public_json_post(
                "/api/v1/auth/invitations/device/challenge",
                &DeviceChallengeRequest {
                    invitation_code: &input.invitation_code,
                    phone_verification_challenge_id: input.phone_verification_challenge_id,
                    phone: &input.phone,
                    app_instance_id,
                    platform: &input.platform,
                    public_key: encode_base64(identity.public_spki().as_bytes()),
                },
            )
            .await?;
        if challenge.algorithm != "ES256" {
            return Err(AccountApiError::InternalError);
        }
        let canonical = canonical_device_proof_message(CanonicalDeviceProof {
            device_challenge_id: &challenge.device_challenge_id.to_string(),
            invitation_id: &challenge.invitation_id.to_string(),
            phone_challenge_id: &input.phone_verification_challenge_id.to_string(),
            app_instance_id: identity.metadata().app_instance_id,
            platform: &input.platform,
            nonce_base64url: &challenge.nonce,
        })
        .map_err(|_| AccountApiError::InternalError)?;
        let signature = identity
            .sign(&canonical)
            .await
            .map_err(|_| AccountApiError::InternalError)?;
        let registration: WebRegistrationResponse = self
            .web_json_post(
                "/api/v1/auth/invitations/register/web",
                &WebRegistrationRequest {
                    invitation_code: &input.invitation_code,
                    phone_verification_challenge_id: input.phone_verification_challenge_id,
                    phone: &input.phone,
                    display_name: &input.display_name,
                    password: &input.password,
                    app_instance_id,
                    platform: &input.platform,
                    device_display_name: &input.device_display_name,
                    device_challenge_id: challenge.device_challenge_id,
                    device_challenge_nonce: &challenge.nonce,
                    device_challenge_signature: encode_base64(signature.as_bytes()),
                },
            )
            .await?;
        let access_token = AccountAccessToken::from_server(registration.access_token)?;
        let bootstrap = self.bootstrap(&access_token).await?;
        Ok(RegisteredAccountSession {
            access_token,
            expires_at: registration.expires_at,
            bootstrap,
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn request_registration_sms(
        &self,
        input: &StandaloneSmsRequestInput,
    ) -> Result<SmsRequested, AccountApiError> {
        self.web_json_post("/api/v1/auth/account/registration/sms/request", input)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn verify_registration_sms(
        &self,
        input: &StandaloneSmsVerifyInput,
    ) -> Result<(), AccountApiError> {
        self.web_json_post::<_, serde_json::Value>(
            "/api/v1/auth/account/registration/sms/verify",
            input,
        )
        .await
        .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn register_standalone(
        &self,
        identity_adapter: &DeviceIdentityAdapter,
        input: &StandaloneRegistrationInput,
    ) -> Result<RegisteredAccountSession, AccountApiError> {
        let identity = identity_adapter
            .get_or_create()
            .await
            .map_err(|_| AccountApiError::InternalError)?;
        let app_instance_id = identity.metadata().app_instance_id.as_uuid();
        let challenge: AccountDeviceChallengeResponse = self
            .web_json_post(
                "/api/v1/auth/account/registration/device/challenge",
                &AccountDeviceChallengeRequest {
                    phone_verification_challenge_id: input.phone_verification_challenge_id,
                    phone: &input.phone,
                    app_instance_id,
                    platform: &input.platform,
                    public_key: encode_base64(identity.public_spki().as_bytes()),
                },
            )
            .await?;
        if challenge.algorithm != "ES256" || challenge.protocol != "account-registration-v1" {
            return Err(AccountApiError::InternalError);
        }
        let canonical = canonical_account_registration_device_proof_message(
            CanonicalAccountRegistrationDeviceProof {
                device_challenge_id: &challenge.device_challenge_id.to_string(),
                phone_challenge_id: &input.phone_verification_challenge_id.to_string(),
                app_instance_id: identity.metadata().app_instance_id,
                platform: &input.platform,
                nonce_base64url: &challenge.nonce,
            },
        )
        .map_err(|_| AccountApiError::InternalError)?;
        let signature = identity
            .sign(&canonical)
            .await
            .map_err(|_| AccountApiError::InternalError)?;
        self.accept_auth_response(
            self.web_json_post(
                "/api/v1/auth/account/registration/complete",
                &StandaloneRegistrationRequest {
                    phone_verification_challenge_id: input.phone_verification_challenge_id,
                    phone: &input.phone,
                    display_name: &input.display_name,
                    password: &input.password,
                    app_instance_id,
                    platform: &input.platform,
                    device_display_name: &input.device_display_name,
                    device_challenge_id: challenge.device_challenge_id,
                    device_challenge_nonce: &challenge.nonce,
                    device_challenge_signature: encode_base64url(signature.as_bytes()),
                },
            )
            .await?,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn password_login(
        &self,
        identity_adapter: &DeviceIdentityAdapter,
        input: &PasswordLoginInput,
    ) -> Result<RegisteredAccountSession, AccountApiError> {
        let identity = identity_adapter
            .get_or_create()
            .await
            .map_err(|_| AccountApiError::InternalError)?;
        self.accept_auth_response(
            self.web_json_post(
                "/api/v1/auth/account/login",
                &PasswordLoginRequest {
                    phone: &input.phone,
                    password: &input.password,
                    app_instance_id: identity.metadata().app_instance_id.as_uuid(),
                    platform: &input.platform,
                    device_display_name: &input.device_display_name,
                    public_key: encode_base64(identity.public_spki().as_bytes()),
                },
            )
            .await?,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn request_password_reset_sms(
        &self,
        input: &StandaloneSmsRequestInput,
    ) -> Result<SmsRequested, AccountApiError> {
        self.web_json_post("/api/v1/auth/account/password-reset/sms/request", input)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn verify_password_reset_sms(
        &self,
        input: &StandaloneSmsVerifyInput,
    ) -> Result<(), AccountApiError> {
        self.web_json_post::<_, serde_json::Value>(
            "/api/v1/auth/account/password-reset/sms/verify",
            input,
        )
        .await
        .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn complete_password_reset(
        &self,
        input: &PasswordResetCompleteInput,
    ) -> Result<(), AccountApiError> {
        self.web_json_post::<_, serde_json::Value>(
            "/api/v1/auth/account/password-reset/complete",
            input,
        )
        .await
        .map(|_| ())
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_first_company(
        &self,
        token: &AccountAccessToken,
        input: &CreateFirstCompanyInput,
    ) -> Result<FirstCompanyResponse, AccountApiError> {
        self.authenticated_web_json_post("/api/v1/account/companies/first", token, input)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn accept_auth_response(
        &self,
        response: AccountAuthResponse,
    ) -> Result<RegisteredAccountSession, AccountApiError> {
        if response.token_type != "bearer" {
            return Err(AccountApiError::InternalError);
        }
        let access_token = AccountAccessToken::from_server(response.access_token)?;
        let bootstrap = self.bootstrap(&access_token).await?;
        Ok(RegisteredAccountSession {
            access_token,
            expires_at: response.expires_at,
            bootstrap,
        })
    }

    #[cfg(target_arch = "wasm32")]
    async fn public_json_post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, AccountApiError> {
        let response = Request::post(&format!("{}{}", self.base_url, path))
            .json(body)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn web_json_post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, AccountApiError> {
        let response = Request::post(&format!("{}{}", self.base_url, path))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .json(body)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn authenticated_web_json_post<B: Serialize, T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
        body: &B,
    ) -> Result<T, AccountApiError> {
        let response = Request::post(&format!("{}{}", self.base_url, path))
            .credentials(RequestCredentials::Include)
            .header(WEB_SESSION_HEADER, "1")
            .header(
                "Authorization",
                &format!("Bearer {}", token.expose_to_authorization_header()),
            )
            .json(body)
            .map_err(|_| AccountApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AccountApiError::NetworkUnavailable)?;
        parse_json(response).await
    }
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
        400 | 409 | 422 => AccountApiError::InvalidRequest,
        429 => AccountApiError::RateLimited,
        401 => AccountApiError::AuthenticationRequired,
        403 => AccountApiError::PermissionDenied,
        503 => AccountApiError::ConfigurationUnavailable,
        419 => AccountApiError::ReauthenticationRequired,
        _ => AccountApiError::InternalError,
    }
}

fn encode_base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 3) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[(((second & 15) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn encode_base64url(bytes: &[u8]) -> String {
    encode_base64(bytes)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_and_legacy_tokens_are_distinct_and_redacted() {
        let account = AccountAccessToken::from_server("account-secret".into()).unwrap();
        let legacy = LegacyAccessToken::new("legacy-secret".into());
        assert_eq!(format!("{account:?}"), "AccountAccessToken(<redacted>)");
        assert_eq!(format!("{legacy:?}"), "LegacyAccessToken(<redacted>)");
    }

    #[test]
    fn errors_are_mapped_without_raw_backend_body() {
        assert_eq!(map_status(401), AccountApiError::AuthenticationRequired);
        assert_eq!(map_status(403), AccountApiError::PermissionDenied);
        assert_eq!(map_status(422), AccountApiError::InvalidRequest);
        assert_eq!(map_status(503), AccountApiError::ConfigurationUnavailable);
        assert_eq!(map_status(500), AccountApiError::InternalError);
    }

    #[test]
    fn base64_vectors_are_stable() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64url(&[0xfb, 0xff]), "-_8");
        assert!(!encode_base64url(b"proof").contains('='));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn standalone_contract_has_exact_unique_routes() {
        let routes = AccountApiClient::standalone_auth_routes();
        assert_eq!(routes.len(), 8);
        let unique = routes
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), routes.len());
        assert!(routes
            .iter()
            .all(|route| route.starts_with("/api/v1/auth/account/")));
    }
}
