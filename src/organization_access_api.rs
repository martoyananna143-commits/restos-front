//! Typed owner-only client for organization Venue and employee access management.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrganizationAccessApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    Conflict,
    InvalidRequest,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationVenue {
    pub created: bool,
    pub venue_id: Uuid,
    pub name: String,
    pub code: String,
    pub timezone: Option<String>,
    pub status: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationAccessProfile {
    Owner,
    EmployeeUnassigned,
    EmployeeVenue,
    VenueManager,
    OrganizationManager,
    Unsupported,
}

impl OrganizationAccessProfile {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Owner => "Владелец",
            Self::EmployeeUnassigned => "Сотрудник без ресторана",
            Self::EmployeeVenue => "Сотрудник ресторана",
            Self::VenueManager => "Менеджер ресторанов",
            Self::OrganizationManager => "Менеджер организации",
            Self::Unsupported => "Неподдерживаемый профиль",
        }
    }

    pub const fn wire(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::EmployeeUnassigned => "employee_unassigned",
            Self::EmployeeVenue => "employee_venue",
            Self::VenueManager => "venue_manager",
            Self::OrganizationManager => "organization_manager",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationEmployeeAccess {
    pub employee_profile_id: Uuid,
    pub display_name: String,
    pub employment_status: String,
    pub position_id: Uuid,
    pub position_name: String,
    pub profile: OrganizationAccessProfile,
    pub venue_ids: Vec<Uuid>,
    pub revision: String,
    pub editable: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateOrganizationVenueRequest {
    pub request_id: Uuid,
    pub name: String,
    pub timezone: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceOrganizationAccessRequest {
    pub profile: OrganizationAccessProfile,
    pub venue_ids: Vec<Uuid>,
    pub expected_revision: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceOrganizationPositionRequest {
    pub position_id: Uuid,
    pub expected_revision: String,
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
pub struct OrganizationAccessApiClient {
    base_url: String,
}

impl OrganizationAccessApiClient {
    pub fn new(base_url: String) -> Result<Self, OrganizationAccessApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(OrganizationAccessApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 6] {
        [
            "/api/v1/account/companies/{company_id}/organization/venues",
            "/api/v1/account/companies/{company_id}/organization/venues",
            "/api/v1/account/companies/{company_id}/organization/employees",
            "/api/v1/account/companies/{company_id}/organization/employees/{employee_profile_id}/access",
            "/api/v1/account/companies/{company_id}/organization/employees/{employee_profile_id}/position",
            "/api/v1/account/companies/{company_id}/organization/employees/{employee_profile_id}/access",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn venues(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<OrganizationVenue>, OrganizationAccessApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/organization/venues"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_venue(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreateOrganizationVenueRequest,
    ) -> Result<OrganizationVenue, OrganizationAccessApiError> {
        let request = self
            .authenticated(
                Request::post(&format!(
                    "{}/api/v1/account/companies/{company_id}/organization/venues",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| OrganizationAccessApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn employees(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<OrganizationEmployeeAccess>, OrganizationAccessApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/organization/employees"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn replace_access(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        employee_profile_id: Uuid,
        body: &ReplaceOrganizationAccessRequest,
    ) -> Result<OrganizationEmployeeAccess, OrganizationAccessApiError> {
        let request = self
            .authenticated(
                Request::put(&format!(
                    "{}/api/v1/account/companies/{company_id}/organization/employees/{employee_profile_id}/access",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| OrganizationAccessApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn replace_position(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        employee_profile_id: Uuid,
        body: &ReplaceOrganizationPositionRequest,
    ) -> Result<OrganizationEmployeeAccess, OrganizationAccessApiError> {
        let request = self
            .authenticated(
                Request::put(&format!(
                    "{}/api/v1/account/companies/{company_id}/organization/employees/{employee_profile_id}/position",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| OrganizationAccessApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, OrganizationAccessApiError> {
        let request =
            self.authenticated(Request::get(&format!("{}{}", self.base_url, path)), token);
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    fn authenticated(
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
    ) -> Result<T, OrganizationAccessApiError> {
        let response = response.map_err(|_| OrganizationAccessApiError::NetworkUnavailable)?;
        if response.ok() {
            return response
                .json::<T>()
                .await
                .map_err(|_| OrganizationAccessApiError::InternalError);
        }
        let status = response.status();
        let code = response
            .json::<ErrorEnvelope>()
            .await
            .ok()
            .map(|value| value.detail.code);
        Err(map_error(status, code.as_deref()))
    }
}

fn map_error(status: u16, code: Option<&str>) -> OrganizationAccessApiError {
    match (status, code) {
        (401, _) => OrganizationAccessApiError::AuthenticationRequired,
        (403, _) => OrganizationAccessApiError::PermissionDenied,
        (404, Some("organization_resource_not_found")) => OrganizationAccessApiError::NotFound,
        (409, Some("organization_revision_conflict")) => OrganizationAccessApiError::Conflict,
        (400 | 422, _) => OrganizationAccessApiError::InvalidRequest,
        (404, _) => OrganizationAccessApiError::NotFound,
        _ => OrganizationAccessApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn owner_organization_routes_are_exact_and_account_only() {
        let routes = OrganizationAccessApiClient::routes();
        assert_eq!(routes.len(), 6);
        assert!(routes
            .iter()
            .all(|value| value.starts_with("/api/v1/account/")));
        assert_eq!(
            routes
                .iter()
                .filter(|value| value.ends_with("/venues"))
                .count(),
            2
        );
    }

    #[wasm_bindgen_test]
    fn access_requests_are_strict_and_do_not_contain_auth_or_private_fields() {
        let body = ReplaceOrganizationAccessRequest {
            profile: OrganizationAccessProfile::VenueManager,
            venue_ids: vec![Uuid::from_u128(1)],
            expected_revision: "2026-08-14T10:00:00Z".into(),
        };
        let value = serde_json::to_value(body).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["profile"], "venue_manager");
        for forbidden in ["token", "cookie", "phone", "account_id", "permissions"] {
            assert!(value.get(forbidden).is_none());
        }

        let position = ReplaceOrganizationPositionRequest {
            position_id: Uuid::from_u128(2),
            expected_revision: "2026-08-14T10:00:00Z".into(),
        };
        let value = serde_json::to_value(position).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        for forbidden in ["token", "cookie", "phone", "permissions"] {
            assert!(value.get(forbidden).is_none());
        }
    }

    #[wasm_bindgen_test]
    fn error_mapping_keeps_stale_conflict_distinct_and_bounded() {
        assert_eq!(
            map_error(409, Some("organization_revision_conflict")),
            OrganizationAccessApiError::Conflict
        );
        assert_eq!(
            map_error(404, Some("organization_resource_not_found")),
            OrganizationAccessApiError::NotFound
        );
        assert_eq!(
            map_error(500, Some("private-message")),
            OrganizationAccessApiError::InternalError
        );
    }
}
