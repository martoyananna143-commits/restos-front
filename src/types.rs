//! Data types for the web application API

use serde::{Deserialize, Serialize};

// ── Auth ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub employee_id: i64,
    pub org_id: i64,
    pub name: String,
    pub is_admin: bool,
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

#[derive(Debug, Clone, Serialize)]
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

// ── Analytics ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyticsData {
    pub total_evaluations: i32,
    pub average_score: f64,
    pub employees_count: i32,
    pub top_employees: Vec<TopEmployee>,
    #[serde(default)]
    pub criteria_stats: Vec<CriterionStat>,
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

// ── API Error ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ApiError {
    pub detail: String,
}
