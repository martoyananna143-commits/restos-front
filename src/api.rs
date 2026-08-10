//! API client — all HTTP calls to the backend web endpoints.
//! Authentication via JWT Bearer token stored in auth state.

use gloo_net::http::Request;
use wasm_bindgen::JsValue;

use crate::types::{
    AiChatRequest, AiChatResponse, AiConfirmRequest, AiCreateCriterionSetRequest,
    AiCreateCriterionSetResponse, AiPreviewResponse, AnalyticsData, ApiError,
    CreateInvitationRequest, CreateOrgRequest, CriterionSetOption, Employee, EmployeeTypeOption,
    EvaluationDetail, EvaluationItem, EvaluationTypeOption, Invitation, InviteInfoResponse,
    LoginResponse, MeResponse, OrgInfo, ResumeEvaluationResponse, StandaloneInviteRequest,
    StartEvaluationRequest, StartEvaluationResponse, SubmitRequest, SubmitResponse,
    SwitchOrgRequest, UpdateRoleRequest,
};

#[inline]
fn net_err<E: std::fmt::Display>(e: E) -> String {
    crate::user_error::from_transport_error(e)
}

#[inline]
fn js_val_err(e: JsValue) -> String {
    crate::user_error::from_transport_error(e.as_string().unwrap_or_default())
}

/// Returned in `Err` from API helpers when the server needs a fresh PIN (`403` + `pin_required`).
pub const ERR_PIN_REQUIRED: &str = "__PIN_REQUIRED__";

pub fn get_api_base_pub() -> String {
    get_api_base()
}

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

fn detail_might_be_pin_required(status: u16, text: &str) -> bool {
    if status != 403 {
        return false;
    }
    if text.contains("pin_required") {
        return true;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(d) = v.get("detail") {
            if d.as_str() == Some("pin_required") {
                return true;
            }
            if d.get("code").and_then(|x| x.as_str()) == Some("pin_required") {
                return true;
            }
        }
    }
    false
}

async fn parse_error(resp: gloo_net::http::Response) -> String {
    let status = resp.status();
    let text = match resp.text().await {
        Ok(body) => body,
        Err(_) => String::new(),
    };
    if detail_might_be_pin_required(status, &text) {
        return ERR_PIN_REQUIRED.to_string();
    }
    let detail = if text.trim().is_empty() {
        String::new()
    } else if let Ok(e) = serde_json::from_str::<ApiError>(&text) {
        e.detail
    } else {
        text
    };
    crate::user_error::from_http_status_and_detail(status, &detail)
}

// ── Auth ──────────────────────────────────────────────────────────────────────

