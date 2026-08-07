//! Browser WebAuthn adapter and the exact Stage 22A device-binding proof.

use std::{cell::Cell, rc::Rc};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, ArrayBuffer, Function, Promise, Reflect, Uint8Array, JSON};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{prelude::*, JsCast};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{
    AuthenticatorAssertionResponse, AuthenticatorAttestationResponse, DomException,
    PublicKeyCredential,
};

use crate::{
    account_api::{
        AccountAccessToken, AccountApiClient, AccountApiError, RegisteredAccountSession,
    },
    account_session::AccountSessionAdapter,
    device_identity::{DeviceIdentityAdapter, DeviceIdentityError},
    passkey_api::{
        AuthenticationOptionsRequest, AuthenticationVerifyRequest, DeviceContext, PasskeyApiClient,
        RegistrationVerifyRequest,
    },
};

const DEVICE_PROTOCOL: &[u8] = b"restos-passkey-login-device-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasskeyOperationState {
    Idle,
    Preparing,
    AwaitingAuthenticator,
    Verifying,
    Success,
    SafeError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PasskeyError {
    Unavailable,
    Cancelled,
    AlreadyRegistered,
    SecurityUnavailable,
    InvalidResponse,
    RateLimited,
    NetworkUnavailable,
    AuthenticationRejected,
    InternalError,
}

#[derive(Clone, Default)]
pub struct PasskeyOperationGuard(Rc<Cell<bool>>);

impl PasskeyOperationGuard {
    pub fn try_begin(&self) -> Result<PasskeyOperationPermit, PasskeyError> {
        if self.0.replace(true) {
            return Err(PasskeyError::Unavailable);
        }
        Ok(PasskeyOperationPermit(self.0.clone()))
    }
}

pub struct PasskeyOperationPermit(Rc<Cell<bool>>);

