//! Typed Account-only client for assigned assessments and employee attempts.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq)]
pub enum AssessmentAttemptApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    InvalidRequest,
    Conflict(RevisionConflict),
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentSummary {
    pub id: Uuid,
    pub company_id: Uuid,
    pub status: String,
    pub assigned_at: String,
    pub due_at: Option<String>,
    pub template_name: String,
    pub template_version: i32,
    pub read_only: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssignmentDetail {
    pub id: Uuid,
    pub company_id: Uuid,
    pub status: String,
    pub assigned_at: String,
    pub due_at: Option<String>,
    pub template_name: String,
    pub template_version: i32,
    pub read_only: bool,
    pub document: TemplateDocument,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TemplateDocument {
    pub version_id: Uuid,
    pub sections: Vec<AssessmentSection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssessmentSection {
    pub id: Uuid,
    pub parent_section_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub items: Vec<AssessmentItem>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssessmentItem {
    pub id: Uuid,
    pub prompt: String,
    pub guidance: Option<String>,
    pub answer_type: String,
    pub required: bool,
    pub sort_order: i32,
    pub config: InputConfig,
    pub options: Vec<AssessmentOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputConfig {
    pub placeholder: Option<String>,
    pub max_length: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AssessmentOption {
    pub id: Uuid,
    pub label: String,
    pub sort_order: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AttemptAnswer {
    pub item_id: Uuid,
    pub answer_type: String,
    pub value: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AttemptDocument {
    pub id: Uuid,
    pub assignment_id: Uuid,
    pub status: String,
    pub revision: i32,
    pub started_at: String,
    pub last_saved_at: Option<String>,
    pub submitted_at: Option<String>,
    pub read_only: bool,
    pub read_only_reason: Option<String>,
    pub document: TemplateDocument,
    pub answers: Vec<AttemptAnswer>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceDraftRequest {
    pub expected_revision: i32,
    pub answers: Vec<AttemptAnswer>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RevisionConflictEnvelope {
    pub detail: RevisionConflict,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RevisionConflict {
    pub code: String,
    pub current_revision: i32,
    pub answers: Vec<AttemptAnswer>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CompletionResult {
    pub scoring_algorithm: CompletionAlgorithm,
    pub submitted_at: String,
    pub answered_count: usize,
    pub required_count: usize,
    pub total_count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum CompletionAlgorithm {
    #[serde(rename = "completion_v1")]
    CompletionV1,
}

#[derive(Clone, Debug)]
pub struct AssessmentAttemptApiClient {
    base_url: String,
}

impl AssessmentAttemptApiClient {
    pub fn new(base_url: String) -> Result<Self, AssessmentAttemptApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(AssessmentAttemptApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 7] {
        [
            "/api/v1/account/assessment-assignments",
            "/api/v1/account/assessment-assignments/{assignment_id}",
            "/api/v1/account/assessment-assignments/{assignment_id}/attempt",
            "/api/v1/account/assessment-attempts/{attempt_id}",
            "/api/v1/account/assessment-attempts/{attempt_id}/draft",
            "/api/v1/account/assessment-attempts/{attempt_id}/submit",
            "/api/v1/account/assessment-attempts/{attempt_id}/result",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn list_assignments(
        &self,
        token: &AccountAccessToken,
    ) -> Result<Vec<AssignmentSummary>, AssessmentAttemptApiError> {
        self.get("/api/v1/account/assessment-assignments", token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_assignment(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
    ) -> Result<AssignmentDetail, AssessmentAttemptApiError> {
        self.get(
            &format!("/api/v1/account/assessment-assignments/{id}"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create_or_resume(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
    ) -> Result<AttemptDocument, AssessmentAttemptApiError> {
        self.post_empty(
            &format!("/api/v1/account/assessment-assignments/{id}/attempt"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_attempt(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
    ) -> Result<AttemptDocument, AssessmentAttemptApiError> {
        self.get(&format!("/api/v1/account/assessment-attempts/{id}"), token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn replace_draft(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
        body: &ReplaceDraftRequest,
    ) -> Result<AttemptDocument, AssessmentAttemptApiError> {
        let response = Request::put(&format!(
            "{}/api/v1/account/assessment-attempts/{id}/draft",
            self.base_url
        ))
        .credentials(RequestCredentials::Include)
        .header(
            "Authorization",
            &format!("Bearer {}", token.authorization_value()),
        )
        .json(body)
        .map_err(|_| AssessmentAttemptApiError::InvalidRequest)?
        .send()
        .await
        .map_err(|_| AssessmentAttemptApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn submit(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
    ) -> Result<CompletionResult, AssessmentAttemptApiError> {
        self.post_empty(
            &format!("/api/v1/account/assessment-attempts/{id}/submit"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn result(
        &self,
        token: &AccountAccessToken,
        id: Uuid,
    ) -> Result<CompletionResult, AssessmentAttemptApiError> {
        self.get(
            &format!("/api/v1/account/assessment-attempts/{id}/result"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, AssessmentAttemptApiError> {
        let response = Request::get(&format!("{}{}", self.base_url, path))
            .credentials(RequestCredentials::Include)
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| AssessmentAttemptApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn post_empty<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, AssessmentAttemptApiError> {
        let response = Request::post(&format!("{}{}", self.base_url, path))
            .credentials(RequestCredentials::Include)
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| AssessmentAttemptApiError::NetworkUnavailable)?;
        parse_response(response).await
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_response<T: DeserializeOwned>(
    response: Response,
) -> Result<T, AssessmentAttemptApiError> {
    if response.status() == 409 {
        let conflict = response
            .json::<RevisionConflictEnvelope>()
            .await
            .map_err(|_| AssessmentAttemptApiError::InternalError)?;
        return Err(AssessmentAttemptApiError::Conflict(conflict.detail));
    }
    if !response.ok() {
        return Err(map_status(response.status()));
    }
    response
        .json::<T>()
        .await
        .map_err(|_| AssessmentAttemptApiError::InternalError)
}

fn map_status(status: u16) -> AssessmentAttemptApiError {
    match status {
        401 => AssessmentAttemptApiError::AuthenticationRequired,
        403 => AssessmentAttemptApiError::PermissionDenied,
        404 => AssessmentAttemptApiError::NotFound,
        400 | 422 => AssessmentAttemptApiError::InvalidRequest,
        _ => AssessmentAttemptApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    const ASSIGNMENT_ID: Uuid = Uuid::from_u128(1);
    const ATTEMPT_ID: Uuid = Uuid::from_u128(2);
    const COMPANY_ID: Uuid = Uuid::from_u128(3);

    fn assignment_json() -> String {
        format!(
            r#"{{"id":"{ASSIGNMENT_ID}","company_id":"{COMPANY_ID}","status":"assigned","assigned_at":"2026-08-07T00:00:00Z","due_at":null,"template_name":"Pilot","template_version":1,"read_only":false}}"#
        )
    }

    fn document_json() -> String {
        format!(
            r#"{{"id":"{ATTEMPT_ID}","assignment_id":"{ASSIGNMENT_ID}","status":"draft","revision":2,"started_at":"2026-08-07T00:00:00Z","last_saved_at":null,"submitted_at":null,"read_only":false,"read_only_reason":null,"document":{{"version_id":"{COMPANY_ID}","sections":[]}},"answers":[]}}"#
        )
    }

    #[wasm_bindgen_test]
    fn stage23c_declares_exact_seven_routes() {
        assert_eq!(
            AssessmentAttemptApiClient::routes(),
            [
                "/api/v1/account/assessment-assignments",
                "/api/v1/account/assessment-assignments/{assignment_id}",
                "/api/v1/account/assessment-assignments/{assignment_id}/attempt",
                "/api/v1/account/assessment-attempts/{attempt_id}",
                "/api/v1/account/assessment-attempts/{attempt_id}/draft",
                "/api/v1/account/assessment-attempts/{attempt_id}/submit",
                "/api/v1/account/assessment-attempts/{attempt_id}/result",
            ]
        );
    }

    #[wasm_bindgen_test]
    fn stage23c_api_dtos_decode_list_detail_document_and_nullable_fields() {
        let summary: AssignmentSummary = serde_json::from_str(&assignment_json()).unwrap();
        assert_eq!(summary.id, ASSIGNMENT_ID);
        assert_eq!(summary.company_id, COMPANY_ID);
        assert_eq!(summary.due_at, None);
        let list: Vec<AssignmentSummary> =
            serde_json::from_str(&format!("[{}]", assignment_json())).unwrap();
        assert_eq!(list.len(), 1);
        let attempt: AttemptDocument = serde_json::from_str(&document_json()).unwrap();
        assert_eq!(attempt.assignment_id, ASSIGNMENT_ID);
        assert_eq!(attempt.last_saved_at, None);
        assert_eq!(attempt.submitted_at, None);
        assert_eq!(attempt.read_only_reason, None);
        let detail = format!(
            r#"{{"id":"{ASSIGNMENT_ID}","company_id":"{COMPANY_ID}","status":"assigned","assigned_at":"2026-08-07T00:00:00Z","due_at":null,"template_name":"Pilot","template_version":1,"read_only":false,"document":{{"version_id":"{COMPANY_ID}","sections":[]}}}}"#
        );
        assert!(serde_json::from_str::<AssignmentDetail>(&detail).is_ok());
    }

    #[wasm_bindgen_test]
    fn stage23c_api_dtos_reject_unknown_and_wrong_case_fields() {
        assert!(serde_json::from_str::<AssignmentSummary>(
            &assignment_json().replace("}", ",\"score\":1}")
        )
        .is_err());
        assert!(serde_json::from_str::<AttemptDocument>(
            &document_json().replace("assignment_id", "assignmentId")
        )
        .is_err());
        assert!(serde_json::from_str::<AttemptDocument>(
            &document_json().replace("}", ",\"passed\":true}")
        )
        .is_err());
    }

    #[wasm_bindgen_test]
    fn stage23c_draft_request_has_revision_and_full_answers_payload() {
        let body = ReplaceDraftRequest {
            expected_revision: 7,
            answers: vec![AttemptAnswer {
                item_id: Uuid::from_u128(4),
                answer_type: "boolean".into(),
                value: Value::Bool(true),
            }],
        };
        let encoded = serde_json::to_value(body).unwrap();
        assert_eq!(encoded["expected_revision"], 7);
        assert_eq!(encoded["answers"].as_array().unwrap().len(), 1);
        assert!(encoded.get("expectedRevision").is_none());
        for forbidden in ["token", "cookie", "score", "passed", "correct", "weight"] {
            assert!(encoded.get(forbidden).is_none());
        }
    }

    #[wasm_bindgen_test]
    fn stage23c_conflict_is_strict_and_typed() {
        let value = r#"{"detail":{"code":"assessment_revision_conflict","current_revision":3,"answers":[]}}"#;
        let parsed: RevisionConflictEnvelope = serde_json::from_str(value).unwrap();
        assert_eq!(parsed.detail.current_revision, 3);
        assert!(serde_json::from_str::<RevisionConflictEnvelope>(
            &value.replace("}}", ",\"score\":1}}")
        )
        .is_err());
    }

    #[wasm_bindgen_test]
    fn stage23c_completion_accepts_completion_only_shape() {
        let value = r#"{"scoring_algorithm":"completion_v1","submitted_at":"2026-01-01T00:00:00Z","answered_count":2,"required_count":1,"total_count":2}"#;
        assert!(serde_json::from_str::<CompletionResult>(value).is_ok());
        assert!(
            serde_json::from_str::<CompletionResult>(&value.replace("}", ",\"score\":99}"))
                .is_err()
        );
        assert!(serde_json::from_str::<CompletionResult>(
            &value.replace("completion_v1", "weighted_v1")
        )
        .is_err());
    }

    #[wasm_bindgen_test]
    fn stage23c_status_mapping_is_safe_and_bounded() {
        assert_eq!(
            map_status(401),
            AssessmentAttemptApiError::AuthenticationRequired
        );
        assert_eq!(map_status(403), AssessmentAttemptApiError::PermissionDenied);
        assert_eq!(map_status(404), AssessmentAttemptApiError::NotFound);
        assert_eq!(map_status(422), AssessmentAttemptApiError::InvalidRequest);
        assert_eq!(map_status(500), AssessmentAttemptApiError::InternalError);
    }
}
