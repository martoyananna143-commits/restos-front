//! Typed repeatable product taste/speed measurement client.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use gloo_net::http::{Request, Response};
#[cfg(target_arch = "wasm32")]
use web_sys::RequestCredentials;

use crate::account_api::AccountAccessToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductMeasurementApiError {
    AuthenticationRequired,
    PermissionDenied,
    NotFound,
    Conflict,
    InvalidRequest,
    NetworkUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProductItem {
    pub position_type: String,
    pub position_name: String,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub taste_score: f64,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub appearance_score: f64,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub output_score: f64,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub ticket_time_score: f64,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub planned_quantity: f64,
    #[serde(deserialize_with = "deserialize_decimal_f64")]
    pub actual_quantity: f64,
    pub quantity_unit: String,
    pub planned_ticket_seconds: u32,
    pub actual_ticket_seconds: u32,
    pub comment: Option<String>,
}

fn deserialize_decimal_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum DecimalValue {
        Number(f64),
        Text(String),
    }

    let value = match DecimalValue::deserialize(deserializer)? {
        DecimalValue::Number(value) => value,
        DecimalValue::Text(value) => value.parse().map_err(serde::de::Error::custom)?,
    };
    if !value.is_finite() {
        return Err(serde::de::Error::custom("decimal must be finite"));
    }
    Ok(value)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductMetricComponent {
    pub numerator: String,
    pub denominator: String,
    pub score_percent: String,
    pub coverage: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductMeasurementResult {
    pub scoring_algorithm: String,
    pub scoring_version: u16,
    pub item_count: usize,
    pub taste: ProductMetricComponent,
    pub speed: ProductMetricComponent,
    pub overall: ProductMetricComponent,
    pub items: Vec<ProductPositionResult>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProductPositionResult {
    pub position_index: usize,
    pub position_type: String,
    pub taste: ProductMetricComponent,
    pub speed: ProductMetricComponent,
    pub overall: ProductMetricComponent,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProductMeasurement {
    pub id: Uuid,
    pub company_id: Uuid,
    pub venue_id: Uuid,
    pub status: String,
    pub revision: i32,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub result: Option<ProductMeasurementResult>,
    pub items: Vec<ProductItem>,
}

#[derive(Serialize)]
struct CreateRequest {
    request_id: Uuid,
    venue_id: Uuid,
}
#[derive(Serialize)]
struct ReplaceRequest<'a> {
    expected_revision: i32,
    items: &'a [ProductItem],
}
#[derive(Serialize)]
struct CompleteRequest {
    expected_revision: i32,
}

#[derive(Clone, Debug)]
pub struct ProductMeasurementApiClient {
    base_url: String,
}

impl ProductMeasurementApiClient {
    pub fn new(base_url: String) -> Result<Self, ProductMeasurementApiError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        if base_url.is_empty() {
            return Err(ProductMeasurementApiError::InternalError);
        }
        Ok(Self { base_url })
    }

    pub const fn routes() -> [&'static str; 3] {
        [
            "/api/v1/account/companies/{company_id}/product-measurements",
            "/api/v1/account/companies/{company_id}/product-measurements/{measurement_id}",
            "/api/v1/account/companies/{company_id}/product-measurements/{measurement_id}/complete",
        ]
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn create(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        request_id: Uuid,
        venue_id: Uuid,
    ) -> Result<ProductMeasurement, ProductMeasurementApiError> {
        self.send(
            Request::post(&format!(
                "{}/api/v1/account/companies/{company_id}/product-measurements",
                self.base_url
            )),
            token,
            &CreateRequest {
                request_id,
                venue_id,
            },
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn replace(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        value: &ProductMeasurement,
        items: &[ProductItem],
    ) -> Result<ProductMeasurement, ProductMeasurementApiError> {
        self.send(
            Request::put(&format!(
                "{}/api/v1/account/companies/{company_id}/product-measurements/{}",
                self.base_url, value.id
            )),
            token,
            &ReplaceRequest {
                expected_revision: value.revision,
                items,
            },
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn complete(
        &self,
        token: &AccountAccessToken,
        company_id: Uuid,
        value: &ProductMeasurement,
    ) -> Result<ProductMeasurement, ProductMeasurementApiError> {
        self.send(
            Request::post(&format!(
                "{}/api/v1/account/companies/{company_id}/product-measurements/{}/complete",
                self.base_url, value.id
            )),
            token,
            &CompleteRequest {
                expected_revision: value.revision,
            },
        )
        .await
    }

    #[cfg(target_arch = "wasm32")]
    async fn send<T: DeserializeOwned, B: Serialize>(
        &self,
        request: gloo_net::http::RequestBuilder,
        token: &AccountAccessToken,
        body: &B,
    ) -> Result<T, ProductMeasurementApiError> {
        let response = request
            .credentials(RequestCredentials::Include)
            .header("Cache-Control", "no-cache")
            .header(
                "Authorization",
                &format!("Bearer {}", token.authorization_value()),
            )
            .json(body)
            .map_err(|_| ProductMeasurementApiError::InvalidRequest)?
            .send()
            .await
            .map_err(|_| ProductMeasurementApiError::NetworkUnavailable)?;
        parse_response(response).await
    }
}

#[cfg(target_arch = "wasm32")]
async fn parse_response<T: DeserializeOwned>(
    response: Response,
) -> Result<T, ProductMeasurementApiError> {
    if response.ok() {
        return response
            .json()
            .await
            .map_err(|_| ProductMeasurementApiError::InternalError);
    }
    Err(match response.status() {
        401 => ProductMeasurementApiError::AuthenticationRequired,
        403 => ProductMeasurementApiError::PermissionDenied,
        404 => ProductMeasurementApiError::NotFound,
        409 => ProductMeasurementApiError::Conflict,
        400 | 422 => ProductMeasurementApiError::InvalidRequest,
        _ => ProductMeasurementApiError::InternalError,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn product_measurement_declares_only_dynamic_lifecycle_routes() {
        assert_eq!(ProductMeasurementApiClient::routes().len(), 3);
        assert!(ProductMeasurementApiClient::routes()[2].ends_with("/complete"));
    }

    #[wasm_bindgen_test]
    fn create_request_carries_client_idempotency_key() {
        let request_id = Uuid::from_u128(1);
        let venue_id = Uuid::from_u128(2);
        let value = serde_json::to_value(CreateRequest {
            request_id,
            venue_id,
        })
        .unwrap();
        assert_eq!(value["request_id"], request_id.to_string());
        assert_eq!(value["venue_id"], venue_id.to_string());
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[wasm_bindgen_test]
    fn decimal_fields_accept_strict_api_strings_and_numbers() {
        let from_strings: ProductItem = serde_json::from_str(
            r#"{"position_type":"dish","position_name":"Суп","taste_score":"3.0","appearance_score":"2.5","output_score":"2","ticket_time_score":"3","planned_quantity":"250","actual_quantity":"245.5","quantity_unit":"g","planned_ticket_seconds":600,"actual_ticket_seconds":540,"comment":null}"#,
        )
        .expect("decimal strings are the FastAPI response contract");
        assert_eq!(from_strings.actual_quantity, 245.5);

        let from_numbers: ProductItem = serde_json::from_str(
            r#"{"position_type":"drink","position_name":"Чай","taste_score":3,"appearance_score":3,"output_score":3,"ticket_time_score":2,"planned_quantity":300,"actual_quantity":300,"quantity_unit":"ml","planned_ticket_seconds":300,"actual_ticket_seconds":310,"comment":null}"#,
        )
        .expect("JSON numbers remain accepted");
        assert_eq!(from_numbers.ticket_time_score, 2.0);
    }
}
