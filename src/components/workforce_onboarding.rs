//! Minimal Account-only UI for creating one employee invitation.

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    workforce_api::{
        CreateWorkforceInvitationRequest, WorkforceApiClient, WorkforceApiError,
        WorkforceInvitationResponse,
    },
};

use super::russian_phone_input::{
    canonical_russian_phone, russian_phone_is_complete, RussianPhoneInput,
};

fn normalized_employee_name(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty() && normalized.chars().count() <= 255).then_some(normalized)
}

fn create_is_admitted(name: &str, phone: &str, in_flight: bool, read_only: bool) -> bool {
    normalized_employee_name(name).is_some()
        && russian_phone_is_complete(phone)
        && !in_flight
        && !read_only
}

fn safe_workforce_error(error: &WorkforceApiError) -> &'static str {
    match error {
        WorkforceApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        WorkforceApiError::PermissionDenied => "Нет доступа к приглашениям этой организации.",
        WorkforceApiError::InvalidRequest => {
            "Проверьте имя, номер телефона сотрудника и выбранный объект."
        }
        WorkforceApiError::Conflict => {
            "Не удалось создать отдельное приглашение. Проверьте список сотрудников и ожидающие приглашения."
        }
        WorkforceApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите действие вручную."
        }
        WorkforceApiError::InternalError => "Не удалось создать приглашение. Попробуйте позже.",
    }
}

fn current_scope(
    session: &AccountSessionAdapter,
) -> Option<(crate::account_api::AccountAccessToken, Uuid)> {
    let AccountSessionState::Authenticated(account) = session.state() else {
        return None;
    };
    Some((account.access_token, account.selected_company?.0))
}

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    if crypto.is_null() || crypto.is_undefined() {
        return None;
    }
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    let value = function.call0(&crypto).ok()?.as_string()?;
    Uuid::parse_str(&value).ok()
}

