//! Data types for the web application API

use serde::{Deserialize, Serialize};

// ── Auth ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrgInfo {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub employee_id: i64,
    pub org_id: i64,
    pub name: String,
    pub is_admin: bool,
    #[serde(default)]
    pub is_superuser: bool,
    #[serde(default)]
    pub available_orgs: Vec<OrgInfo>,
}

#[derive(Debug, Deserialize)]
pub struct InviteInfoResponse {
    pub valid: bool,
    #[serde(default)]
    pub is_standalone: bool,
    #[serde(default)]
    pub organization_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StandaloneInviteRequest {
    pub ttl_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct SwitchOrgRequest {
    pub org_id: i64,
}

#[derive(Debug, Serialize)]
pub struct CreateOrgRequest {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MeResponse {
    pub employee_id: i64,
    pub org_id: i64,
    pub name: String,
    pub is_admin: bool,
    #[serde(default)]
    pub is_superuser: bool,
    pub position: Option<String>,
    pub organization_name: Option<String>,
    #[serde(default)]
    pub available_orgs: Vec<OrgInfo>,
    #[serde(default)]
    pub web_login: Option<String>,
    #[serde(default)]
    pub has_password: bool,
    #[serde(default)]
    pub has_pin: bool,
    #[serde(default)]
    pub pin_required_now: bool,
    #[serde(default)]
    pub is_org_creator: bool,
    #[serde(default)]
    pub is_employee_role: bool,
}

// ── Criterion / Form ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Criterion {
    pub id: i64,
    pub name: String,
    pub code: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    #[serde(default = "default_true")]
    pub is_required: bool,
}

fn default_value_type() -> String { "boolean".into() }
fn default_true() -> bool { true }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub criterion_id: i64,
    pub value: AnswerValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnswerValue {
    Boolean(bool),
    Number(f64),
    Text(String),
}

#[derive(Debug, Serialize)]
pub struct SubmitRequest {
    pub answers: Vec<Answer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitResponse {
    pub evaluation_id: i64,
    pub score_percentage: f64,
    pub passed_criteria: i32,
    pub failed_criteria: i32,
    pub total_criteria: i32,
}

// ── Evaluation setup ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EmployeeTypeOption {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub is_administrator: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvaluationTypeOption {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CriterionSetOption {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub criterion_ids: Option<Vec<i64>>,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleSheetPreviewRow {
    pub block: String,
    pub criterion: String,
    pub value_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleSheetPreviewResponse {
    pub rows_count: i32,
    pub blocks: Vec<String>,
    pub sample_rows: Vec<GoogleSheetPreviewRow>,
    pub spreadsheet_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleDriveFileItem {
    pub file_id: String,
    pub name: String,
    #[serde(default)]
    pub modified_time: Option<String>,
    #[serde(default)]
    pub web_view_link: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleDriveBrowseResponse {
    pub files: Vec<GoogleDriveFileItem>,
}

#[derive(Debug, Serialize)]
pub struct StartEvaluationRequest {
    pub evaluated_employee_id: i64,
    pub criterion_set_id: i64,
    pub evaluation_type_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartEvaluationResponse {
    pub evaluation_id: i64,
    pub criteria: Vec<Criterion>,
    pub organization_name: String,
    pub evaluated_employee_name: String,
    pub criterion_set_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResumeEvaluationResponse {
    pub evaluation_id: i64,
    pub criteria: Vec<Criterion>,
    pub organization_name: String,
    pub evaluated_employee_name: String,
    pub criterion_set_name: String,
    #[serde(default)]
    pub saved_answers: Vec<Answer>,
}

// ── Employees ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Employee {
    pub id: i64,
    pub full_name: String,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    pub employee_type_id: i64,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct UpdateRoleRequest {
    pub employee_type_id: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Invitation {
    pub code: String,
    pub invite_url: String,
    pub organization_id: i64,
    #[serde(default)]
    pub employee_type_id: Option<i64>,
    #[serde(default)]
    pub employee_type_name: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    #[serde(default)]
    pub contact_telegram: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub used: bool,
}

#[derive(Debug, Serialize)]
pub struct CreateInvitationRequest {
    pub employee_type_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_telegram: Option<String>,
    pub ttl_seconds: i64,
}

// ── Evaluations list ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvaluationItem {
    pub id: i64,
    #[serde(default)]
    pub evaluated_employee_id: Option<i64>,
    #[serde(default)]
    pub evaluated_employee_name: Option<String>,
    pub evaluation_type_id: i64,
    #[serde(default)]
    pub evaluation_type_name: Option<String>,
    pub criterion_set_id: i64,
    #[serde(default)]
    pub score_percentage: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvaluationCriterionAnswer {
    pub name: String,
    #[serde(default = "default_value_type")]
    pub value_type: String,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EvaluationDetail {
    pub evaluation_id: i64,
    pub organization_name: String,
    pub evaluation_type_name: String,
    #[serde(default)]
    pub criterion_set_name: Option<String>,
    pub evaluated_employee_name: String,
    pub filled_by_employee_name: String,
    pub evaluation_date: String,
    #[serde(default)]
    pub score_percentage: Option<f64>,
    pub passed_criteria: i32,
    pub failed_criteria: i32,
    pub total_criteria: i32,
    pub status: String,
    #[serde(default)]
    pub comment: Option<String>,
    pub criteria: Vec<EvaluationCriterionAnswer>,
}

// ── Analytics ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AnalyticsData {
    pub total_evaluations: i32,
    pub average_score: f64,
    pub employees_count: i32,
    pub top_employees: Vec<TopEmployee>,
    #[serde(default)]
    pub criteria_stats: Vec<CriterionStat>,
    #[serde(default)]
    pub is_personal_view: bool,
    #[serde(default)]
    pub monthly_average_score: f64,
    #[serde(default)]
    pub my_evaluations: Vec<MyEvaluationRow>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MyEvaluationRow {
    pub id: i64,
    #[serde(default)]
    pub filled_by_employee_id: Option<i64>,
    #[serde(default)]
    pub filled_by_employee_name: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub score_percentage: Option<f64>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TopEmployee {
    pub id: i64,
    pub name: String,
    pub avg: f64,
    pub count: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CriterionStat {
    #[serde(default)]
    pub criterion_name: String,
    pub total_checks: i32,
    pub passed: i32,
    pub failed: i32,
    pub pass_rate: f64,
}

// ── AI Assistant ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct AiChatRequest {
    pub message: String,
    pub conversation_history: Vec<AiMessage>,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiChatResponse {
    pub response: String,
    pub conversation_history: Vec<AiMessage>,
}

#[derive(Debug, Serialize)]
pub struct AiCreateCriterionSetRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiCreateCriterionSetResponse {
    pub set_id: i64,
    pub set_name: String,
    pub criteria_created: i32,
    pub criterion_ids: Vec<i64>,
    pub ai_response: String,
}

// ── Two-step AI flow ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiPreviewCriterion {
    pub name: String,
    pub value_type: String,
    #[serde(default)]
    pub description: Option<String>,
    pub is_required: bool,
}

impl Default for AiPreviewCriterion {
    fn default() -> Self {
        Self {
            name: String::new(),
            value_type: "boolean".to_string(),
            description: None,
            is_required: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiPreviewResponse {
    pub set_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub criteria: Vec<AiPreviewCriterion>,
    pub ai_response: String,
}

#[derive(Debug, Serialize)]
pub struct AiConfirmRequest {
    pub set_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub is_default: bool,
    pub criteria: Vec<AiPreviewCriterion>,
}

// ── API Error ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub detail: String,
}
