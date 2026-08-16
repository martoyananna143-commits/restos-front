//! Strict Account-only restaurant metric dashboard client.

use serde::{de::DeserializeOwned, Deserialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestaurantMetricsApiError {
    AuthenticationRequired,
    PermissionDenied,
    InvalidRequest,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricSourceComponent {
    pub source_type: String,
    pub scoring_algorithm: String,
    pub scoring_version: u16,
    pub score_percent: String,
    pub coverage: String,
    pub observation_count: usize,
    pub critical_failure_count: usize,
    pub stop_factor_count: usize,
    pub latest_at: String,
    pub comparison_status: String,
    pub delta: Option<String>,
    pub delta_unit: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RestaurantMetric {
    pub code: String,
    pub title: String,
    pub composite_score_percent: Option<String>,
    pub target_score_percent: Option<String>,
    pub overdue_action_count: Option<usize>,
    pub status: String,
    pub components: Vec<MetricSourceComponent>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RestaurantMetricDashboard {
    pub company_id: Uuid,
    pub venue_id: Option<Uuid>,
    pub source_type: Option<String>,
    pub section_code: Option<String>,
    pub from_at: Option<String>,
    pub to_at: String,
    pub timezone: String,
    pub comparison_from_at: Option<String>,
    pub comparison_to_at: Option<String>,
    pub aggregation_status: String,
    pub metrics: Vec<RestaurantMetric>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricCriterionContribution {
    pub item_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub section_code: Option<String>,
    pub item_code: Option<String>,
    pub prompt: Option<String>,
    pub position_index: Option<usize>,
    pub position_type: Option<String>,
    pub normalized: Option<String>,
    pub source_normalized: Option<String>,
    pub weight: Option<String>,
    pub critical_failure: Option<bool>,
    pub stop_factor_failure: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricSourceDrilldown {
    pub observation_id: Uuid,
    pub attempt_id: Option<Uuid>,
    pub product_measurement_id: Option<Uuid>,
    pub source_type: String,
    pub scoring_algorithm: String,
    pub scoring_version: u16,
    pub score_percent: String,
    pub coverage: String,
    pub critical_failure_count: usize,
    pub stop_factor_count: usize,
    pub observed_at: String,
    pub items: Vec<MetricCriterionContribution>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MetricTemplateOption {
    pub template_version_id: Uuid,
    pub name: String,
    pub activity_type: String,
    pub completed_observation_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LowIndicatorItem {
    pub rank: usize,
    pub item_code: String,
    pub label: String,
    pub score_percent: String,
    pub sample_count: usize,
    pub coverage: String,
    pub critical_failure_count: usize,
    pub stop_factor_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LowIndicatorRanking {
    pub template_version_id: Uuid,
    pub scoring_algorithm: String,
    pub scoring_version: u16,
    pub from_at: String,
    pub to_at: String,
    pub minimum_observations: usize,
    pub maximum_items: usize,
    pub observation_count: usize,
    pub status: String,
    pub items: Vec<LowIndicatorItem>,
}

#[derive(Clone, Debug)]
pub struct RestaurantMetricsApiClient {
    base_url: String,
}

impl RestaurantMetricsApiClient {
    pub fn new(base_url: String) -> Result<Self, RestaurantMetricsApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(RestaurantMetricsApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn route() -> &'static str {
        "/api/v1/account/companies/{company_id}/restaurant-metrics"
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn dashboard(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        venue_id: Option<Uuid>,
        today: bool,
        compare_previous: bool,
        source_type: Option<&str>,
        section_code: Option<&str>,
    ) -> Result<RestaurantMetricDashboard, RestaurantMetricsApiError> {
        let mut url = format!(
            "{}/api/v1/account/companies/{company_id}/restaurant-metrics",
            self.base_url
        );
        if let Some(value) = venue_id {
            url.push_str("?venue_id=");
            url.push_str(&value.to_string());
        }
        if today {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str("period=today");
            if compare_previous {
                url.push_str("&compare_previous=true");
            }
        }
        if let Some(value) = source_type {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str("source_type=");
            url.push_str(value);
        }
        if let Some(value) = section_code {
            url.push(if url.contains('?') { '&' } else { '?' });
            url.push_str("section_code=");
            url.push_str(value);
        }
        let response = Request::get(&url)
            .credentials(RequestCredentials::Include)
            .header("Cache-Control", "no-cache")
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| RestaurantMetricsApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn sources(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        metric_code: &str,
        venue_id: Option<Uuid>,
        source_type: Option<&str>,
        section_code: Option<&str>,
    ) -> Result<Vec<MetricSourceDrilldown>, RestaurantMetricsApiError> {
        let mut url = format!(
            "{}/api/v1/account/companies/{company_id}/restaurant-metrics/{metric_code}/sources?limit=50",
            self.base_url
        );
        if let Some(value) = venue_id {
            url.push_str("&venue_id=");
            url.push_str(&value.to_string());
        }
        if let Some(value) = source_type {
            url.push_str("&source_type=");
            url.push_str(value);
        }
        if let Some(value) = section_code {
            url.push_str("&section_code=");
            url.push_str(value);
        }
        let response = Request::get(&url)
            .credentials(RequestCredentials::Include)
            .header("Cache-Control", "no-cache")
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| RestaurantMetricsApiError::NetworkUnavailable)?;
        parse_response(response).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn template_options(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        venue_id: Option<Uuid>,
    ) -> Result<Vec<MetricTemplateOption>, RestaurantMetricsApiError> {
        let mut path = format!("/api/v1/account/companies/{company_id}/restaurant-metrics/options");
        if let Some(value) = venue_id {
            path.push_str("?venue_id=");
            path.push_str(&value.to_string());
        }
        self.get(&path, token).await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn low_indicators(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        template_version_id: Uuid,
        venue_id: Option<Uuid>,
    ) -> Result<LowIndicatorRanking, RestaurantMetricsApiError> {
        let mut path = format!(
            "/api/v1/account/companies/{company_id}/restaurant-metrics/low-indicators?template_version_id={template_version_id}"
        );
        if let Some(value) = venue_id {
            path.push_str("&venue_id=");
            path.push_str(&value.to_string());
        }
        self.get(&path, token).await
    }

    #[cfg(target_arch = "wasm32")]
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        token: &AccountAccessToken,
    ) -> Result<T, RestaurantMetricsApiError> {
        let response = Request::get(&format!("{}{}", self.base_url, path))
            .credentials(RequestCredentials::Include)
            .header("Cache-Control", "no-cache")
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .send()
            .await
            .map_err(|_| RestaurantMetricsApiError::NetworkUnavailable)?;
        parse_response(response).await
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_response<T: DeserializeOwned>(
    response: Response,
) -> Result<T, RestaurantMetricsApiError> {
    if response.ok() {
        return response
            .json()
            .await
            .map_err(|_| RestaurantMetricsApiError::InternalError);
    }
    Err(match response.status() {
        401 => RestaurantMetricsApiError::AuthenticationRequired,
        403 => RestaurantMetricsApiError::PermissionDenied,
        400 | 422 => RestaurantMetricsApiError::InvalidRequest,
        _ => RestaurantMetricsApiError::InternalError,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn metric_dashboard_contract_is_strict_and_components_only() {
        let json = r#"{"company_id":"00000000-0000-0000-0000-000000000001","venue_id":null,"source_type":null,"section_code":null,"from_at":"2026-08-13T00:00:00Z","to_at":"2026-08-14T00:00:00Z","timezone":"Europe/Moscow","comparison_from_at":"2026-08-12T00:00:00Z","comparison_to_at":"2026-08-13T00:00:00Z","aggregation_status":"components_only","metrics":[{"code":"taste","title":"Вкус","composite_score_percent":null,"target_score_percent":null,"overdue_action_count":null,"status":"components_available","components":[{"source_type":"walkthrough","scoring_algorithm":"weighted_v1","scoring_version":1,"score_percent":"85.0000","coverage":"1.000000","observation_count":1,"critical_failure_count":0,"stop_factor_count":0,"latest_at":"2026-08-13T00:00:00Z","comparison_status":"comparable","delta":"5.0000","delta_unit":"percentage_points"}]}]}"#;
        let parsed: RestaurantMetricDashboard = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.metrics[0].composite_score_percent, None);
        assert_eq!(parsed.metrics[0].components.len(), 1);
        assert!(serde_json::from_str::<RestaurantMetricDashboard>(
            &json.replace("\"metrics\":", "\"answers\":[],\"metrics\":")
        )
        .is_err());
    }

    #[wasm_bindgen_test]
    fn metric_dashboard_declares_exact_route() {
        assert_eq!(
            RestaurantMetricsApiClient::route(),
            "/api/v1/account/companies/{company_id}/restaurant-metrics"
        );
    }

    #[wasm_bindgen_test]
    fn user_journey_ranking_and_template_options_are_strict_and_private_free() {
        let option = r#"{"template_version_id":"00000000-0000-0000-0000-000000000001","name":"Synthetic evaluation","activity_type":"evaluation","completed_observation_count":3}"#;
        assert!(serde_json::from_str::<MetricTemplateOption>(option).is_ok());
        assert!(serde_json::from_str::<MetricTemplateOption>(
            &option.replace("\"name\":", "\"phone\":\"+70000000000\",\"name\":")
        )
        .is_err());

        let ranking = r#"{"template_version_id":"00000000-0000-0000-0000-000000000001","scoring_algorithm":"weighted_v1","scoring_version":1,"from_at":"2026-08-01T00:00:00Z","to_at":"2026-08-14T00:00:00Z","minimum_observations":3,"maximum_items":10,"observation_count":3,"status":"available","items":[{"rank":1,"item_code":"service-speed","label":"Скорость обслуживания","score_percent":"50.0000","sample_count":3,"coverage":"1.000000","critical_failure_count":0,"stop_factor_count":0}]}"#;
        let value: LowIndicatorRanking = serde_json::from_str(ranking).unwrap();
        assert_eq!(value.items.len(), 1);
        assert_eq!(value.items[0].sample_count, 3);
        assert!(serde_json::from_str::<LowIndicatorRanking>(
            &ranking.replace("\"items\":", "\"answers\":[],\"items\":")
        )
        .is_err());
    }
}
