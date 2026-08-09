//! Browser-only device identity and backend-compatible device-proof primitives.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use js_sys::{Array, Promise, Uint8Array};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{prelude::*, JsCast};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::CryptoKey;

const PROTOCOL: &[u8] = b"restos-device-registration-v1";
const ACCOUNT_REGISTRATION_PROTOCOL: &[u8] = b"restos-device-registration-v2/account-registration";
const DB_NAME: &str = "restos_account_device";
const STORE_NAME: &str = "device_identity";
const RECORD_KEY: &str = "primary";
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInstanceId(Uuid);

impl AppInstanceId {
    pub fn parse(value: &str) -> Result<Self, DeviceIdentityError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| DeviceIdentityError::InvalidUuid)
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Debug for AppInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AppInstanceId(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DevicePublicKeySpki(Vec<u8>);

impl DevicePublicKeySpki {
    pub fn new(bytes: Vec<u8>) -> Result<Self, DeviceIdentityError> {
        if bytes.is_empty() {
            return Err(DeviceIdentityError::CorruptedRecord);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for DevicePublicKeySpki {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DevicePublicKeySpki(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSignatureDer(Vec<u8>);

impl DeviceSignatureDer {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for DeviceSignatureDer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DeviceSignatureDer(<redacted>)")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentityMetadata {
    pub schema_version: u32,
    pub app_instance_id: AppInstanceId,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceIdentityError {
    BrowserUnavailable,
    CryptoUnavailable,
    IndexedDbUnavailable,
    OpenBlocked,
    MissingStore,
    CorruptedRecord,
    InvalidUuid,
    InvalidPlatform,
    InvalidNonce,
    FieldTooLarge,
    InvalidRawSignatureLength,
    CryptoOperationFailed,
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct DeviceIdentity {
    metadata: DeviceIdentityMetadata,
    private_key: CryptoKey,
    public_key: CryptoKey,
    public_spki: DevicePublicKeySpki,
}

#[cfg(target_arch = "wasm32")]
impl fmt::Debug for DeviceIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceIdentity")
            .field("metadata", &self.metadata)
            .field("key_material", &"<redacted>")
            .finish()
    }
}

#[cfg(target_arch = "wasm32")]
impl DeviceIdentity {
    pub fn metadata(&self) -> &DeviceIdentityMetadata {
        &self.metadata
    }

    pub fn public_spki(&self) -> &DevicePublicKeySpki {
        &self.public_spki
    }

    pub async fn sign(&self, message: &[u8]) -> Result<DeviceSignatureDer, DeviceIdentityError> {
        let raw = JsFuture::from(sign_p256(&self.private_key, &Uint8Array::from(message)))
            .await
            .map_err(|_| DeviceIdentityError::CryptoOperationFailed)?;
        let bytes = Uint8Array::new(&raw).to_vec();
        raw_p256_signature_to_der(&bytes)
    }

    pub fn private_key_is_non_extractable_sign_only(&self) -> bool {
        if self.private_key.extractable() {
            return false;
        }
        let usages: Array = self.private_key.usages();
        usages.length() == 1 && usages.get(0).as_string().as_deref() == Some("sign")
    }
}

#[derive(Clone, Default)]
pub struct DeviceIdentityAdapter;

impl DeviceIdentityAdapter {
    pub fn new() -> Self {
        Self
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_or_create(&self) -> Result<DeviceIdentity, DeviceIdentityError> {
        let record = JsFuture::from(load_or_create_device_identity(
            DB_NAME,
            STORE_NAME,
            RECORD_KEY,
            SCHEMA_VERSION,
        ))
        .await
        .map_err(classify_js_error)?;
        identity_from_record(&record)
    }
}

#[cfg(target_arch = "wasm32")]
fn identity_from_record(record: &JsValue) -> Result<DeviceIdentity, DeviceIdentityError> {
    let schema_version = reflect(record, "schemaVersion")?
        .as_f64()
        .filter(|value| *value == SCHEMA_VERSION as f64)
        .ok_or(DeviceIdentityError::CorruptedRecord)? as u32;
    let app_instance_id = reflect(record, "appInstanceId")?
        .as_string()
        .ok_or(DeviceIdentityError::CorruptedRecord)
        .and_then(|value| AppInstanceId::parse(&value))?;
    let created_at = reflect(record, "createdAt")?
        .as_string()
        .filter(|value| !value.is_empty())
        .ok_or(DeviceIdentityError::CorruptedRecord)?;
    let private_key = reflect(record, "privateKey")?
        .dyn_into::<CryptoKey>()
        .map_err(|_| DeviceIdentityError::CorruptedRecord)?;
    let public_key = reflect(record, "publicKey")?
        .dyn_into::<CryptoKey>()
        .map_err(|_| DeviceIdentityError::CorruptedRecord)?;
    let public_spki_value = reflect(record, "publicSpki")?;
    if !public_spki_value.is_instance_of::<Uint8Array>() {
        return Err(DeviceIdentityError::CorruptedRecord);
    }
    let public_spki = DevicePublicKeySpki::new(Uint8Array::new(&public_spki_value).to_vec())?;
    let identity = DeviceIdentity {
        metadata: DeviceIdentityMetadata {
            schema_version,
            app_instance_id,
            created_at,
        },
        private_key,
        public_key,
        public_spki,
    };
    if !identity.private_key_is_non_extractable_sign_only()
        || !key_uses_p256_ecdsa(&identity.private_key)?
        || !key_uses_p256_ecdsa(&identity.public_key)?
    {
        return Err(DeviceIdentityError::CorruptedRecord);
    }
    Ok(identity)
}

#[cfg(target_arch = "wasm32")]
fn key_uses_p256_ecdsa(key: &CryptoKey) -> Result<bool, DeviceIdentityError> {
    let algorithm = key
        .algorithm()
        .map_err(|_| DeviceIdentityError::CorruptedRecord)?;
    let name = js_sys::Reflect::get(algorithm.as_ref(), &JsValue::from_str("name"))
        .map_err(|_| DeviceIdentityError::CorruptedRecord)?
        .as_string()
        .ok_or(DeviceIdentityError::CorruptedRecord)?;
    let named_curve = js_sys::Reflect::get(algorithm.as_ref(), &JsValue::from_str("namedCurve"))
        .map_err(|_| DeviceIdentityError::CorruptedRecord)?
        .as_string()
        .ok_or(DeviceIdentityError::CorruptedRecord)?;
    Ok(name == "ECDSA" && named_curve == "P-256")
}

#[cfg(target_arch = "wasm32")]
fn reflect(record: &JsValue, name: &str) -> Result<JsValue, DeviceIdentityError> {
    js_sys::Reflect::get(record, &JsValue::from_str(name))
        .map_err(|_| DeviceIdentityError::CorruptedRecord)
}

#[cfg(target_arch = "wasm32")]
fn classify_js_error(error: JsValue) -> DeviceIdentityError {
    match error.as_string().as_deref() {
        Some("blocked") => DeviceIdentityError::OpenBlocked,
        Some("missing-store") => DeviceIdentityError::MissingStore,
        Some("crypto-unavailable") => DeviceIdentityError::CryptoUnavailable,
        _ => DeviceIdentityError::IndexedDbUnavailable,
    }
}

pub struct CanonicalDeviceProof<'a> {
    pub device_challenge_id: &'a str,
    pub invitation_id: &'a str,
    pub phone_challenge_id: &'a str,
    pub app_instance_id: AppInstanceId,
    pub platform: &'a str,
    pub nonce_base64url: &'a str,
}

pub fn canonical_device_proof_message(
    input: CanonicalDeviceProof<'_>,
) -> Result<Vec<u8>, DeviceIdentityError> {
    if !input.platform.is_ascii() || input.platform.is_empty() {
        return Err(DeviceIdentityError::InvalidPlatform);
    }
    let device_challenge_id =
        Uuid::parse_str(input.device_challenge_id).map_err(|_| DeviceIdentityError::InvalidUuid)?;
    let invitation_id =
        Uuid::parse_str(input.invitation_id).map_err(|_| DeviceIdentityError::InvalidUuid)?;
    let phone_challenge_id =
        Uuid::parse_str(input.phone_challenge_id).map_err(|_| DeviceIdentityError::InvalidUuid)?;
    let nonce = decode_base64url(input.nonce_base64url)?;
    if nonce.is_empty() {
        return Err(DeviceIdentityError::InvalidNonce);
    }

    let fields: [&[u8]; 7] = [
        PROTOCOL,
        device_challenge_id.as_bytes(),
        invitation_id.as_bytes(),
        phone_challenge_id.as_bytes(),
        input.app_instance_id.0.as_bytes(),
        input.platform.as_bytes(),
        &nonce,
    ];
    let mut message = Vec::new();
    for field in fields {
        let length = u32::try_from(field.len()).map_err(|_| DeviceIdentityError::FieldTooLarge)?;
        message.extend_from_slice(&length.to_be_bytes());
        message.extend_from_slice(field);
    }
    Ok(message)
}

pub struct CanonicalAccountRegistrationDeviceProof<'a> {
    pub device_challenge_id: &'a str,
    pub phone_challenge_id: &'a str,
    pub app_instance_id: AppInstanceId,
    pub platform: &'a str,
    pub nonce_base64url: &'a str,
}

pub fn canonical_account_registration_device_proof_message(
    input: CanonicalAccountRegistrationDeviceProof<'_>,
) -> Result<Vec<u8>, DeviceIdentityError> {
    if !input.platform.is_ascii() || input.platform.is_empty() {
        return Err(DeviceIdentityError::InvalidPlatform);
    }
    let device_challenge_id =
        Uuid::parse_str(input.device_challenge_id).map_err(|_| DeviceIdentityError::InvalidUuid)?;
    let phone_challenge_id =
        Uuid::parse_str(input.phone_challenge_id).map_err(|_| DeviceIdentityError::InvalidUuid)?;
    let nonce = decode_base64url(input.nonce_base64url)?;
    if nonce.is_empty() {
        return Err(DeviceIdentityError::InvalidNonce);
    }

    let fields: [&[u8]; 6] = [
        ACCOUNT_REGISTRATION_PROTOCOL,
        device_challenge_id.as_bytes(),
        phone_challenge_id.as_bytes(),
        input.app_instance_id.0.as_bytes(),
        input.platform.as_bytes(),
        &nonce,
    ];
    let mut message = Vec::new();
    for field in fields {
        let length = u32::try_from(field.len()).map_err(|_| DeviceIdentityError::FieldTooLarge)?;
        message.extend_from_slice(&length.to_be_bytes());
        message.extend_from_slice(field);
    }
    Ok(message)
}

pub fn raw_p256_signature_to_der(raw: &[u8]) -> Result<DeviceSignatureDer, DeviceIdentityError> {
    if raw.len() != 64 {
        return Err(DeviceIdentityError::InvalidRawSignatureLength);
    }
    let r = der_integer(&raw[..32]);
    let s = der_integer(&raw[32..]);
    let sequence_length = 2 + r.len() + 2 + s.len();
    debug_assert!(sequence_length < 128);
    let mut der = Vec::with_capacity(sequence_length + 2);
    der.extend_from_slice(&[0x30, sequence_length as u8, 0x02, r.len() as u8]);
    der.extend_from_slice(&r);
    der.extend_from_slice(&[0x02, s.len() as u8]);
    der.extend_from_slice(&s);
    Ok(DeviceSignatureDer(der))
}

fn der_integer(component: &[u8]) -> Vec<u8> {
    let first_nonzero = component
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(component.len() - 1);
    let trimmed = &component[first_nonzero..];
    let mut result = Vec::with_capacity(trimmed.len() + 1);
    if trimmed[0] & 0x80 != 0 {
        result.push(0);
    }
    result.extend_from_slice(trimmed);
    result
}

fn decode_base64url(value: &str) -> Result<Vec<u8>, DeviceIdentityError> {
    if value.is_empty() || value.bytes().any(|byte| !base64url_value(byte).is_some()) {
        return Err(DeviceIdentityError::InvalidNonce);
    }
    if value.len() % 4 == 1 {
        return Err(DeviceIdentityError::InvalidNonce);
    }
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        accumulator = (accumulator << 6) | u32::from(base64url_value(byte).unwrap());
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    if bits > 0 && accumulator != 0 {
        return Err(DeviceIdentityError::InvalidNonce);
    }
    Ok(output)
}

fn base64url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
let pendingIdentity = null;

function openIdentityDb(name, store) {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(name, 1);
    request.onblocked = () => reject("blocked");
    request.onerror = () => reject("open-error");
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(store)) db.createObjectStore(store);
    };
    request.onsuccess = () => {
      const db = request.result;
      db.onversionchange = () => db.close();
      if (!db.objectStoreNames.contains(store)) {
        db.close();
        reject("missing-store");
      } else {
        resolve(db);
      }
    };
  });
}

function readIdentity(db, store, key) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readonly");
    const request = tx.objectStore(store).get(key);
    request.onerror = () => reject("read-error");
    request.onsuccess = () => resolve(request.result || null);
    tx.onabort = () => reject("read-abort");
  });
}