#[component]
pub fn WorkforceOnboardingPage() -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<WorkforceApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut employee_name = use_signal(String::new);
    let mut employee_phone = use_signal(String::new);
    let mut selected_venue = use_signal(|| None::<Uuid>);
    let mut request_id = use_signal(|| None::<Uuid>);
    let mut creating = use_signal(|| false);
    let mut operation_generation = use_signal(|| 0_u64);
    let mut error = use_signal(|| None::<String>);
    let mut created = use_signal(|| None::<WorkforceInvitationResponse>);
    let mut reload = use_signal(|| 0_u64);

    let list_session = session.clone();
    let list_api = api.clone();
    let venues = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = list_session.clone();
        let api = list_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(WorkforceApiError::AuthenticationRequired);
            };
            let result = api.venues(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(WorkforceApiError::AuthenticationRequired);
            }
            result
        }
    });

    let create_session = session.clone();
    let create_api = api.clone();
    let read_only = current_scope(&session).is_none();

    rsx! {
        section { class: "workforce-onboarding", aria_labelledby: "workforce-title",
            header { class: "management-hero",
                div {
                    p { class: "management-eyebrow", "RESTOS • КОМАНДА" }
                    h1 { id: "workforce-title", "Пригласить одного сотрудника" }
                    p { "Создайте одноразовый код и передайте его сотруднику безопасным способом." }
                }
            }
            if let Some(result) = created() {
                div { class: "card card-green workforce-invitation-result", role: "status",
                    p { class: "heading-md", "Приглашение готово" }
                    p { "Код действует ограниченное время и показывается только в этом рабочем процессе." }
                    output { class: "workforce-invitation-code", aria_label: "Код приглашения", "{result.invitation_code}" }
                    p { class: "account-auth-help", "Не отправляйте код в публичные чаты и не сохраняйте его в браузере." }
                    button {
                        class: "btn-secondary", r#type: "button",
                        onclick: move |_| {
                            created.set(None);
                            employee_name.set(String::new());
                            employee_phone.set(String::new());
                            selected_venue.set(None);
                            request_id.set(None);
                            error.set(None);
                        },
                        "Пригласить ещё"
                    }
                }
            } else {
                div { class: "card card-glass workforce-invitation-form",
                    if let Some(message) = error() {
                        div { class: "error-msg", role: "alert", "{message}" }
                    }
                    div { class: "form-field",
                        label { class: "field-label", r#for: "workforce-name", "Имя сотрудника" }
                        input {
                            id: "workforce-name", class: "field-input", maxlength: "255",
                            autocomplete: "off", value: "{employee_name}", disabled: creating() || read_only,
                            oninput: move |event| {
                                employee_name.set(event.value());
                                request_id.set(None);
                                error.set(None);
                            }
                        }
                    }
                    RussianPhoneInput {
                        id: "workforce-phone".to_string(),
                        label: "Номер телефона сотрудника".to_string(),
                        value: employee_phone(),
                        invalid: false,
                        disabled: creating() || read_only,
                        described_by: "workforce-phone-help".to_string(),
                        on_change: move |digits| {
                            employee_phone.set(digits);
                            request_id.set(None);
                            error.set(None);
                        }
                    }
                    p { id: "workforce-phone-help", class: "account-auth-help",
                        "Код подтверждения будет отправлен на указанный номер."
                    }
                    match venues() {
                        None => rsx! { p { class: "account-auth-help", "Загрузка объектов..." } },
                        Some(Err(problem)) => rsx! {
                            div { class: "error-msg", role: "alert", "{safe_workforce_error(&problem)}" }
                            button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить загрузку" }
                        },
                        Some(Ok(items)) if !items.is_empty() => rsx! {
                            div { class: "form-field",
                                label { class: "field-label", r#for: "workforce-venue", "Рабочий объект" }
                                select {
                                    id: "workforce-venue", class: "field-input", disabled: creating() || read_only,
                                    value: selected_venue().map(|value| value.to_string()).unwrap_or_default(),
                                    onchange: move |event| {
                                        selected_venue.set(Uuid::parse_str(&event.value()).ok());
                                        request_id.set(None);
                                        error.set(None);
                                    },
                                    option { value: "", "Без привязки к объекту" }
                                    for venue in items {
                                        option { key: "{venue.venue_id}", value: "{venue.venue_id}", "{venue.name}" }
                                    }
                                }
                            }
                        },
                        Some(Ok(_)) => rsx! { p { class: "account-auth-help", "Сотрудник будет добавлен без привязки к объекту." } },
                    }
                    button {
                        class: "btn-primary", r#type: "button",
                        disabled: !create_is_admitted(&employee_name(), &employee_phone(), creating(), read_only),
                        onclick: move |_| {
                            let Some(name) = normalized_employee_name(&employee_name()) else { return; };
                            let Some(phone) = canonical_russian_phone(&employee_phone()) else { return; };
                            let Some((token, company_id)) = current_scope(&create_session) else {
                                error.set(Some("Сессия недоступна. Войдите снова.".into()));
                                return;
                            };
                            let operation_request_id = match request_id() {
                                Some(value) => value,
                                None => {
                                    let Some(value) = browser_request_id() else {
                                        error.set(Some("Не удалось подготовить безопасный запрос. Обновите страницу.".into()));
                                        return;
                                    };
                                    request_id.set(Some(value));
                                    value
                                }
                            };
                            creating.set(true);
                            error.set(None);
                            operation_generation += 1;
                            let generation = operation_generation();
                            let epoch = lifecycle_epoch();
                            let api = create_api.clone();
                            let request = CreateWorkforceInvitationRequest {
                                request_id: operation_request_id,
                                employee_name: name,
                                phone,
                                venue_id: selected_venue(),
                            };
                            spawn(async move {
                                let result = api.create_invitation(&token, company_id, &request).await;
                                if lifecycle_epoch() != epoch || operation_generation() != generation {
                                    return;
                                }
                                creating.set(false);
                                match result {
                                    Ok(value) => {
                                        created.set(Some(value));
                                        employee_phone.set(String::new());
                                        request_id.set(None);
                                    }
                                    Err(problem) => error.set(Some(safe_workforce_error(&problem).into())),
                                }
                            });
                        },
                        if creating() { "Создание..." } else if error().is_some() { "Повторить создание" } else { "Создать приглашение" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn employee_name_is_bounded_and_normalized() {
        assert_eq!(
            normalized_employee_name("  Анна   Иванова ").as_deref(),
            Some("Анна Иванова")
        );
        assert!(normalized_employee_name("").is_none());
        assert!(normalized_employee_name(&"a".repeat(256)).is_none());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn admission_is_explicit_and_single_flight() {
        assert!(create_is_admitted(
            "Synthetic Employee",
            "9991234567",
            false,
            false
        ));
        assert!(!create_is_admitted("", "9991234567", false, false));
        assert!(!create_is_admitted(
            "Synthetic Employee",
            "123",
            false,
            false
        ));
        assert!(!create_is_admitted(
            "Synthetic Employee",
            "9991234567",
            true,
            false
        ));
        assert!(!create_is_admitted(
            "Synthetic Employee",
            "9991234567",
            false,
            true
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn workforce_uses_shared_phone_contract() {
        assert_eq!(
            canonical_russian_phone("+7 (999) 123-45-67").as_deref(),
            Some("+79991234567")
        );
        assert!(russian_phone_is_complete("9991234567"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn invitation_phone_is_not_persisted_or_logged_by_component() {
        let source = include_str!("workforce_onboarding.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source section must exist");
        for forbidden in ["localStorage", "sessionStorage", "indexedDB", "console.log"] {
            assert!(!production.contains(forbidden));
        }
        assert!(production.contains("employee_phone.set(String::new())"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn public_errors_do_not_enumerate_company_or_employee() {
        for error in [
            WorkforceApiError::PermissionDenied,
            WorkforceApiError::Conflict,
            WorkforceApiError::InternalError,
        ] {
            let message = safe_workforce_error(&error).to_lowercase();
            assert!(!message.contains("uuid"));
            assert!(!message.contains("существует"));
            assert!(!message.contains("телефон"));
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn individual_invitation_is_named_separately_from_group_onboarding() {
        let source = include_str!("workforce_onboarding.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("Пригласить одного сотрудника"));
        assert!(!production.contains("date_of_birth"));
        assert!(!production.contains("birth_date"));
    }
}
