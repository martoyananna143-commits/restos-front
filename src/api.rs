//! API client — all HTTP calls to the backend web endpoints.
//! Authentication via JWT Bearer token stored in auth state.

use gloo_net::http::Request;
use crate::types::{
    AnalyticsData, ApiError, CriterionSetOption, Employee, EmployeeTypeOption,
    EvaluationItem, EvaluationTypeOption, LoginResponse, StartEvaluationRequest,
    StartEvaluationResponse, SubmitRequest, SubmitResponse, UpdateRoleRequest,
};

fn get_api_base() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(origin) = window.location().origin() {
            if origin.contains(":8080") {
                return origin.replace(":8080", ":8000");
            }
            return origin;
        }
    }
    "http://localhost:8000".into()
}

fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

async fn parse_error(resp: gloo_net::http::Response) -> String {
    if let Ok(body) = resp.text().await {
        if let Ok(e) = serde_json::from_str::<ApiError>(&body) {
            return e.detail;
        }
        return body;
    }
    "Unknown error".into()
}

// ── Auth ──────────────────────────────────────────────────────────────────────

pub async fn login(login: &str, password: &str) -> Result<LoginResponse, String> {
    let body = format!("username={}&password={}", urlencoding::encode(login), urlencoding::encode(password));
    let url = format!("{}/api/web/auth/login", get_api_base());
    let resp = Request::post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn register(
    invite_code: &str,
    full_name: &str,
    login_name: &str,
    password: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/register", get_api_base());
    let body = serde_json::json!({
        "invite_code": invite_code,
        "full_name": full_name,
        "login": login_name,
        "password": password,
    });
    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

// ── Employees ─────────────────────────────────────────────────────────────────

pub async fn fetch_employees(token: &str) -> Result<Vec<Employee>, String> {
    let url = format!("{}/api/web/employees", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_employee_types(token: &str) -> Result<Vec<EmployeeTypeOption>, String> {
    let url = format!("{}/api/web/employee-types", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn update_employee_role(
    token: &str,
    employee_id: i64,
    employee_type_id: i64,
) -> Result<(), String> {
    let url = format!("{}/api/web/employees/{}/role", get_api_base(), employee_id);
    let body = UpdateRoleRequest { employee_type_id };
    let resp = Request::put(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() { Ok(()) } else { Err(parse_error(resp).await) }
}

// ── Evaluations ───────────────────────────────────────────────────────────────

pub async fn fetch_evaluations(token: &str) -> Result<Vec<EvaluationItem>, String> {
    let url = format!("{}/api/web/evaluations", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_evaluation_type(
    token: &str,
    name: &str,
    code: &str,
    description: Option<&str>,
) -> Result<EvaluationTypeOption, String> {
    let url = format!("{}/api/web/evaluation-types", get_api_base());
    let mut body = serde_json::Map::new();
    body.insert("name".into(), serde_json::json!(name));
    body.insert("code".into(), serde_json::json!(code));
    if let Some(d) = description {
        body.insert("description".into(), serde_json::json!(d));
    }
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn update_evaluation_type(
    token: &str,
    type_id: i64,
    name: Option<&str>,
    code: Option<&str>,
    description: Option<&str>,
) -> Result<EvaluationTypeOption, String> {
    let url = format!("{}/api/web/evaluation-types/{}", get_api_base(), type_id);
    let mut body = serde_json::Map::new();
    if let Some(n) = name {
        body.insert("name".into(), serde_json::json!(n));
    }
    if let Some(c) = code {
        body.insert("code".into(), serde_json::json!(c));
    }
    if let Some(d) = description {
        body.insert("description".into(), serde_json::json!(d));
    }
    let resp = Request::put(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_evaluation_types(token: &str) -> Result<Vec<EvaluationTypeOption>, String> {
    let url = format!("{}/api/web/evaluation-types", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_criterion_sets(token: &str, full: bool) -> Result<Vec<CriterionSetOption>, String> {
    let url = if full {
        format!("{}/api/web/criterion-sets?full=true", get_api_base())
    } else {
        format!("{}/api/web/criterion-sets", get_api_base())
    };
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_criterion_set(token: &str, set_id: i64) -> Result<CriterionSetOption, String> {
    let url = format!("{}/api/web/criterion-sets/{}", get_api_base(), set_id);
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_criterion_set(
    token: &str,
    name: &str,
    description: Option<&str>,
    is_default: bool,
    criterion_ids: &[i64],
) -> Result<CriterionSetOption, String> {
    let url = format!("{}/api/web/criterion-sets", get_api_base());
    let body = serde_json::json!({
        "name": name,
        "description": description,
        "is_default": is_default,
        "criterion_ids": criterion_ids,
    });
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn update_criterion_set(
    token: &str,
    set_id: i64,
    name: Option<&str>,
    description: Option<&str>,
    is_default: Option<bool>,
    criterion_ids: Option<&[i64]>,
) -> Result<CriterionSetOption, String> {
    let url = format!("{}/api/web/criterion-sets/{}", get_api_base(), set_id);
    let mut body = serde_json::Map::new();
    if let Some(n) = name {
        body.insert("name".into(), serde_json::json!(n));
    }
    if let Some(d) = description {
        body.insert("description".into(), serde_json::json!(d));
    }
    if let Some(def) = is_default {
        body.insert("is_default".into(), serde_json::json!(def));
    }
    if let Some(ids) = criterion_ids {
        body.insert("criterion_ids".into(), serde_json::json!(ids));
    }
    let resp = Request::put(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn upload_excel_criterion_sets(
    token: &str,
    file: web_sys::File,
) -> Result<ExcelUploadResponse, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let form_data = web_sys::FormData::new().map_err(|e| e.as_string().unwrap_or_default())?;
    form_data.append_with_blob("file", file.as_ref()).map_err(|e| e.as_string().unwrap_or_default())?;

    let url = format!("{}/api/web/criterion-sets/upload-excel", get_api_base());
    let mut init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(form_data.as_ref());

    let req = web_sys::Request::new_with_str_and_init(&url, &init).map_err(|e| e.as_string().unwrap_or_default())?;
    req.headers().set("Authorization", &auth_header(token)).map_err(|e| e.as_string().unwrap_or_default())?;

    let resp_val = JsFuture::from(web_sys::window()
        .ok_or("No window")?
        .fetch_with_request(&req))
        .await
        .map_err(|e| e.as_string().unwrap_or_default())?;
    let resp: web_sys::Response = resp_val.dyn_into().map_err(|_| "Invalid response")?;

    if !resp.ok() {
        let text_promise = resp.text().map_err(|e| e.as_string().unwrap_or_default())?;
        let text_val = JsFuture::from(text_promise).await.map_err(|e| e.as_string().unwrap_or_default())?;
        let text = text_val.as_string().unwrap_or_default();
        let err: ApiError = serde_json::from_str(&text).unwrap_or(ApiError { detail: text });
        return Err(err.detail);
    }
    let text_promise = resp.text().map_err(|e| e.as_string().unwrap_or_default())?;
    let text_val = JsFuture::from(text_promise).await.map_err(|e| e.as_string().unwrap_or_default())?;
    let text = text_val.as_string().unwrap_or_default();
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ExcelUploadResponse {
    pub success: bool,
    pub created_sets: i32,
    pub created_criteria: i32,
    pub set_names: Vec<String>,
}

pub async fn fetch_all_criteria(token: &str) -> Result<Vec<crate::types::Criterion>, String> {
    let url = format!("{}/api/web/criteria-all", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_criterion(
    token: &str,
    name: &str,
    code: &str,
    description: Option<&str>,
    value_type: &str,
    is_required: bool,
) -> Result<crate::types::Criterion, String> {
    let url = format!("{}/api/web/criteria", get_api_base());
    let mut body = serde_json::Map::new();
    body.insert("name".into(), serde_json::json!(name));
    body.insert("code".into(), serde_json::json!(code));
    if let Some(d) = description {
        body.insert("description".into(), serde_json::json!(d));
    }
    body.insert("value_type".into(), serde_json::json!(value_type));
    body.insert("is_required".into(), serde_json::json!(is_required));
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn update_criterion(
    token: &str,
    criterion_id: i64,
    name: Option<&str>,
    code: Option<&str>,
    description: Option<&str>,
    value_type: Option<&str>,
    is_required: Option<bool>,
) -> Result<crate::types::Criterion, String> {
    let url = format!("{}/api/web/criteria/{}", get_api_base(), criterion_id);
    let mut body = serde_json::Map::new();
    if let Some(n) = name {
        body.insert("name".into(), serde_json::json!(n));
    }
    if let Some(c) = code {
        body.insert("code".into(), serde_json::json!(c));
    }
    if let Some(d) = description {
        body.insert("description".into(), serde_json::json!(d));
    }
    if let Some(v) = value_type {
        body.insert("value_type".into(), serde_json::json!(v));
    }
    if let Some(r) = is_required {
        body.insert("is_required".into(), serde_json::json!(r));
    }
    let resp = Request::put(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn start_evaluation(
    token: &str,
    req: StartEvaluationRequest,
) -> Result<StartEvaluationResponse, String> {
    let url = format!("{}/api/web/evaluations/start", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn submit_evaluation(
    token: &str,
    evaluation_id: i64,
    req: SubmitRequest,
) -> Result<SubmitResponse, String> {
    let url = format!("{}/api/web/evaluations/{}/submit", get_api_base(), evaluation_id);
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}

// ── Analytics ─────────────────────────────────────────────────────────────────

pub async fn fetch_analytics(token: &str) -> Result<AnalyticsData, String> {
    let url = format!("{}/api/web/analytics", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        resp.json().await.map_err(|e| e.to_string())
    } else {
        Err(parse_error(resp).await)
    }
}
