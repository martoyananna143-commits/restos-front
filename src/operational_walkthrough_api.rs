//! Typed Account-only operational walkthrough client.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::{account_api::AccountAccessToken, assessment_attempt_api::AttemptDocument};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationalWalkthroughApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    Conflict,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OperationalWalkthroughTemplate {
    pub template_id: Uuid,
    pub template_version_id: Uuid,
    pub name: String,
    pub version: i32,
    pub section_count: usize,
    pub item_count: usize,
    pub scoring_algorithm: String,
    pub scoring_ready: bool,
}

#[derive(Serialize)]
struct StartRequest {
    venue_id: Uuid,
    template_version_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct OperationalWalkthroughApiClient {
    base_url: String,
}

impl OperationalWalkthroughApiClient {
    pub fn new(base_url: String) -> Result<Self, OperationalWalkthroughApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(OperationalWalkthroughApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 2] {
        [
            "/api/v1/account/companies/{company_id}/operational-walkthroughs/templates",
            "/api/v1/account/companies/{company_id}/operational-walkthroughs",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn templates(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<OperationalWalkthroughTemplate>, OperationalWalkthroughApiError> {
        let response = self
            .request(
                Request::get(&format!(
                    "{}/api/v1/account/companies/{company_id}/operational-walkthroughs/templates",
                    self.base_url
                )),
                token,
            )
            .send()
            .await
            .map_err(|_| OperationalWalkthroughApiError::NetworkUnavailable)?;
        parse(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn start(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        venue_id: Uuid,
        template_version_id: Uuid,
    ) -> Result<AttemptDocument, OperationalWalkthroughApiError> {
        let response = self
            .request(
                Request::post(&format!(
                    "{}/api/v1/account/companies/{company_id}/operational-walkthroughs",
                    self.base_url
                )),
                token,
            )
            .json(&StartRequest {
                venue_id,
                template_version_id,
            })
            .map_err(|_| OperationalWalkthroughApiError::InternalError)?
            .send()
            .await
            .map_err(|_| OperationalWalkthroughApiError::NetworkUnavailable)?;
        parse(response).await
    }

    #[cfg(target_arch = "wasm32")]
    fn request(
        &self,
        request: gloo_net::http::RequestBuilder,
        token: &AccountAccessToken,
    ) -> gloo_net::http::RequestBuilder {
        request
            .credentials(RequestCredentials::Include)
            .header("Cache-Control", "no-cache")
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse<T: serde::de::DeserializeOwned>(
    response: Response,
) -> Result<T, OperationalWalkthroughApiError> {
    if response.ok() {
        return response
            .json()
            .await
            .map_err(|_| OperationalWalkthroughApiError::InternalError);
    }
    Err(match response.status() {
        401 => OperationalWalkthroughApiError::AuthenticationRequired,
        403 => OperationalWalkthroughApiError::PermissionDenied,
        404 => OperationalWalkthroughApiError::NotFound,
        409 => OperationalWalkthroughApiError::Conflict,
        _ => OperationalWalkthroughApiError::InternalError,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn operational_walkthrough_routes_are_exact() {
        assert_eq!(OperationalWalkthroughApiClient::routes().len(), 2);
        assert!(OperationalWalkthroughApiClient::routes()[0].ends_with("/templates"));
    }

    #[wasm_bindgen_test]
    fn operational_template_contract_is_strict() {
        let value = r#"{"template_id":"00000000-0000-0000-0000-000000000001","template_version_id":"00000000-0000-0000-0000-000000000002","name":"Обход","version":1,"section_count":2,"item_count":10,"scoring_algorithm":"weighted_v1","scoring_ready":true}"#;
        assert!(serde_json::from_str::<OperationalWalkthroughTemplate>(value).is_ok());
        assert!(serde_json::from_str::<OperationalWalkthroughTemplate>(
            &value.replace("}", ",\"employee_name\":\"x\"}")
        )
        .is_err());
    }
}
