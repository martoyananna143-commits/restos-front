//! Typed Account-only client for the Stage 19 assessment catalog boundary.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssessmentApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    Conflict,
    InvalidRequest,
    ConfigurationUnavailable,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryQuery {
    pub activity_type: Option<String>,
    pub query: Option<String>,
    pub limit: u16,
    pub offset: u32,
}

impl LibraryQuery {
    pub fn first_page() -> Self {
        Self {
            limit: 50,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MethodologySummary {
    pub id: Option<Uuid>,
    pub title: String,
    pub version: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct LibraryTemplateSummary {
    pub template_id: Uuid,
    pub version_id: Uuid,
    pub code: String,
    pub name: String,
    pub activity_type: String,
    pub version: i32,
    pub methodology: MethodologySummary,
    pub local_description: Option<String>,
    pub section_count: i64,
    pub item_count: i64,
    pub option_count: i64,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TemplateInfo {
    pub id: Uuid,
    pub scope: String,
    pub company_id: Option<Uuid>,
    pub source_library_version_id: Option<Uuid>,
    pub code: String,
    pub name: String,
    pub activity_type: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct TemplateVersionSummary {
    pub version_id: Uuid,
    pub version: i32,
    pub status: String,
    pub edit_revision: i32,
    pub local_description: Option<String>,
    pub change_note: Option<String>,
    pub published_at: Option<String>,
    pub section_count: i64,
    pub item_count: i64,
    pub option_count: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MethodologyDetail {
    pub id: Uuid,
    pub code: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub version: i32,
    pub owner_type: Option<String>,
    pub status: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TemplateOption {
    pub code: String,
    pub label: String,
    pub sort_order: i32,
    pub numeric_value: Option<serde_json::Value>,
    pub is_disqualifying: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TemplateItem {
    pub code: String,
    pub prompt: String,
    pub guidance: Option<String>,
    pub response_type: String,
    pub is_required: bool,
    pub sort_order: i32,
    pub evidence_mode: String,
    pub criticality: String,
    pub options: Vec<TemplateOption>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TemplateSection {
    pub code: String,
    pub title: String,
    pub description: Option<String>,
    pub section_kind: String,
    pub sort_order: i32,
    pub parent_code: Option<String>,
    pub items: Vec<TemplateItem>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TemplateDocument {
    pub template: TemplateInfo,
    pub version: TemplateVersionSummary,
    pub methodology: MethodologyDetail,
    pub sections: Vec<TemplateSection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct CompanyTemplateSummary {
    pub template_id: Uuid,
    pub code: String,
    pub name: String,
    pub activity_type: String,
    pub status: String,
    pub source_library_version_id: Option<Uuid>,
    pub methodology: Option<MethodologySummary>,
    pub latest_draft: Option<TemplateVersionSummary>,
    pub latest_published: Option<TemplateVersionSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AdoptLibraryTemplateRequest {
    pub source_library_version_id: Uuid,
    pub company_template_code: String,
    pub company_template_name: Option<String>,
    pub local_description: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AdoptedLibraryTemplate {
    pub company_template_id: Uuid,
    pub draft_version_id: Uuid,
    pub source_library_version_id: Uuid,
    pub methodology_id: Uuid,
    pub section_count: i64,
    pub item_count: i64,
    pub option_count: i64,
}

#[derive(Clone, Debug)]
pub struct AssessmentApiClient {
    base_url: String,
}

impl AssessmentApiClient {
    pub fn new(base_url: String) -> Result<Self, AssessmentApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(AssessmentApiError::ConfigurationUnavailable);
        }
        Ok(Self { base_url })
    }

    pub fn library_path(query: &LibraryQuery) -> String {
        let mut values = vec![
            format!("limit={}", query.limit.clamp(1, 100)),
            format!("offset={}", query.offset),
        ];
        if let Some(activity_type) = query
            .activity_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            values.push(format!(
                "activity_type={}",
                urlencoding::encode(activity_type)
            ));
        }
        if let Some(search) = query
            .query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            values.push(format!("query={}", urlencoding::encode(search)));
        }
        format!("/api/v1/assessment-library?{}", values.join("&"))
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn list_library_templates(
        &self,
        token: &AccountAccessToken,
        query: &LibraryQuery,
    ) -> Result<Vec<LibraryTemplateSummary>, AssessmentApiError> {
        self.get(&Self::library_path(query), token).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_library_template(
        &self,
        token: &AccountAccessToken,
        template_id: Uuid,
    ) -> Result<TemplateDocument, AssessmentApiError> {
        self.get(&format!("/api/v1/assessment-library/{template_id}"), token)
            .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn list_company_templates(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
    ) -> Result<Vec<CompanyTemplateSummary>, AssessmentApiError> {
        self.get(
            &format!("/api/v1/companies/{company_id}/assessment-templates"),
            token,
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn adopt_library_template(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        request: &AdoptLibraryTemplateRequest,
    ) -> Result<AdoptedLibraryTemplate, AssessmentApiError> {
        let response = Request::post(&format!(
            "{}/api/v1/companies/{company_id}/assessment-templates/adopt",
            self.base_url
        ))
        .header(
            "Authorization",
            &format!("Bearer {}", token.authorization_value()),
        )
        .json(request)
        .map_err(|_| AssessmentApiError::InvalidRequest)?
        .send()
        .await
        .map_err(|_| AssessmentApiError::NetworkUnavailable)?;
        parse_json(response).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, AssessmentApiError> {
        let response = Request::get(&format!("{}{}", self.base_url, path))
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| AssessmentApiError::NetworkUnavailable)?;
        parse_json(response).await
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_json<T: DeserializeOwned>(response: Response) -> Result<T, AssessmentApiError> {
    if !response.ok() {
        return Err(map_status(response.status()));
    }
    response
        .json::<T>()
        .await
        .map_err(|_| AssessmentApiError::InternalError)
}

fn map_status(status: u16) -> AssessmentApiError {
    match status {
        401 => AssessmentApiError::AuthenticationRequired,
        403 => AssessmentApiError::PermissionDenied,
        404 => AssessmentApiError::NotFound,
        409 => AssessmentApiError::Conflict,
        400 | 422 => AssessmentApiError::InvalidRequest,
        503 => AssessmentApiError::ConfigurationUnavailable,
        _ => AssessmentApiError::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn library_query_is_deterministic_and_encoded() {
        let path = AssessmentApiClient::library_path(&LibraryQuery {
            activity_type: Some("service check".into()),
            query: Some("  зал & кухня  ".into()),
            limit: 25,
            offset: 50,
        });
        assert_eq!(
            path,
            "/api/v1/assessment-library?limit=25&offset=50&activity_type=service%20check&query=%D0%B7%D0%B0%D0%BB%20%26%20%D0%BA%D1%83%D1%85%D0%BD%D1%8F"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn library_summary_matches_stage_19_schema() {
        let value: LibraryTemplateSummary = serde_json::from_str(
            r#"{
                "template_id":"00000000-0000-0000-0000-000000000001",
                "version_id":"00000000-0000-0000-0000-000000000002",
                "code":"service-standard",
                "name":"Стандарт сервиса",
                "activity_type":"evaluation",
                "version":2,
                "methodology":{"title":"Методология сервиса","version":1},
                "local_description":"Проверка стандартов",
                "section_count":3,
                "item_count":18,
                "option_count":8,
                "published_at":"2030-01-01T00:00:00Z"
            }"#,
        )
        .unwrap();
        assert_eq!(value.item_count, 18);
        assert_eq!(value.methodology.title, "Методология сервиса");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn backend_statuses_map_to_safe_ui_errors() {
        assert_eq!(map_status(401), AssessmentApiError::AuthenticationRequired);
        assert_eq!(map_status(403), AssessmentApiError::PermissionDenied);
        assert_eq!(map_status(404), AssessmentApiError::NotFound);
        assert_eq!(map_status(409), AssessmentApiError::Conflict);
        assert_eq!(map_status(422), AssessmentApiError::InvalidRequest);
        assert_eq!(
            map_status(503),
            AssessmentApiError::ConfigurationUnavailable
        );
        assert_eq!(map_status(500), AssessmentApiError::InternalError);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn account_token_type_is_required_by_public_methods() {
        fn accepts_account_token(_: &AccountAccessToken) {}
        let token = AccountAccessToken::from_server("memory-only".into()).unwrap();
        accepts_account_token(&token);
    }
}
