//! Typed Account client for organization structure, reusable invitations and tasks.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::{FormData, RequestCredentials};

use crate::account_api::AccountAccessToken;

/// Release gate for group onboarding that collects EmployeeProfile birth date.
/// It remains fail-closed until the separately approved legal publication task.
pub const GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED: bool = false;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrganizationWorkflowApiError {
    AuthenticationRequired,
    NotFound,
    Conflict,
    InvalidRequest,
    Unavailable,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AccessPreset {
    pub code: String,
    pub title: String,
    pub scope: String,
    pub assignable: bool,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationPosition {
    pub position_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub access_preset: String,
    pub revision: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreatePositionRequest {
    pub request_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub access_preset: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupInvitation {
    pub invitation_id: Uuid,
    pub company_id: Uuid,
    pub venue_id: Uuid,
    pub position_id: Uuid,
    pub label: String,
    pub status: String,
    pub max_registrations: i32,
    pub registration_count: i32,
    pub expires_at: String,
    pub join_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateGroupInvitationRequest {
    pub request_id: Uuid,
    pub venue_id: Uuid,
    pub position_id: Uuid,
    pub label: String,
    pub expires_at: String,
    pub max_registrations: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct JoinGroupInvitationRequest {
    pub token: String,
    pub request_id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub birth_date: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GroupRegistration {
    pub registration_id: Uuid,
    pub company_id: Uuid,
    pub venue_id: Uuid,
    pub employee_profile_id: Uuid,
    pub display_name: String,
    pub status: String,
    pub created_at: String,
    pub activated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskRequest {
    pub request_id: Uuid,
    pub venue_id: Uuid,
    pub assessment_attempt_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTask {
    pub task_id: Uuid,
    pub company_id: Uuid,
    pub venue_id: Uuid,
    pub assessment_attempt_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub version: i32,
    pub assignment_count: i32,
    pub task_assignment_id: Option<Uuid>,
    pub assignment_status: Option<String>,
    pub assignment_version: Option<i32>,
    pub photo_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DispatchTaskRequest {
    pub employee_profile_ids: Vec<Uuid>,
    pub expected_version: i32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpdateTaskRequest {
    pub expected_version: i32,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransitionTaskRequest {
    pub action: String,
    pub expected_version: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskAssignment {
    pub task_assignment_id: Uuid,
    pub task_id: Uuid,
    pub task_status: String,
    pub status: String,
    pub version: i32,
    pub submitted_at: Option<String>,
    pub reviewed_at: Option<String>,
    pub photo_count: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskEvent {
    pub event_type: String,
    pub occurred_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskAssignee {
    pub employee_profile_id: Uuid,
    pub display_name: String,
    pub position_name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskPhoto {
    pub photo_id: Uuid,
    pub task_id: Uuid,
    pub task_assignment_id: Option<Uuid>,
    pub mime_type: String,
    pub byte_size: i32,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TaskMediaCapability {
    pub enabled: bool,
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
pub struct OrganizationWorkflowApiClient {
    base_url: String,
}

impl OrganizationWorkflowApiClient {
    pub fn new(base_url: String) -> Result<Self, OrganizationWorkflowApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(OrganizationWorkflowApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 22] {
        [
            "/api/v1/account/organization/access-presets",
            "/api/v1/account/task-media/capability",
            "/api/v1/account/companies/{company_id}/organization/positions",
            "/api/v1/account/companies/{company_id}/organization/positions",
            "/api/v1/account/companies/{company_id}/organization/group-invitations",
            "/api/v1/account/companies/{company_id}/organization/group-invitations",
            "/api/v1/account/companies/{company_id}/organization/group-invitations/{invitation_id}",
            "/api/v1/account/organization/group-invitations/join",
            "/api/v1/account/companies/{company_id}/organization/pending-registrations",
            "/api/v1/account/companies/{company_id}/organization/pending-registrations/{registration_id}/activate",
            "/api/v1/account/companies/{company_id}/tasks",
            "/api/v1/account/companies/{company_id}/tasks/assignees",
            "/api/v1/account/companies/{company_id}/tasks",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/history",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/dispatch",
            "/api/v1/account/companies/{company_id}/task-assignments/{task_assignment_id}/transition",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/photos",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/photos",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/photos/{photo_id}/content",
            "/api/v1/account/companies/{company_id}/tasks/{task_id}/photos/{photo_id}",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn task_media_capability(
        &self,
        token: &AccountAccessToken,
    ) -> Result<TaskMediaCapability, OrganizationWorkflowApiError> {
        self.get("/api/v1/account/task-media/capability", token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn access_presets(
        &self,
        token: &AccountAccessToken,
    ) -> Result<Vec<AccessPreset>, OrganizationWorkflowApiError> {
        self.get("/api/v1/account/organization/access-presets", token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn positions(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<OrganizationPosition>, OrganizationWorkflowApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/organization/positions"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_position(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreatePositionRequest,
    ) -> Result<OrganizationPosition, OrganizationWorkflowApiError> {
        self.post(
            &format!("/api/v1/account/companies/{company_id}/organization/positions"),
            token,
            body,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn group_invitations(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<GroupInvitation>, OrganizationWorkflowApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/organization/group-invitations"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_group_invitation(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreateGroupInvitationRequest,
    ) -> Result<GroupInvitation, OrganizationWorkflowApiError> {
        self.post(
            &format!("/api/v1/account/companies/{company_id}/organization/group-invitations"),
            token,
            body,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn revoke_group_invitation(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        invitation_id: Uuid,
    ) -> Result<GroupInvitation, OrganizationWorkflowApiError> {
        let request = self.authenticated(Request::delete(&format!("{}/api/v1/account/companies/{company_id}/organization/group-invitations/{invitation_id}", self.base_url)), token);
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn join_group_invitation(
        &self,
        token: &AccountAccessToken,
        body: &JoinGroupInvitationRequest,
    ) -> Result<GroupRegistration, OrganizationWorkflowApiError> {
        self.post(
            "/api/v1/account/organization/group-invitations/join",
            token,
            body,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn pending_registrations(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<GroupRegistration>, OrganizationWorkflowApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/organization/pending-registrations"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn activate_registration(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        registration_id: Uuid,
    ) -> Result<GroupRegistration, OrganizationWorkflowApiError> {
        self.post(&format!("/api/v1/account/companies/{company_id}/organization/pending-registrations/{registration_id}/activate"), token, &serde_json::json!({})).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn tasks(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        view: &str,
    ) -> Result<Vec<OrganizationTask>, OrganizationWorkflowApiError> {
        let view = match view {
            "mine" => "mine",
            "review" => "review",
            _ => "created",
        };
        self.get(
            &format!("/api/v1/account/companies/{company_id}/tasks?view={view}"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_task(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        body: &CreateTaskRequest,
    ) -> Result<OrganizationTask, OrganizationWorkflowApiError> {
        self.post(
            &format!("/api/v1/account/companies/{company_id}/tasks"),
            token,
            body,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn task_history(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
    ) -> Result<Vec<TaskEvent>, OrganizationWorkflowApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/tasks/{task_id}/history"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn task_assignees(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        venue_id: Uuid,
    ) -> Result<Vec<TaskAssignee>, OrganizationWorkflowApiError> {
        self.get(
            &format!("/api/v1/account/companies/{company_id}/tasks/assignees?venue_id={venue_id}"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn update_task(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        body: &UpdateTaskRequest,
    ) -> Result<OrganizationTask, OrganizationWorkflowApiError> {
        let request = self
            .authenticated(
                Request::put(&format!(
                    "{}/api/v1/account/companies/{company_id}/tasks/{task_id}",
                    self.base_url
                )),
                token,
            )
            .json(body)
            .map_err(|_| OrganizationWorkflowApiError::InvalidRequest)?;
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn cancel_task(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        expected_version: i32,
    ) -> Result<OrganizationTask, OrganizationWorkflowApiError> {
        let request = self.authenticated(
            Request::delete(&format!(
                "{}/api/v1/account/companies/{company_id}/tasks/{task_id}?expected_version={expected_version}",
                self.base_url
            )),
            token,
        );
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn dispatch_task(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        body: &DispatchTaskRequest,
    ) -> Result<OrganizationTask, OrganizationWorkflowApiError> {
        self.post(
            &format!("/api/v1/account/companies/{company_id}/tasks/{task_id}/dispatch"),
            token,
            body,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn transition_task(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        assignment_id: Uuid,
        body: &TransitionTaskRequest,
    ) -> Result<TaskAssignment, OrganizationWorkflowApiError> {
        self.post(&format!("/api/v1/account/companies/{company_id}/task-assignments/{assignment_id}/transition"), token, body).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn task_photos(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        assignment_id: Option<Uuid>,
    ) -> Result<Vec<TaskPhoto>, OrganizationWorkflowApiError> {
        let suffix = assignment_id
            .map(|value| format!("?task_assignment_id={value}"))
            .unwrap_or_default();
        self.get(
            &format!("/api/v1/account/companies/{company_id}/tasks/{task_id}/photos{suffix}"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn upload_task_photo(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        assignment_id: Option<Uuid>,
        file: web_sys::File,
    ) -> Result<TaskPhoto, OrganizationWorkflowApiError> {
        let form = FormData::new().map_err(|_| OrganizationWorkflowApiError::InternalError)?;
        form.append_with_blob("photo", file.as_ref())
            .map_err(|_| OrganizationWorkflowApiError::InvalidRequest)?;
        let suffix = assignment_id
            .map(|value| format!("?task_assignment_id={value}"))
            .unwrap_or_default();
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(form.as_ref());
        init.set_credentials(RequestCredentials::Include);
        let request = web_sys::Request::new_with_str_and_init(
            &format!(
                "{}/api/v1/account/companies/{company_id}/tasks/{task_id}/photos{suffix}",
                self.base_url
            ),
            &init,
        )
        .map_err(|_| OrganizationWorkflowApiError::InternalError)?;
        self.authorize_web_request(&request, token)?;
        self.decode_web(fetch(request).await).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn download_task_photo(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        photo_id: Uuid,
    ) -> Result<Vec<u8>, OrganizationWorkflowApiError> {
        let init = web_sys::RequestInit::new();
        init.set_method("GET");
        init.set_credentials(RequestCredentials::Include);
        let request = web_sys::Request::new_with_str_and_init(
            &format!(
                "{}/api/v1/account/companies/{company_id}/tasks/{task_id}/photos/{photo_id}/content",
                self.base_url
            ),
            &init,
        )
        .map_err(|_| OrganizationWorkflowApiError::InternalError)?;
        self.authorize_web_request(&request, token)?;
        let response = fetch(request)
            .await
            .map_err(|_| OrganizationWorkflowApiError::NetworkUnavailable)?;
        if !response.ok() {
            return Err(self.web_error(response).await);
        }
        let buffer = JsFuture::from(
            response
                .array_buffer()
                .map_err(|_| OrganizationWorkflowApiError::InternalError)?,
        )
        .await
        .map_err(|_| OrganizationWorkflowApiError::NetworkUnavailable)?;
        Ok(js_sys::Uint8Array::new(&buffer).to_vec())
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn delete_task_photo(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        task_id: Uuid,
        photo_id: Uuid,
    ) -> Result<TaskPhoto, OrganizationWorkflowApiError> {
        let init = web_sys::RequestInit::new();
        init.set_method("DELETE");
        init.set_credentials(RequestCredentials::Include);
        let request = web_sys::Request::new_with_str_and_init(
            &format!(
                "{}/api/v1/account/companies/{company_id}/tasks/{task_id}/photos/{photo_id}",
                self.base_url
            ),
            &init,
        )
        .map_err(|_| OrganizationWorkflowApiError::InternalError)?;
        self.authorize_web_request(&request, token)?;
        self.decode_web(fetch(request).await).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, OrganizationWorkflowApiError> {
        let request =
            self.authenticated(Request::get(&format!("{}{}", self.base_url, path)), token);
        self.decode(request.send().await).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn post<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        token: &AccountAccessToken,
        body: &B,
    ) -> Result<T, OrganizationWorkflowApiError> {
        let request = self
            .authenticated(Request::post(&format!("{}{}", self.base_url, path)), token)
            .json(body)
            .map_err(|_| OrganizationWorkflowApiError::InvalidRequest)?;
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
    fn authorize_web_request(
        &self,
        request: &web_sys::Request,
        token: &AccountAccessToken,
    ) -> Result<(), OrganizationWorkflowApiError> {
        request
            .headers()
            .set(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .map_err(|_| OrganizationWorkflowApiError::InternalError)?;
        request
            .headers()
            .set("X-RestOS-Web-Session", "1")
            .map_err(|_| OrganizationWorkflowApiError::InternalError)
    }

    #[cfg(target_arch = "wasm32")]
    async fn decode_web<T: DeserializeOwned>(
        &self,
        response: Result<web_sys::Response, wasm_bindgen::JsValue>,
    ) -> Result<T, OrganizationWorkflowApiError> {
        let response = response.map_err(|_| OrganizationWorkflowApiError::NetworkUnavailable)?;
        if !response.ok() {
            return Err(self.web_error(response).await);
        }
        let text = JsFuture::from(
            response
                .text()
                .map_err(|_| OrganizationWorkflowApiError::InternalError)?,
        )
        .await
        .map_err(|_| OrganizationWorkflowApiError::NetworkUnavailable)?
        .as_string()
        .ok_or(OrganizationWorkflowApiError::InternalError)?;
        serde_json::from_str(&text).map_err(|_| OrganizationWorkflowApiError::InternalError)
    }

    #[cfg(target_arch = "wasm32")]
    async fn web_error(&self, response: web_sys::Response) -> OrganizationWorkflowApiError {
        let status = response.status();
        let code = match response.text() {
            Ok(promise) => JsFuture::from(promise)
                .await
                .ok()
                .and_then(|value| value.as_string())
                .and_then(|text| serde_json::from_str::<ErrorEnvelope>(&text).ok())
                .map(|value| value.detail.code),
            Err(_) => None,
        };
        map_error(status, code.as_deref())
    }

    #[cfg(target_arch = "wasm32")]
    async fn decode<T: DeserializeOwned>(
        &self,
        response: Result<Response, gloo_net::Error>,
    ) -> Result<T, OrganizationWorkflowApiError> {
        let response = response.map_err(|_| OrganizationWorkflowApiError::NetworkUnavailable)?;
        if response.ok() {
            return response
                .json::<T>()
                .await
                .map_err(|_| OrganizationWorkflowApiError::InternalError);
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

#[cfg(target_arch = "wasm32")]
async fn fetch(request: web_sys::Request) -> Result<web_sys::Response, wasm_bindgen::JsValue> {
    let value = JsFuture::from(
        web_sys::window()
            .ok_or_else(|| wasm_bindgen::JsValue::from_str("window unavailable"))?
            .fetch_with_request(&request),
    )
    .await?;
    value.dyn_into::<web_sys::Response>()
}

fn map_error(status: u16, code: Option<&str>) -> OrganizationWorkflowApiError {
    match (status, code) {
        (401, _) => OrganizationWorkflowApiError::AuthenticationRequired,
        (404, _) => OrganizationWorkflowApiError::NotFound,
        (409, _) => OrganizationWorkflowApiError::Conflict,
        (410 | 503, _) => OrganizationWorkflowApiError::Unavailable,
        (400 | 422, _) => OrganizationWorkflowApiError::InvalidRequest,
        _ => OrganizationWorkflowApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn organization_workflow_routes_are_account_scoped_and_exact() {
        let routes = OrganizationWorkflowApiClient::routes();
        assert_eq!(routes.len(), 22);
        assert!(routes
            .iter()
            .all(|route| route.starts_with("/api/v1/account/")));
        assert_eq!(
            routes
                .iter()
                .filter(|route| route.contains("group-invitations"))
                .count(),
            4
        );
        assert_eq!(
            routes
                .iter()
                .filter(|route| **route == "/api/v1/account/task-media/capability")
                .count(),
            1
        );
    }

    #[wasm_bindgen_test]
    fn workflow_errors_are_safe_and_typed() {
        assert_eq!(
            map_error(401, None),
            OrganizationWorkflowApiError::AuthenticationRequired
        );
        assert_eq!(
            map_error(409, Some("workflow_conflict")),
            OrganizationWorkflowApiError::Conflict
        );
        assert_eq!(
            map_error(410, Some("workflow_unavailable")),
            OrganizationWorkflowApiError::Unavailable
        );
    }

    #[wasm_bindgen_test]
    fn private_task_media_request_uses_exact_account_headers_and_strict_public_dto() {
        let client = OrganizationWorkflowApiClient::new("https://app.example.test".into())
            .expect("synthetic base URL is valid");
        let request = web_sys::Request::new_with_str(
            "https://app.example.test/api/v1/account/companies/company/tasks/task/photos",
        )
        .expect("synthetic request is valid");
        let token = AccountAccessToken::from_server("synthetic-token".into())
            .expect("synthetic token is valid");
        client
            .authorize_web_request(&request, &token)
            .expect("headers can be applied");
        assert_eq!(
            request.headers().get("Authorization").unwrap(),
            Some("Bearer synthetic-token".into())
        );
        assert_eq!(
            request.headers().get("X-RestOS-Web-Session").unwrap(),
            Some("1".into())
        );
        assert_ne!(
            request.headers().get("Authorization").unwrap(),
            Some("synthetic-token".into())
        );

        let public = r#"{
            "photo_id":"00000000-0000-0000-0000-000000000001",
            "task_id":"00000000-0000-0000-0000-000000000002",
            "task_assignment_id":null,
            "mime_type":"image/png",
            "byte_size":128,
            "status":"ready"
        }"#;
        assert!(serde_json::from_str::<TaskPhoto>(public).is_ok());
        let internal = public.replace(
            "\"status\":\"ready\"",
            "\"status\":\"ready\",\"object_key\":\"evidence/private\"",
        );
        assert!(serde_json::from_str::<TaskPhoto>(&internal).is_err());

        assert_eq!(
            serde_json::from_str::<TaskMediaCapability>(r#"{"enabled":false}"#)
                .expect("synthetic capability is valid"),
            TaskMediaCapability { enabled: false }
        );
        assert!(serde_json::from_str::<TaskMediaCapability>(
            r#"{"enabled":false,"provider":"disabled"}"#
        )
        .is_err());
    }
}
