//! Typed Account-only client for assessment management.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;
use crate::assessment_attempt_api::AttemptDocument;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssessmentManagementApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    DuplicateActiveAssignment,
    AlreadyCompleted,
    StateConflict,
    InvalidRequest,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Assigned,
    InProgress,
    Completed,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagementEmployee {
    pub employee_profile_id: Uuid,
    pub display_name: String,
    pub position_title: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignableTemplate {
    pub template_id: Uuid,
    pub template_version_id: Uuid,
    pub name: String,
    pub activity_type: String,
    pub version: i32,
    pub published_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentEmployee {
    pub employee_profile_id: Uuid,
    pub display_name: String,
    pub position_title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentTemplate {
    pub template_id: Uuid,
    pub template_version_id: Uuid,
    pub name: String,
    pub version: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompletionReceipt {
    pub scoring_algorithm: CompletionAlgorithm,
    pub submitted_at: String,
    pub answered_count: usize,
    pub required_count: usize,
    pub total_count: usize,
    pub scoring_version: Option<u16>,
    pub score_percent: Option<String>,
    pub coverage: Option<String>,
    pub critical_failure_count: Option<usize>,
    pub stop_factor_count: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum CompletionAlgorithm {
    #[serde(rename = "completion_v1")]
    CompletionV1,
    #[serde(rename = "weighted_v1")]
    WeightedV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentProgress {
    pub answered_count: usize,
    pub required_count: usize,
    pub total_count: usize,
    pub started_at: Option<String>,
    pub last_saved_at: Option<String>,
    pub submitted_at: Option<String>,
    pub completion: Option<CompletionReceipt>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagerAssignment {
    pub id: Uuid,
    pub venue_id: Option<Uuid>,
    pub status: AssignmentStatus,
    pub assigned_at: String,
    pub due_at: Option<String>,
    pub revoked_at: Option<String>,
    pub completed_at: Option<String>,
    pub employee: AssignmentEmployee,
    pub template: AssignmentTemplate,
    pub progress: AssignmentProgress,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateAssignmentRequest {
    pub employee_profile_id: Uuid,
    pub template_version_id: Uuid,
    pub venue_id: Option<Uuid>,
    pub due_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StartManagerMeasurementRequest {
    pub employee_profile_id: Uuid,
    pub template_version_id: Uuid,
    pub venue_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ManagementErrorEnvelope {
    detail: ManagementErrorDetail,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ManagementErrorDetail {
    code: String,
}

#[derive(Clone, Debug)]
pub struct AssessmentManagementApiClient {
    base_url: String,
}

impl AssessmentManagementApiClient {
    pub fn new(base_url: String) -> Result<Self, AssessmentManagementApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(AssessmentManagementApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 7] {
        [
            "/api/v1/account/companies/{company_id}/assessment-management/employees",
            "/api/v1/account/companies/{company_id}/assessment-management/templates",
            "/api/v1/account/companies/{company_id}/assessment-management/assignments",
            "/api/v1/account/companies/{company_id}/assessment-management/assignments",
            "/api/v1/account/companies/{company_id}/assessment-management/assignments/{assignment_id}",
            "/api/v1/account/companies/{company_id}/assessment-management/assignments/{assignment_id}/revoke",
            "/api/v1/account/companies/{company_id}/assessment-management/measurements",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn employees(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        q: Option<&str>,
        limit: usize,
        after: Option<Uuid>,
    ) -> Result<Vec<ManagementEmployee>, AssessmentManagementApiError> {
        let mut query = format!("?limit={}", limit.clamp(1, 100));
        if let Some(value) = q.map(str::trim).filter(|value| !value.is_empty()) {
            query.push_str("&q=");
            query.push_str(&urlencoding::encode(value));
        }
        if let Some(value) = after {
            query.push_str("&after=");
            query.push_str(&value.to_string());
        }
        self.get(
            &format!(
                "/api/v1/account/companies/{company_id}/assessment-management/employees{query}"
            ),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn templates(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<AssignableTemplate>, AssessmentManagementApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/assessment-management/templates"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn assignments(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        status: Option<AssignmentStatus>,
        limit: usize,
        after: Option<Uuid>,
    ) -> Result<Vec<ManagerAssignment>, AssessmentManagementApiError> {
        let mut query = format!("?limit={}", limit.clamp(1, 100));
        if let Some(value) = status {
            query.push_str("&status=");
            query.push_str(value.as_wire());
        }
        if let Some(value) = after {
            query.push_str("&after=");
            query.push_str(&value.to_string());
        }
        self.get(
            &format!(
                "/api/v1/account/companies/{company_id}/assessment-management/assignments{query}"
            ),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_assignment(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreateAssignmentRequest,
    ) -> Result<ManagerAssignment, AssessmentManagementApiError> {
        let response = self
            .request(
                Request::post(&format!(
                    "{}/api/v1/account/companies/{company_id}/assessment-management/assignments",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| AssessmentManagementApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AssessmentManagementApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn start_measurement(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &StartManagerMeasurementRequest,
    ) -> Result<AttemptDocument, AssessmentManagementApiError> {
        let response = self
            .request(
                Request::post(&format!(
                    "{}/api/v1/account/companies/{company_id}/assessment-management/measurements",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| AssessmentManagementApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| AssessmentManagementApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn assignment(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        assignment_id: Uuid,
    ) -> Result<ManagerAssignment, AssessmentManagementApiError> {
        self.get(
            &format!(
                "/api/v1/account/companies/{company_id}/assessment-management/assignments/{assignment_id}"
            ),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn revoke(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        assignment_id: Uuid,
    ) -> Result<ManagerAssignment, AssessmentManagementApiError> {
        let response = self
            .request(Request::post(&format!(
                "{}/api/v1/account/companies/{company_id}/assessment-management/assignments/{assignment_id}/revoke",
                self.base_url
            )), token)
            .send()
            .await
            .map_err(|_| AssessmentManagementApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, AssessmentManagementApiError> {
        let response = self
            .request(Request::get(&format!("{}{}", self.base_url, path)), token)
            .send()
            .await
            .map_err(|_| AssessmentManagementApiError::NetworkUnavailable)?;
        parse_response(response).await
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

impl AssignmentStatus {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Assigned => "assigned",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Revoked => "revoked",
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_response<T: DeserializeOwned>(
    response: Response,
) -> Result<T, AssessmentManagementApiError> {
    if response.ok() {
        return response
            .json::<T>()
            .await
            .map_err(|_| AssessmentManagementApiError::InternalError);
    }
    let status = response.status();
    let code = response
        .json::<ManagementErrorEnvelope>()
        .await
        .ok()
        .map(|value| value.detail.code);
    Err(map_error(status, code.as_deref()))
}

fn map_error(status: u16, code: Option<&str>) -> AssessmentManagementApiError {
    match (status, code) {
        (401, _) => AssessmentManagementApiError::AuthenticationRequired,
        (403, Some("permission_denied")) => AssessmentManagementApiError::PermissionDenied,
        (404, Some("assessment_management_not_found")) => AssessmentManagementApiError::NotFound,
        (409, Some("duplicate_active_assignment")) => {
            AssessmentManagementApiError::DuplicateActiveAssignment
        }
        (409, Some("assessment_already_completed")) => {
            AssessmentManagementApiError::AlreadyCompleted
        }
        (409, Some("assessment_state_conflict")) => AssessmentManagementApiError::StateConflict,
        (400 | 422, _) => AssessmentManagementApiError::InvalidRequest,
        (403, _) => AssessmentManagementApiError::PermissionDenied,
        (404, _) => AssessmentManagementApiError::NotFound,
        _ => AssessmentManagementApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    const EMPLOYEE: Uuid = Uuid::from_u128(1);
    const TEMPLATE: Uuid = Uuid::from_u128(2);
    const VERSION: Uuid = Uuid::from_u128(3);
    const ASSIGNMENT: Uuid = Uuid::from_u128(4);

    fn assignment_json() -> String {
        format!(
            r#"{{"id":"{ASSIGNMENT}","venue_id":null,"status":"assigned","assigned_at":"2026-08-08T00:00:00Z","due_at":null,"revoked_at":null,"completed_at":null,"employee":{{"employee_profile_id":"{EMPLOYEE}","display_name":"Сотрудник","position_title":null}},"template":{{"template_id":"{TEMPLATE}","template_version_id":"{VERSION}","name":"Проверка","version":1}},"progress":{{"answered_count":0,"required_count":1,"total_count":2,"started_at":null,"last_saved_at":null,"submitted_at":null,"completion":null}}}}"#
        )
    }

    #[wasm_bindgen_test]
    fn stage23e_declares_exact_seven_routes() {
        assert_eq!(AssessmentManagementApiClient::routes().len(), 7);
        assert_eq!(
            AssessmentManagementApiClient::routes()[0],
            "/api/v1/account/companies/{company_id}/assessment-management/employees"
        );
        assert!(AssessmentManagementApiClient::routes()[5].ends_with("/{assignment_id}/revoke"));
        assert!(AssessmentManagementApiClient::routes()[6].ends_with("/measurements"));
    }

    #[wasm_bindgen_test]
    fn stage23e_strict_dtos_decode_exact_nullable_contract() {
        let assignment: ManagerAssignment = serde_json::from_str(&assignment_json()).unwrap();
        assert_eq!(assignment.status, AssignmentStatus::Assigned);
        assert_eq!(assignment.due_at, None);
        assert_eq!(assignment.progress.completion, None);
        let employee = format!(
            r#"{{"employee_profile_id":"{EMPLOYEE}","display_name":"Сотрудник","position_title":null,"status":"active"}}"#
        );
        assert!(serde_json::from_str::<ManagementEmployee>(&employee).is_ok());
        let template = format!(
            r#"{{"template_id":"{TEMPLATE}","template_version_id":"{VERSION}","name":"Проверка","activity_type":"evaluation","version":1,"published_at":"2026-08-08T00:00:00Z"}}"#
        );
        assert!(serde_json::from_str::<AssignableTemplate>(&template).is_ok());
    }

    #[wasm_bindgen_test]
    fn stage23e_dtos_reject_unknown_wrong_case_and_sensitive_fields() {
        assert!(serde_json::from_str::<ManagerAssignment>(
            &assignment_json().replace("}}", ",\"answers\":[]}}")
        )
        .is_err());
        assert!(serde_json::from_str::<ManagerAssignment>(
            &assignment_json().replace("assigned_at", "assignedAt")
        )
        .is_err());
        assert!(serde_json::from_str::<ManagerAssignment>(
            &assignment_json().replace("}}", ",\"score\":99}}")
        )
        .is_err());
    }

    #[wasm_bindgen_test]
    fn stage23e_create_request_contains_only_client_controlled_fields() {
        let body = CreateAssignmentRequest {
            employee_profile_id: EMPLOYEE,
            template_version_id: VERSION,
            venue_id: None,
            due_at: Some("2026-09-01T09:00:00Z".into()),
        };
        let value = serde_json::to_value(body).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        for forbidden in [
            "company_id",
            "status",
            "assigned_at",
            "account_id",
            "answers",
        ] {
            assert!(value.get(forbidden).is_none());
        }
    }

    #[wasm_bindgen_test]
    fn stage23e_error_codes_are_distinct_and_bounded() {
        assert_eq!(
            map_error(403, Some("permission_denied")),
            AssessmentManagementApiError::PermissionDenied
        );
        assert_eq!(
            map_error(409, Some("duplicate_active_assignment")),
            AssessmentManagementApiError::DuplicateActiveAssignment
        );
        assert_eq!(
            map_error(409, Some("assessment_already_completed")),
            AssessmentManagementApiError::AlreadyCompleted
        );
        assert_eq!(
            map_error(409, Some("assessment_state_conflict")),
            AssessmentManagementApiError::StateConflict
        );
        assert_eq!(
            map_error(422, None),
            AssessmentManagementApiError::InvalidRequest
        );
        assert_eq!(
            map_error(500, None),
            AssessmentManagementApiError::InternalError
        );
    }
}