function addIdentity(db, store, key, value) {
  return new Promise((resolve, reject) => {
    const tx = db.transaction(store, "readwrite");
    tx.objectStore(store).add(value, key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => {};
    tx.onabort = () => reject("write-conflict");
  });
}

function secureUuidV4() {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map(x => x.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0,8)}-${hex.slice(8,12)}-${hex.slice(12,16)}-${hex.slice(16,20)}-${hex.slice(20)}`;
}

export function load_or_create_device_identity(dbName, store, key, schemaVersion) {
  if (pendingIdentity) return pendingIdentity;
  pendingIdentity = (async () => {
    if (!globalThis.crypto || !crypto.subtle) throw "crypto-unavailable";
    const db = await openIdentityDb(dbName, store);
    try {
      const existing = await readIdentity(db, store, key);
      if (existing) return existing;
      const pair = await crypto.subtle.generateKey(
        { name: "ECDSA", namedCurve: "P-256" },
        false,
        ["sign"]
      );
      if (pair.privateKey.extractable || pair.privateKey.usages.length !== 1 ||
          pair.privateKey.usages[0] !== "sign") throw "invalid-private-key";
      const spki = new Uint8Array(await crypto.subtle.exportKey("spki", pair.publicKey));
      const record = {
        schemaVersion,
        appInstanceId: secureUuidV4(),
        privateKey: pair.privateKey,
        publicKey: pair.publicKey,
        publicSpki: spki,
        createdAt: new Date().toISOString(),
      };
      try {
        await addIdentity(db, store, key, record);
      } catch (_) {
        const winner = await readIdentity(db, store, key);
        if (winner) return winner;
        throw "write-error";
      }
      const committed = await readIdentity(db, store, key);
      if (!committed) throw "write-error";
      return committed;
    } finally {
      db.close();
    }
  })().finally(() => { pendingIdentity = null; });
  return pendingIdentity;
}

export function sign_p256(privateKey, message) {
  return crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, privateKey, message);
}

export function export_private_key_for_test(privateKey) {
  return crypto.subtle.exportKey("pkcs8", privateKey);
}
"#)]
extern "C" {
    fn load_or_create_device_identity(
        db_name: &str,
        store: &str,
        key: &str,
        schema_version: u32,
    ) -> Promise;
    fn sign_p256(private_key: &CryptoKey, message: &Uint8Array) -> Promise;
    #[cfg(test)]
    fn export_private_key_for_test(private_key: &CryptoKey) -> Promise;
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE: &str = "00112233-4455-6677-8899-aabbccddeeff";
    const INVITATION: &str = "10213243-5465-7687-98a9-bacbdcedfe0f";
    const PHONE: &str = "ffeeddcc-bbaa-9988-7766-554433221100";
    const APP: &str = "01234567-89ab-4def-8123-456789abcdef";

    fn proof<'a>(platform: &'a str, nonce: &'a str) -> CanonicalDeviceProof<'a> {
        CanonicalDeviceProof {
            device_challenge_id: DEVICE,
            invitation_id: INVITATION,
            phone_challenge_id: PHONE,
            app_instance_id: AppInstanceId::parse(APP).unwrap(),
            platform,
            nonce_base64url: nonce,
        }
    }

    #[test]
    fn canonical_message_matches_backend_vector_and_uuid_network_order() {
        let message = canonical_device_proof_message(proof(
            "ios",
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
        ))
        .unwrap();
        assert_eq!(&message[0..4], &(29u32.to_be_bytes()));
        assert_eq!(&message[4..33], PROTOCOL);
        assert_eq!(
            &message[37..53],
            Uuid::parse_str(DEVICE).unwrap().as_bytes()
        );
        assert_eq!(hex(&message), "0000001d726573746f732d6465766963652d726567697374726174696f6e2d76310000001000112233445566778899aabbccddeeff00000010102132435465768798a9bacbdcedfe0f00000010ffeeddccbbaa99887766554433221100000000100123456789ab4def8123456789abcdef00000003696f7300000020000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
    }

    #[test]
    fn canonical_fields_are_ordered_and_domain_bound() {
        let baseline = canonical_device_proof_message(proof("ios", "AA")).unwrap();
        let changed_platform = canonical_device_proof_message(proof("web", "AA")).unwrap();
        let changed_nonce = canonical_device_proof_message(proof("ios", "AQ")).unwrap();
        assert_ne!(baseline, changed_platform);
        assert_ne!(baseline, changed_nonce);
        assert!(canonical_device_proof_message(proof("iös", "AA")).is_err());
        assert!(canonical_device_proof_message(proof("ios", "A")).is_err());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn standalone_registration_has_separate_versioned_context() {
        let standalone = canonical_account_registration_device_proof_message(
            CanonicalAccountRegistrationDeviceProof {
                device_challenge_id: DEVICE,
                phone_challenge_id: PHONE,
                app_instance_id: AppInstanceId::parse(APP).unwrap(),
                platform: "web",
                nonce_base64url: "AA",
            },
        )
        .unwrap();
        let invitation = canonical_device_proof_message(proof("web", "AA")).unwrap();
        assert_ne!(standalone, invitation);
        assert!(standalone
            .windows(ACCOUNT_REGISTRATION_PROTOCOL.len())
            .any(|window| window == ACCOUNT_REGISTRATION_PROTOCOL));
        assert!(!standalone
            .windows(INVITATION.len())
            .any(|window| window == INVITATION.as_bytes()));
    }

    #[test]
    fn raw_signature_der_vectors_cover_sign_and_zero_rules() {
        let ordinary = [1u8; 64];
        let der = raw_p256_signature_to_der(&ordinary).unwrap();
        assert_eq!(der.as_bytes()[0], 0x30);
        assert_eq!(der.as_bytes().len(), 70);

        let mut high_r = [1u8; 64];
        high_r[0] = 0x80;
        assert_eq!(
            raw_p256_signature_to_der(&high_r).unwrap().as_bytes().len(),
            71
        );
        let mut high_s = [1u8; 64];
        high_s[32] = 0x80;
        assert_eq!(
            raw_p256_signature_to_der(&high_s).unwrap().as_bytes().len(),
            71
        );

        let mut leading_zero = [1u8; 64];
        leading_zero[0] = 0;
        assert_eq!(
            raw_p256_signature_to_der(&leading_zero)
                .unwrap()
                .as_bytes()
                .len(),
            69
        );
        let mut zero_r = [1u8; 64];
        zero_r[..32].fill(0);
        assert_eq!(raw_p256_signature_to_der(&zero_r).unwrap().as_bytes()[3], 1);
    }

    #[test]
    fn raw_signature_rejects_non_p256_lengths_and_is_deterministic() {
        assert_eq!(
            raw_p256_signature_to_der(&[0; 63]),
            Err(DeviceIdentityError::InvalidRawSignatureLength)
        );
        assert_eq!(
            raw_p256_signature_to_der(&[0; 65]),
            Err(DeviceIdentityError::InvalidRawSignatureLength)
        );
        let raw = [0x42; 64];
        assert_eq!(
            raw_p256_signature_to_der(&raw).unwrap(),
            raw_p256_signature_to_der(&raw).unwrap()
        );
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod browser_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test(async)]
    async fn identity_is_non_extractable_persistent_and_signs_after_read() {
        let adapter = DeviceIdentityAdapter::new();
        let first = adapter.get_or_create().await.unwrap();
        assert!(first.private_key_is_non_extractable_sign_only());
        assert!(!first.public_spki().as_bytes().is_empty());
        assert!(
            JsFuture::from(export_private_key_for_test(&first.private_key))
                .await
                .is_err()
        );
        let signature = first.sign(b"browser-vector").await.unwrap();
        assert!(signature.as_bytes().len() >= 68);

        let second = adapter.get_or_create().await.unwrap();
        assert_eq!(
            first.metadata().app_instance_id,
            second.metadata().app_instance_id
        );
        assert!(second.sign(b"after-indexeddb-read").await.is_ok());
    }

    #[wasm_bindgen_test(async)]
    async fn concurrent_get_or_create_is_single_flight() {
        let promises = Array::new();
        promises.push(&load_or_create_device_identity(
            DB_NAME,
            STORE_NAME,
            RECORD_KEY,
            SCHEMA_VERSION,
        ));
        promises.push(&load_or_create_device_identity(
            DB_NAME,
            STORE_NAME,
            RECORD_KEY,
            SCHEMA_VERSION,
        ));
        let records = JsFuture::from(Promise::all(&promises)).await.unwrap();
        let records = Array::from(&records);
        let left = identity_from_record(&records.get(0)).unwrap();
        let right = identity_from_record(&records.get(1)).unwrap();
        assert_eq!(
            left.metadata().app_instance_id,
            right.metadata().app_instance_id
        );
    }
}