pub async fn login(login: &str, password: &str) -> Result<LoginResponse, String> {
    let body = format!(
        "username={}&password={}",
        urlencoding::encode(login),
        urlencoding::encode(password)
    );
    let url = format!("{}/api/web/auth/login", get_api_base());
    let resp = Request::post(&url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn register(
    invite_code: &str,
    full_name: &str,
    login_name: &str,
    password: &str,
    org_name: Option<&str>,
) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/register", get_api_base());
    let mut body = serde_json::json!({
        "invite_code": invite_code,
        "full_name": full_name,
        "login": login_name,
        "password": password,
    });
    if let Some(org) = org_name {
        body["org_name"] = serde_json::Value::String(org.to_string());
    }
    let resp = Request::post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_invite_info(code: &str) -> Result<InviteInfoResponse, String> {
    let url = format!("{}/api/web/auth/invite-info/{}", get_api_base(), code);
    let resp = Request::get(&url).send().await.map_err(net_err)?;
    if resp.ok() {
        resp.json::<InviteInfoResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_standalone_invite(
    token: &str,
    ttl_seconds: i64,
) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/web/auth/invite-standalone", get_api_base());
    let req = StandaloneInviteRequest { ttl_seconds };
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<serde_json::Value>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn switch_org(token: &str, org_id: i64) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/switch-org", get_api_base());
    let req = SwitchOrgRequest { org_id };
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_org(token: &str, name: &str) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/create-org", get_api_base());
    let req = CreateOrgRequest {
        name: name.to_string(),
    };
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_me(token: &str) -> Result<MeResponse, String> {
    let url = format!("{}/api/web/auth/me", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<MeResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

/// Set or change the second-factor PIN (main login password unchanged). Returns a new session token.
pub async fn set_web_pin(
    token: &str,
    login_password: &str,
    new_pin: &str,
) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/pin", get_api_base());
    let body = serde_json::json!({
        "login_password": login_password,
        "new_pin": new_pin,
    });
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn verify_web_pin(token: &str, pin: &str) -> Result<LoginResponse, String> {
    let url = format!("{}/api/web/auth/verify-pin", get_api_base());
    let body = serde_json::json!({ "pin": pin });
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json::<LoginResponse>().await.map_err(net_err)
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        Ok(())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn dismiss_employee(token: &str, employee_id: i64) -> Result<(), String> {
    let url = format!("{}/api/web/employees/{}", get_api_base(), employee_id);
    let resp = Request::delete(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        Ok(())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_invitations(token: &str) -> Result<Vec<Invitation>, String> {
    let url = format!("{}/api/web/invitations", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_invitation(
    token: &str,
    req: &CreateInvitationRequest,
) -> Result<Invitation, String> {
    let url = format!("{}/api/web/invitations", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn delete_invitation(token: &str, code: &str) -> Result<(), String> {
    let url = format!(
        "{}/api/web/invitations/{}",
        get_api_base(),
        urlencoding::encode(code)
    );
    let resp = Request::delete(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        Ok(())
    } else {
        Err(parse_error(resp).await)
    }
}

// ── Evaluations ───────────────────────────────────────────────────────────────

pub async fn fetch_evaluations(token: &str) -> Result<Vec<EvaluationItem>, String> {
    let url = format!("{}/api/web/evaluations", get_api_base());
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_evaluation_detail(
    token: &str,
    evaluation_id: i64,
) -> Result<EvaluationDetail, String> {
    let url = format!(
        "{}/api/web/evaluations/{}/detail",
        get_api_base(),
        evaluation_id
    );
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_criterion_sets(
    token: &str,
    full: bool,
) -> Result<Vec<CriterionSetOption>, String> {
    let url = if full {
        format!("{}/api/web/criterion-sets?full=true", get_api_base())
    } else {
        format!("{}/api/web/criterion-sets", get_api_base())
    };
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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

    let form_data = web_sys::FormData::new().map_err(js_val_err)?;
    form_data
        .append_with_blob("file", file.as_ref())
        .map_err(js_val_err)?;

    let url = format!("{}/api/web/criterion-sets/upload-excel", get_api_base());
    let mut init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(form_data.as_ref());

    let req = web_sys::Request::new_with_str_and_init(&url, &init).map_err(js_val_err)?;
    req.headers()
        .set("Authorization", &auth_header(token))
        .map_err(js_val_err)?;

    let resp_val = JsFuture::from(
        web_sys::window()
            .ok_or_else(|| net_err("no window"))?
            .fetch_with_request(&req),
    )
    .await
    .map_err(js_val_err)?;
    let resp: web_sys::Response = resp_val
        .dyn_into()
        .map_err(|_| net_err("invalid response"))?;

    if !resp.ok() {
        let status = resp.status();
        let text_promise = resp.text().map_err(js_val_err)?;
        let text_val = JsFuture::from(text_promise).await.map_err(js_val_err)?;
        let text = text_val.as_string().unwrap_or_default();
        let err: ApiError = serde_json::from_str(&text).unwrap_or(ApiError { detail: text });
        return Err(crate::user_error::from_http_status_and_detail(
            status,
            &err.detail,
        ));
    }
    let text_promise = resp.text().map_err(js_val_err)?;
    let text_val = JsFuture::from(text_promise).await.map_err(js_val_err)?;
    let text = text_val.as_string().unwrap_or_default();
    serde_json::from_str(&text).map_err(net_err)
}

pub async fn preview_google_criterion_set(
    token: &str,
    url: &str,
) -> Result<crate::types::GoogleSheetPreviewResponse, String> {
    let api_url = format!("{}/api/web/criterion-sets/google/preview", get_api_base());
    let body = serde_json::json!({ "url": url });
    let resp = Request::post(&api_url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_google_criterion_set(
    token: &str,
    name: &str,
    url: &str,
) -> Result<crate::types::CriterionSetOption, String> {
    let api_url = format!("{}/api/web/criterion-sets/google", get_api_base());
    let body = serde_json::json!({ "name": name, "url": url });
    let resp = Request::post(&api_url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn browse_google_drive_folder(
    token: &str,
    folder_url: &str,
) -> Result<crate::types::GoogleDriveBrowseResponse, String> {
    let api_url = format!(
        "{}/api/web/criterion-sets/google/folder/browse",
        get_api_base()
    );
    let body = serde_json::json!({ "folder_url": folder_url });
    let resp = Request::post(&api_url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn create_google_criterion_set_from_folder(
    token: &str,
    folder_url: &str,
    file_id: &str,
    name: &str,
) -> Result<crate::types::CriterionSetOption, String> {
    let api_url = format!(
        "{}/api/web/criterion-sets/google/from-folder",
        get_api_base()
    );
    let body = serde_json::json!({
        "folder_url": folder_url,
        "file_id": file_id,
        "name": name,
    });
    let resp = Request::post(&api_url)
        .header("Authorization", &auth_header(token))
        .json(&body)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn sync_google_criterion_set(
    token: &str,
    set_id: i64,
) -> Result<crate::types::CriterionSetOption, String> {
    let api_url = format!(
        "{}/api/web/criterion-sets/{}/google/sync",
        get_api_base(),
        set_id
    );
    let resp = Request::post(&api_url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;
    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn submit_evaluation(
    token: &str,
    evaluation_id: i64,
    req: SubmitRequest,
) -> Result<SubmitResponse, String> {
    let url = format!(
        "{}/api/web/evaluations/{}/submit",
        get_api_base(),
        evaluation_id
    );
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(&req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn save_evaluation_draft(
    token: &str,
    evaluation_id: i64,
    req: &SubmitRequest,
) -> Result<(), String> {
    let url = format!(
        "{}/api/web/evaluations/{}/draft",
        get_api_base(),
        evaluation_id
    );
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        Ok(())
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn fetch_evaluation_resume(
    token: &str,
    evaluation_id: i64,
) -> Result<ResumeEvaluationResponse, String> {
    let url = format!(
        "{}/api/web/evaluations/{}/resume",
        get_api_base(),
        evaluation_id
    );
    let resp = Request::get(&url)
        .header("Authorization", &auth_header(token))
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

// ── AI Assistant ─────────────────────────────────────────────────────────────

pub async fn ai_chat(token: &str, req: &AiChatRequest) -> Result<AiChatResponse, String> {
    let url = format!("{}/api/web/ai/chat", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn ai_create_criterion_set(
    token: &str,
    req: &AiCreateCriterionSetRequest,
) -> Result<AiCreateCriterionSetResponse, String> {
    let url = format!("{}/api/web/criterion-sets/ai-create", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn ai_preview_criterion_set(
    token: &str,
    req: &AiCreateCriterionSetRequest,
) -> Result<AiPreviewResponse, String> {
    let url = format!("{}/api/web/criterion-sets/ai-preview", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}

pub async fn ai_confirm_criterion_set(
    token: &str,
    req: &AiConfirmRequest,
) -> Result<AiCreateCriterionSetResponse, String> {
    let url = format!("{}/api/web/criterion-sets/ai-confirm", get_api_base());
    let resp = Request::post(&url)
        .header("Authorization", &auth_header(token))
        .json(req)
        .map_err(net_err)?
        .send()
        .await
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
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
        .map_err(net_err)?;

    if resp.ok() {
        resp.json().await.map_err(net_err)
    } else {
        Err(parse_error(resp).await)
    }
}
