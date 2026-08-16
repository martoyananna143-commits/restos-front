//! Typed Account-only client for employee invitation onboarding.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkforceApiError {
    AuthenticationRequired,
    PermissionDenied,
    InvalidRequest,
    Conflict,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkforceVenue {
    pub venue_id: Uuid,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkforceInvitationRequest {
    pub request_id: Uuid,
    pub employee_name: String,
    pub phone: String,
    pub venue_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkforceInvitationResponse {
    pub created: bool,
    pub invitation_id: Uuid,
    pub employee_profile_id: Uuid,
    pub invitation_code: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptWorkforceInvitationRequest {
    pub invitation_code: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcceptWorkforceInvitationResponse {
    pub joined: bool,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    detail: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    code: String,
}

#[derive(Clone, Debug)]
pub struct WorkforceApiClient {
    base_url: String,
}

impl WorkforceApiClient {
    pub fn new(base_url: String) -> Result<Self, WorkforceApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(WorkforceApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    #[allow(dead_code)]
    pub const fn routes() -> [&'static str; 3] {
        [
            "/api/v1/account/companies/{company_id}/workforce/venues",
            "/api/v1/account/companies/{company_id}/workforce/invitations",
            "/api/v1/account/workforce/invitations/accept",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn accept_invitation(
        &self,
        token: &AccountAccessToken,
        body: &AcceptWorkforceInvitationRequest,
    ) -> Result<AcceptWorkforceInvitationResponse, WorkforceApiError> {
        let request = self
            .authenticated_request(
                Request::post(&format!(
                    "{}/api/v1/account/workforce/invitations/accept",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| WorkforceApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn venues(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<WorkforceVenue>, WorkforceApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/workforce/venues"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_invitation(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreateWorkforceInvitationRequest,
    ) -> Result<WorkforceInvitationResponse, WorkforceApiError> {
        let request = self
            .authenticated_request(
                Request::post(&format!(
                    "{}/api/v1/account/companies/{company_id}/workforce/invitations",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| WorkforceApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, WorkforceApiError> {
        let request =
            self.authenticated_request(Request::get(&format!("{}{}", self.base_url, path)), token);
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    fn authenticated_request(
        &self,
        request: gloo_net::http::RequestBuilder,
        token: &AccountAccessToken,
    ) -> gloo_net::http::RequestBuilder {
        request
            .credentials(RequestCredentials::Include)
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .header("X-RestOS-Web-Session", "1")
    }

    #[cfg(target_arch = "wasm32")]
    async fn decode<T: DeserializeOwned>(
        &self,
        response: Result<Response, gloo_net::Error>,
    ) -> Result<T, WorkforceApiError> {
        let response = response.map_err(|_| WorkforceApiError::NetworkUnavailable)?;
        if response.ok() {
            return response
                .json::<T>()
                .await
                .map_err(|_| WorkforceApiError::InternalError);
        }
        let status = response.status();
        let code = response
            .json::<ErrorEnvelope>()
            .await
            .ok()
            .map(|value| value.detail.code);
        Err(match (status, code.as_deref()) {
            (401, _) => WorkforceApiError::AuthenticationRequired,
            (403, Some("permission_denied")) => WorkforceApiError::PermissionDenied,
            (409, Some("workforce_invitation_conflict" | "invitation_conflict")) => {
                WorkforceApiError::Conflict
            }
            (422, Some("invalid_workforce_invitation")) => WorkforceApiError::InvalidRequest,
            (400..=499, _) => WorkforceApiError::InvalidRequest,
            _ => WorkforceApiError::InternalError,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "wasm32")]
    use js_sys::{Function, Reflect, JSON};
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen::JsValue;

    #[cfg(target_arch = "wasm32")]
    struct FetchGuard {
        original: JsValue,
    }

    #[cfg(target_arch = "wasm32")]
    impl Drop for FetchGuard {
        fn drop(&mut self) {
            let global = js_sys::global();
            let _ = Reflect::set(&global, &JsValue::from_str("fetch"), &self.original);
            let _ =
                Reflect::delete_property(&global, &JsValue::from_str("__restosWorkforceRequests"));
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn install_fetch_observer(status: u16) -> FetchGuard {
        let global = js_sys::global();
        let original =
            Reflect::get(&global, &JsValue::from_str("fetch")).expect("browser fetch must exist");
        let script = format!(
            r#"
const request = input instanceof Request ? input : new Request(input, init);
const record = (body) => {{
  globalThis.__restosWorkforceRequests.push({{
    method: request.method,
    url: request.url,
    authorization: request.headers.get("Authorization"),
    bodyHasToken: body.includes("synthetic-token"),
    bodyHasPhoneField: body.includes('"phone"')
  }});
  let payload;
  if ({status} === 401) {{
    payload = JSON.stringify({{detail: {{code: "authentication_required", private: "must-not-escape"}}}});
  }} else if (request.method === "GET") {{
    payload = JSON.stringify([{{venue_id: "00000000-0000-0000-0000-000000000002", name: "Synthetic Venue"}}]);
  }} else if (request.url.endsWith("/accept")) {{
    payload = JSON.stringify({{joined: true}});
  }} else {{
    payload = JSON.stringify({{
      created: true,
      invitation_id: "00000000-0000-0000-0000-000000000003",
      employee_profile_id: "00000000-0000-0000-0000-000000000004",
      invitation_code: "123456",
      expires_at: "2026-08-17T12:00:00Z"
    }});
  }}
  return new Response(payload, {{status: {status}, headers: {{"Content-Type": "application/json"}}}});
}};
return request.method === "GET"
  ? Promise.resolve(record(""))
  : request.clone().text().then(record);
"#
        );
        let observer = Function::new_with_args("input, init", &script);
        Reflect::set(
            &global,
            &JsValue::from_str("__restosWorkforceRequests"),
            &js_sys::Array::new(),
        )
        .expect("request evidence must initialize");
        Reflect::set(&global, &JsValue::from_str("fetch"), &observer)
            .expect("fetch observer must install");
        FetchGuard { original }
    }

    #[cfg(target_arch = "wasm32")]
    fn request_evidence() -> serde_json::Value {
        let evidence = Reflect::get(
            &js_sys::global(),
            &JsValue::from_str("__restosWorkforceRequests"),
        )
        .expect("request evidence must exist");
        let serialized = JSON::stringify(&evidence)
            .expect("request evidence must serialize")
            .as_string()
            .expect("serialized evidence must be a string");
        serde_json::from_str(&serialized).expect("request evidence must be valid JSON")
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn routes_are_exact_and_account_only() {
        let routes = WorkforceApiClient::routes();
        assert_eq!(routes.len(), 3);
        assert!(routes
            .iter()
            .all(|path| path.starts_with("/api/v1/account/")));
        assert!(routes.iter().all(|path| !path.contains("organization")));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn strict_dto_contains_required_phone_without_auth_material() {
        let request = CreateWorkforceInvitationRequest {
            request_id: Uuid::from_u128(1),
            employee_name: "Synthetic Employee".into(),
            phone: "+79991234567".into(),
            venue_id: None,
        };
        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"phone\":\"+79991234567\""));
        for forbidden in ["otp", "password", "token", "cookie"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test(async)]
    async fn real_workforce_requests_use_one_bearer_header_without_token_leaks() {
        let guard = install_fetch_observer(200);
        let client = WorkforceApiClient::new("https://workforce.test".into()).unwrap();
        let token = AccountAccessToken::from_server("synthetic-token".into()).unwrap();
        let company_id = Uuid::from_u128(1);

        let venues = client.venues(&token, company_id).await;
        let invitation = client
            .create_invitation(
                &token,
                company_id,
                &CreateWorkforceInvitationRequest {
                    request_id: Uuid::from_u128(5),
                    employee_name: "Synthetic Employee".into(),
                    phone: "+79991234567".into(),
                    venue_id: Some(Uuid::from_u128(2)),
                },
            )
            .await;
        let joined = client
            .accept_invitation(
                &token,
                &AcceptWorkforceInvitationRequest {
                    invitation_code: "123456".into(),
                },
            )
            .await;
        let evidence = request_evidence();
        drop(guard);

        assert_eq!(venues.unwrap().len(), 1);
        assert!(invitation.unwrap().created);
        assert!(joined.unwrap().joined);
        let requests = evidence.as_array().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0]["method"], "GET");
        assert_eq!(requests[1]["method"], "POST");
        assert_eq!(requests[2]["method"], "POST");
        for request in requests {
            assert_eq!(request["authorization"], "Bearer synthetic-token");
            assert_ne!(request["authorization"], "synthetic-token");
            assert!(!request["authorization"]
                .as_str()
                .unwrap()
                .contains("Bearer Bearer"));
            assert!(!request["url"].as_str().unwrap().contains("synthetic-token"));
            assert_eq!(request["bodyHasToken"], false);
        }
        assert_eq!(requests[0]["bodyHasPhoneField"], false);
        assert_eq!(requests[1]["bodyHasPhoneField"], true);
        assert_eq!(requests[2]["bodyHasPhoneField"], false);
        assert!(requests[0]["url"].as_str().unwrap().ends_with("/venues"));
        assert!(requests[1]["url"]
            .as_str()
            .unwrap()
            .ends_with("/invitations"));
        assert!(requests[2]["url"].as_str().unwrap().ends_with("/accept"));
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test(async)]
    async fn controlled_401_is_typed_and_does_not_reflect_body_or_token() {
        let guard = install_fetch_observer(401);
        let client = WorkforceApiClient::new("https://workforce.test".into()).unwrap();
        let token = AccountAccessToken::from_server("synthetic-expired-token".into()).unwrap();

        let result = client.venues(&token, Uuid::from_u128(1)).await;
        let evidence = request_evidence();
        drop(guard);

        assert_eq!(result, Err(WorkforceApiError::AuthenticationRequired));
        assert_eq!(evidence.as_array().unwrap().len(), 1);
        let debug = format!("{result:?}");
        assert!(!debug.contains("must-not-escape"));
        assert!(!debug.contains("synthetic-expired-token"));
    }
}