impl Drop for PasskeyOperationPermit {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

#[derive(Clone)]
pub struct PasskeyAdapter {
    passkeys: PasskeyApiClient,
    accounts: AccountApiClient,
    device_identities: DeviceIdentityAdapter,
    guard: PasskeyOperationGuard,
}

impl PasskeyAdapter {
    pub fn new(base_url: String) -> Result<Self, AccountApiError> {
        Ok(Self {
            passkeys: PasskeyApiClient::new(base_url.clone())?,
            accounts: AccountApiClient::new(base_url)?,
            device_identities: DeviceIdentityAdapter::new(),
            guard: PasskeyOperationGuard::default(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn is_supported() -> bool {
        let Some(window) = web_sys::window() else {
            return false;
        };
        let secure = Reflect::get(window.as_ref(), &JsValue::from_str("isSecureContext"))
            .ok()
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let navigator = window.navigator();
        let credentials =
            Reflect::has(navigator.as_ref(), &JsValue::from_str("credentials")).unwrap_or(false);
        let public_key = Reflect::has(&js_sys::global(), &JsValue::from_str("PublicKeyCredential"))
            .unwrap_or(false);
        passkey_capability(secure, credentials, public_key)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn register(
        &self,
        token: &AccountAccessToken,
        display_name: Option<String>,
    ) -> Result<(), PasskeyError> {
        let _permit = self.guard.try_begin()?;
        let options = self
            .passkeys
            .registration_options(token)
            .await
            .map_err(map_api_error)?;
        let credential = create_credential(&options.public_key).await?;
        self.passkeys
            .registration_verify(
                token,
                &RegistrationVerifyRequest {
                    challenge_id: options.challenge_id,
                    credential,
                    display_name,
                },
            )
            .await
            .map_err(map_api_error)?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn authenticate(
        &self,
        session: &AccountSessionAdapter,
        platform: &str,
        display_name: Option<String>,
    ) -> Result<(), PasskeyError> {
        let _permit = self.guard.try_begin()?;
        let identity = self
            .device_identities
            .get_or_create()
            .await
            .map_err(map_device_error)?;
        let device = DeviceContext {
            app_instance_id: identity.metadata().app_instance_id.as_uuid(),
            platform: platform.to_string(),
            display_name,
            public_key: encode_base64url(identity.public_spki().as_bytes()),
        };
        let options = self
            .passkeys
            .authentication_options(&AuthenticationOptionsRequest {
                device: device.clone(),
            })
            .await
            .map_err(map_api_error)?;
        ensure_discoverable_options(&options.public_key)?;
        let assertion = get_credential(&options.public_key).await?;
        let credential_id = credential_raw_id(&assertion)?;
        let challenge = option_challenge(&options.public_key)?;
        let canonical = canonical_passkey_device_message(
            options.challenge_id,
            &challenge,
            device.app_instance_id,
            &device.platform,
            identity.public_spki().as_bytes(),
            &credential_id,
        )?;
        let signature = identity.sign(&canonical).await.map_err(map_device_error)?;
        let verified = self
            .passkeys
            .authentication_verify(&AuthenticationVerifyRequest {
                challenge_id: options.challenge_id,
                credential: assertion,
                device,
                device_signature: encode_base64url(signature.as_bytes()),
            })
            .await
            .map_err(map_api_error)?;
        if verified.token_type != "bearer" {
            return Err(PasskeyError::InvalidResponse);
        }
        let access_token =
            AccountAccessToken::from_server(verified.access_token).map_err(map_api_error)?;
        let bootstrap = self
            .accounts
            .bootstrap(&access_token)
            .await
            .map_err(map_api_error)?;
        session.accept_registered_session(RegisteredAccountSession {
            access_token,
            expires_at: verified.expires_at,
            bootstrap,
        });
        Ok(())
    }
}

pub fn encode_base64url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn decode_base64url(value: &str) -> Result<Vec<u8>, PasskeyError> {
    if value.contains('=')
        || value
            .bytes()
            .any(|byte| !matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
        || value.len() % 4 == 1
    {
        return Err(PasskeyError::InvalidResponse);
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| PasskeyError::InvalidResponse)
}

fn decode_required_base64url(value: &str) -> Result<Vec<u8>, PasskeyError> {
    require_nonempty_bytes(decode_base64url(value)?)
}

fn require_nonempty_bytes(bytes: Vec<u8>) -> Result<Vec<u8>, PasskeyError> {
    if bytes.is_empty() {
        return Err(PasskeyError::InvalidResponse);
    }
    Ok(bytes)
}

fn encode_optional_base64url(value: Option<Vec<u8>>) -> Option<String> {
    value.map(|bytes| encode_base64url(&bytes))
}

pub fn canonical_passkey_device_message(
    challenge_id: Uuid,
    webauthn_challenge: &[u8],
    app_instance_id: Uuid,
    platform: &str,
    public_spki: &[u8],
    credential_id: &[u8],
) -> Result<Vec<u8>, PasskeyError> {
    if platform.is_empty() || !platform.is_ascii() || webauthn_challenge.is_empty() {
        return Err(PasskeyError::InvalidResponse);
    }
    let public_key_digest = Sha256::digest(public_spki);
    let credential_digest = Sha256::digest(credential_id);
    let fields: [&[u8]; 7] = [
        DEVICE_PROTOCOL,
        challenge_id.as_bytes(),
        webauthn_challenge,
        app_instance_id.as_bytes(),
        platform.as_bytes(),
        public_key_digest.as_ref(),
        credential_digest.as_ref(),
    ];
    let mut message = Vec::new();
    for field in fields {
        let length = u32::try_from(field.len()).map_err(|_| PasskeyError::InvalidResponse)?;
        message.extend_from_slice(&length.to_be_bytes());
        message.extend_from_slice(field);
    }
    Ok(message)
}

fn ensure_discoverable_options(value: &Value) -> Result<(), PasskeyError> {
    match value.get("allowCredentials") {
        None => Ok(()),
        Some(Value::Array(values)) if values.is_empty() => Ok(()),
        _ => Err(PasskeyError::InvalidResponse),
    }
}

fn option_challenge(value: &Value) -> Result<Vec<u8>, PasskeyError> {
    value
        .get("challenge")
        .and_then(Value::as_str)
        .ok_or(PasskeyError::InvalidResponse)
        .and_then(decode_required_base64url)
}

fn map_api_error(error: AccountApiError) -> PasskeyError {
    match error {
        AccountApiError::NetworkUnavailable => PasskeyError::NetworkUnavailable,
        AccountApiError::AuthenticationRequired | AccountApiError::ReauthenticationRequired => {
            PasskeyError::AuthenticationRejected
        }
        AccountApiError::InvalidRequest => PasskeyError::InvalidResponse,
        AccountApiError::RateLimited => PasskeyError::RateLimited,
        AccountApiError::ConfigurationUnavailable => PasskeyError::Unavailable,
        _ => PasskeyError::InternalError,
    }
}

pub fn passkey_capability(
    secure_context: bool,
    credentials_api: bool,
    public_key_credential: bool,
) -> bool {
    secure_context && credentials_api && public_key_credential
}

fn map_device_error(_error: DeviceIdentityError) -> PasskeyError {
    PasskeyError::SecurityUnavailable
}

#[cfg(target_arch = "wasm32")]
fn parse_options(value: &Value) -> Result<JsValue, PasskeyError> {
    JSON::parse(&value.to_string()).map_err(|_| PasskeyError::InvalidResponse)
}

#[cfg(target_arch = "wasm32")]
async fn create_credential(options: &Value) -> Result<Value, PasskeyError> {
    let raw = JsFuture::from(create_public_key_credential(parse_options(options)?))
        .await
        .map_err(classify_browser_error)?;
    registration_credential_json(raw)
}

#[cfg(target_arch = "wasm32")]
async fn get_credential(options: &Value) -> Result<Value, PasskeyError> {
    let raw = JsFuture::from(get_public_key_credential(parse_options(options)?))
        .await
        .map_err(classify_browser_error)?;
    authentication_credential_json(raw)
}

#[cfg(target_arch = "wasm32")]
fn registration_credential_json(value: JsValue) -> Result<Value, PasskeyError> {
    let credential = value
        .dyn_into::<PublicKeyCredential>()
        .map_err(|_| PasskeyError::InvalidResponse)?;
    let authenticator_attachment =
        optional_string_property(credential.as_ref(), "authenticatorAttachment")?;
    let response = credential
        .response()
        .dyn_into::<AuthenticatorAttestationResponse>()
        .map_err(|_| PasskeyError::InvalidResponse)?;
    let transports = reflected_transports(response.as_ref())?;
    let raw_id = required_array_buffer_bytes(credential.raw_id())?;
    let client_data_json = required_array_buffer_bytes(response.client_data_json())?;
    let attestation_object = required_array_buffer_bytes(response.attestation_object())?;
    Ok(json!({
        "id": credential.id(),
        "rawId": encode_base64url(&raw_id),
        "response": {
            "clientDataJSON": encode_base64url(&client_data_json),
            "attestationObject": encode_base64url(&attestation_object),
            "transports": transports,
        },
        "authenticatorAttachment": authenticator_attachment,
        "type": credential.type_(),
    }))
}

#[cfg(target_arch = "wasm32")]
fn authentication_credential_json(value: JsValue) -> Result<Value, PasskeyError> {
    let credential = value
        .dyn_into::<PublicKeyCredential>()
        .map_err(|_| PasskeyError::InvalidResponse)?;
    let authenticator_attachment =
        optional_string_property(credential.as_ref(), "authenticatorAttachment")?;
    let response = credential
        .response()
        .dyn_into::<AuthenticatorAssertionResponse>()
        .map_err(|_| PasskeyError::InvalidResponse)?;
    let user_handle = encode_optional_base64url(response.user_handle().map(array_buffer_bytes));
    let raw_id = required_array_buffer_bytes(credential.raw_id())?;
    let client_data_json = required_array_buffer_bytes(response.client_data_json())?;
    let authenticator_data = required_array_buffer_bytes(response.authenticator_data())?;
    let signature = required_array_buffer_bytes(response.signature())?;
    Ok(json!({
        "id": credential.id(),
        "rawId": encode_base64url(&raw_id),
        "response": {
            "clientDataJSON": encode_base64url(&client_data_json),
            "authenticatorData": encode_base64url(&authenticator_data),
            "signature": encode_base64url(&signature),
            "userHandle": user_handle,
        },
        "authenticatorAttachment": authenticator_attachment,
        "type": credential.type_(),
    }))
}

#[cfg(target_arch = "wasm32")]
fn credential_raw_id(value: &Value) -> Result<Vec<u8>, PasskeyError> {
    value
        .get("rawId")
        .and_then(Value::as_str)
        .ok_or(PasskeyError::InvalidResponse)
        .and_then(decode_required_base64url)
}

#[cfg(target_arch = "wasm32")]
fn reflected_transports(response: &JsValue) -> Result<Vec<String>, PasskeyError> {
    let method = Reflect::get(response, &JsValue::from_str("getTransports"))
        .map_err(|_| PasskeyError::InvalidResponse)?;
    if method.is_undefined() {
        return Ok(Vec::new());
    }
    let values = method
        .dyn_into::<Function>()
        .map_err(|_| PasskeyError::InvalidResponse)?
        .call0(response)
        .map_err(|_| PasskeyError::InvalidResponse)?;
    let values = Array::from(&values);
    let mut result = Vec::with_capacity(values.length() as usize);
    for value in values.iter() {
        result.push(value.as_string().ok_or(PasskeyError::InvalidResponse)?);
    }
    Ok(result)
}

#[cfg(target_arch = "wasm32")]
fn optional_string_property(
    value: &JsValue,
    property: &str,
) -> Result<Option<String>, PasskeyError> {
    let property = Reflect::get(value, &JsValue::from_str(property))
        .map_err(|_| PasskeyError::InternalError)?;
    if property.is_undefined() || property.is_null() {
        return Ok(None);
    }
    property
        .as_string()
        .map(Some)
        .ok_or(PasskeyError::InvalidResponse)
}

#[cfg(target_arch = "wasm32")]
fn array_buffer_bytes(value: ArrayBuffer) -> Vec<u8> {
    Uint8Array::new(&value).to_vec()
}

#[cfg(target_arch = "wasm32")]
fn required_array_buffer_bytes(value: ArrayBuffer) -> Result<Vec<u8>, PasskeyError> {
    require_nonempty_bytes(array_buffer_bytes(value))
}

#[cfg(target_arch = "wasm32")]
fn classify_browser_error(error: JsValue) -> PasskeyError {
    let name = error
        .dyn_ref::<DomException>()
        .map(DomException::name)
        .unwrap_or_default();
    match name.as_str() {
        "NotAllowedError" | "AbortError" => PasskeyError::Cancelled,
        "InvalidStateError" => PasskeyError::AlreadyRegistered,
        "SecurityError" => PasskeyError::SecurityUnavailable,
        "NotSupportedError" => PasskeyError::Unavailable,
        _ => PasskeyError::InternalError,
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
function decodeRequiredBase64url(value) {
  if (typeof value !== "string" || !value || value.includes("=") ||
      value.length % 4 === 1 || !/^[A-Za-z0-9_-]+$/.test(value)) {
    throw new DOMException("Invalid WebAuthn options", "DataError");
  }
  const base64 = value.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat((4 - value.length % 4) % 4);
  const binary = atob(base64);
  return Uint8Array.from(binary, char => char.charCodeAt(0));
}

function descriptor(value) {
  return { ...value, id: decodeRequiredBase64url(value.id) };
}

function creationOptions(value) {
  return {
    ...value,
    challenge: decodeRequiredBase64url(value.challenge),
    user: { ...value.user, id: decodeRequiredBase64url(value.user.id) },
    excludeCredentials: (value.excludeCredentials || []).map(descriptor),
  };
}

function requestOptions(value) {
  if (value.allowCredentials && value.allowCredentials.length) {
    throw new DOMException("Discoverable authentication required", "DataError");
  }
  return {
    ...value,
    challenge: decodeRequiredBase64url(value.challenge),
    allowCredentials: [],
  };
}

export async function create_public_key_credential(value) {
  if (!navigator.credentials || !globalThis.PublicKeyCredential) {
    return Promise.reject(new DOMException("WebAuthn unavailable", "NotSupportedError"));
  }
  const publicKey = creationOptions(value);
  return await navigator.credentials.create({ publicKey });
}

export async function get_public_key_credential(value) {
  if (!navigator.credentials || !globalThis.PublicKeyCredential) {
    return Promise.reject(new DOMException("WebAuthn unavailable", "NotSupportedError"));
  }
  const publicKey = requestOptions(value);
  return await navigator.credentials.get({ publicKey });
}
"#)]
extern "C" {
    fn create_public_key_credential(options: JsValue) -> Promise;
    fn get_public_key_credential(options: JsValue) -> Promise;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn base64url_empty_and_binary_vectors_round_trip_strictly() {
        assert_eq!(encode_base64url(&[]), "");
        assert_eq!(decode_base64url("").unwrap(), Vec::<u8>::new());
        assert_eq!(
            decode_base64url(&encode_base64url(&[])).unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(encode_base64url(b"\xfb\xff"), "-_8");
        assert_eq!(decode_base64url("-_8").unwrap(), b"\xfb\xff");
        for vector in [
            b"a".as_slice(),
            b"binary\0vector".as_slice(),
            &[0, 1, 2, 127, 128, 254, 255],
        ] {
            let encoded = encode_base64url(vector);
            assert!(!encoded.contains('='));
            assert!(encoded
                .bytes()
                .all(|byte| matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_')));
            assert_eq!(decode_base64url(&encoded).unwrap(), vector);
        }
        for invalid in ["A", "AA==", "a+b", "a/b", "a b", "a\nb", "é", "_"] {
            assert_eq!(
                decode_base64url(invalid),
                Err(PasskeyError::InvalidResponse)
            );
        }
    }

    #[wasm_bindgen_test]
    fn mandatory_binary_values_are_separate_from_generic_codec() {
        assert_eq!(decode_base64url(""), Ok(Vec::new()));
        assert_eq!(
            decode_required_base64url(""),
            Err(PasskeyError::InvalidResponse)
        );
        assert_eq!(
            require_nonempty_bytes(Vec::new()),
            Err(PasskeyError::InvalidResponse)
        );
        assert_eq!(encode_optional_base64url(None), None);
        assert_eq!(
            encode_optional_base64url(Some(Vec::new())),
            Some(String::new())
        );
        assert_eq!(
            option_challenge(&json!({"challenge": ""})),
            Err(PasskeyError::InvalidResponse)
        );
        assert_eq!(
            credential_raw_id(&json!({"rawId": ""})),
            Err(PasskeyError::InvalidResponse)
        );
    }

    #[wasm_bindgen_test]
    fn canonical_message_matches_backend_bytes_and_is_deterministic() {
        let challenge_id = Uuid::from_u128(0x00112233445566778899aabbccddeeff);
        let app_id = Uuid::from_u128(0xffeeddccbbaa49888776665544332211);
        let make = || {
            canonical_passkey_device_message(
                challenge_id,
                b"challenge",
                app_id,
                "web",
                b"spki",
                b"credential",
            )
            .unwrap()
        };
        let message = make();
        assert_eq!(message, make());
        let fields = parse_fields(&message);
        assert_eq!(fields.len(), 7);
        assert_eq!(fields[0], DEVICE_PROTOCOL);
        assert_ne!(fields[0], b"restos-device-registration-v1");
        assert_eq!(fields[1], challenge_id.as_bytes());
        assert_eq!(fields[2], b"challenge");
        assert_eq!(fields[3], app_id.as_bytes());
        assert_eq!(fields[4], b"web");
        let spki_digest = Sha256::digest(b"spki");
        let credential_digest = Sha256::digest(b"credential");
        assert_eq!(fields[5], &spki_digest[..]);
        assert_eq!(fields[6], &credential_digest[..]);
        assert_ne!(fields[5], b"spki");
        assert_ne!(fields[6], b"credential");
        assert_eq!(&message[..4], &(DEVICE_PROTOCOL.len() as u32).to_be_bytes());
    }

    #[wasm_bindgen_test]
    fn every_canonical_component_is_domain_separated_and_ordered() {
        let challenge_id = Uuid::from_u128(0x10112233445566778899aabbccddeeff);
        let app_id = Uuid::from_u128(0x20eeddccbbaa49888776665544332211);
        let baseline = canonical_passkey_device_message(
            challenge_id,
            b"challenge",
            app_id,
            "web",
            b"spki",
            b"credential",
        )
        .unwrap();
        let variants = [
            framed(&[
                b"another-passkey-domain",
                challenge_id.as_bytes(),
                b"challenge",
                app_id.as_bytes(),
                b"web",
                Sha256::digest(b"spki").as_ref(),
                Sha256::digest(b"credential").as_ref(),
            ]),
            canonical_passkey_device_message(
                Uuid::from_u128(0x30112233445566778899aabbccddeeff),
                b"challenge",
                app_id,
                "web",
                b"spki",
                b"credential",
            )
            .unwrap(),
            canonical_passkey_device_message(
                challenge_id,
                b"changed",
                app_id,
                "web",
                b"spki",
                b"credential",
            )
            .unwrap(),
            canonical_passkey_device_message(
                challenge_id,
                b"challenge",
                Uuid::from_u128(0x40eeddccbbaa49888776665544332211),
                "web",
                b"spki",
                b"credential",
            )
            .unwrap(),
            canonical_passkey_device_message(
                challenge_id,
                b"challenge",
                app_id,
                "ios",
                b"spki",
                b"credential",
            )
            .unwrap(),
            canonical_passkey_device_message(
                challenge_id,
                b"challenge",
                app_id,
                "web",
                b"changed-spki",
                b"credential",
            )
            .unwrap(),
            canonical_passkey_device_message(
                challenge_id,
                b"challenge",
                app_id,
                "web",
                b"spki",
                b"changed-credential",
            )
            .unwrap(),
        ];
        for variant in &variants {
            assert_ne!(variant.as_slice(), baseline.as_slice());
        }

        let changed_spki = parse_fields(&variants[5]);
        let changed_credential = parse_fields(&variants[6]);
        let original = parse_fields(&baseline);
        assert_ne!(changed_spki[5], original[5]);
        assert_eq!(changed_spki[6], original[6]);
        assert_eq!(changed_credential[5], original[5]);
        assert_ne!(changed_credential[6], original[6]);
        assert_ne!(framed(&[b"ab", b"c"]), framed(&[b"a", b"bc"]));
    }

    #[wasm_bindgen_test]
    fn discoverable_options_reject_nonempty_descriptors() {
        assert!(ensure_discoverable_options(&json!({"challenge": "AA"})).is_ok());
        assert!(
            ensure_discoverable_options(&json!({"challenge": "AA", "allowCredentials": []}))
                .is_ok()
        );
        assert_eq!(
            ensure_discoverable_options(
                &json!({"challenge": "AA", "allowCredentials": [{"id": "AA"}]})
            ),
            Err(PasskeyError::InvalidResponse)
        );
    }

    #[wasm_bindgen_test]
    fn operation_guard_is_single_flight_and_releases_on_drop() {
        let guard = PasskeyOperationGuard::default();
        let permit = guard.try_begin().unwrap();
        assert_eq!(guard.try_begin().err(), Some(PasskeyError::Unavailable));
        drop(permit);
        assert!(guard.try_begin().is_ok());
    }

    fn framed(fields: &[&[u8]]) -> Vec<u8> {
        let mut output = Vec::new();
        for field in fields {
            output.extend_from_slice(&(field.len() as u32).to_be_bytes());
            output.extend_from_slice(field);
        }
        output
    }

    fn parse_fields(message: &[u8]) -> Vec<&[u8]> {
        let mut offset = 0;
        let mut fields = Vec::new();
        while offset < message.len() {
            let length =
                u32::from_be_bytes(message[offset..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            fields.push(&message[offset..offset + length]);
            offset += length;
        }
        fields
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test(async)]
    async fn malformed_creation_options_reject_inside_promise_boundary() {
        let malformed = JSON::parse(r#"{"challenge":"A"}"#).unwrap();
        let create_error = JsFuture::from(create_public_key_credential(malformed))
            .await
            .unwrap_err();
        assert_eq!(
            classify_browser_error(create_error),
            PasskeyError::InternalError
        );
    }

    #[wasm_bindgen_test(async)]
    async fn malformed_authentication_options_reject_inside_promise_boundary() {
        let malformed = JSON::parse(r#"{"challenge":"A"}"#).unwrap();
        let get_error = JsFuture::from(get_public_key_credential(malformed))
            .await
            .unwrap_err();
        assert_eq!(
            classify_browser_error(get_error),
            PasskeyError::InternalError
        );
    }
}
