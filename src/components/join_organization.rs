//! Authenticated invitation join for an existing verified Account.

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    organization_workflow_api::{
        JoinGroupInvitationRequest, OrganizationWorkflowApiClient, OrganizationWorkflowApiError,
        GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED,
    },
    workforce_api::{
        AcceptWorkforceInvitationRequest, AcceptWorkforceInvitationResponse, WorkforceApiClient,
        WorkforceApiError,
    },
};

fn invitation_digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).take(6).collect()
}

fn safe_error(error: &WorkforceApiError) -> &'static str {
    match error {
        WorkforceApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        WorkforceApiError::NetworkUnavailable => "Нет связи с сервером. Повторите вручную.",
        WorkforceApiError::Conflict => "Вы уже состоите в этой организации.",
        WorkforceApiError::PermissionDenied
        | WorkforceApiError::InvalidRequest
        | WorkforceApiError::InternalError => {
            "Приглашение недоступно. Проверьте код или запросите новый."
        }
    }
}

pub(crate) fn group_invitation_token() -> Option<String> {
    let hash = web_sys::window()?.location().hash().ok()?;
    let token = hash.strip_prefix("#token=")?;
    (token.len() == 43
        && token
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-')))
    .then(|| token.to_string())
}

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    Uuid::parse_str(&function.call0(&crypto).ok()?.as_string()?).ok()
}

fn safe_group_error(error: &OrganizationWorkflowApiError) -> &'static str {
    match error {
        OrganizationWorkflowApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        OrganizationWorkflowApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OrganizationWorkflowApiError::Conflict => "Заявка уже изменилась. Обновите страницу.",
        OrganizationWorkflowApiError::NotFound
        | OrganizationWorkflowApiError::Unavailable
        | OrganizationWorkflowApiError::InvalidRequest
        | OrganizationWorkflowApiError::InternalError => {
            "Ссылка недоступна. Запросите у владельца новую."
        }
    }
}

#[component]
pub fn JoinOrganizationPage() -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<WorkforceApiClient>();
    let group_api = use_context::<OrganizationWorkflowApiClient>();
    let mut lifecycle_epoch = use_context::<Signal<u64>>();
    let mut code = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut generation = use_signal(|| 0_u64);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| None::<AcceptWorkforceInvitationResponse>);
    let mut group_success = use_signal(|| false);
    let group_token = use_signal(group_invitation_token);
    let mut first_name = use_signal(String::new);
    let mut last_name = use_signal(String::new);
    let mut birth_date = use_signal(String::new);
    let mut group_request_id = use_signal(|| None::<Uuid>);
    let submit_session = session.clone();
    let submit_api = api.clone();

    rsx! {
        section { class: "journey-page join-organization-page", aria_labelledby: "join-organization-title",
            header { class: "journey-hero",
                p { class: "management-eyebrow", "ПРИГЛАШЕНИЕ" }
                h1 { id: "join-organization-title", "Присоединиться к организации" }
                p {
                    if group_token().is_some() {
                        "Заполните профиль. После отправки владелец активирует вашу заявку."
                    } else {
                        "Введите шестизначный код, который передал владелец организации."
                    }
                }
            }
            article { class: "journey-panel join-organization-card",
                if let Some(joined) = success() {
                    div { class: "journey-success", role: "status",
                        h2 { "✓ Приглашение принято" }
                        p { "Вы присоединились к:" }
                        strong { "{joined.company_name}" }
                        p { "Ресторан:" }
                        strong {
                            if joined.venue_names.is_empty() {
                                "Без привязки к ресторану"
                            } else {
                                {joined.venue_names.join(", ")}
                            }
                        }
                        p { "Должность:" }
                        strong { "{joined.position_name}" }
                        button {
                            class: "btn-primary",
                            r#type: "button",
                            onclick: move |_| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.location().set_hash("/today");
                                }
                            },
                            "Перейти в RestOS"
                        }
                    }
                } else if group_success() {
                    div { class: "journey-success", role: "status",
                        strong { "Организация добавлена" }
                        p { "Доступ обновлён. Можно продолжать работу в RestOS." }
                    }
                } else if group_token().is_some() && !GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED {
                    div { class: "journey-empty", role: "status",
                        strong { "Регистрация временно недоступна" }
                        p { "Групповое приглашение будет доступно после публикации обновлённых правовых документов." }
                    }
                } else if let Some(token) = group_token() {
                    form { onsubmit: move |event| {
                        event.prevent_default();
                        if submitting() || first_name().trim().is_empty() || last_name().trim().is_empty() || birth_date().is_empty() { return; }
                        let AccountSessionState::Authenticated(account) = submit_session.state() else {
                            error.set(Some("Сессия недоступна. Войдите снова.".into()));
                            return;
                        };
                        let Some(request_id) = group_request_id().or_else(browser_request_id) else {
                            error.set(Some("Не удалось подготовить безопасный запрос.".into()));
                            return;
                        };
                        group_request_id.set(Some(request_id));
                        submitting.set(true);
                        error.set(None);
                        generation += 1;
                        let operation_generation = generation();
                        let epoch = lifecycle_epoch();
                        let api = group_api.clone();
                        let body = JoinGroupInvitationRequest {
                            token: token.clone(),
                            request_id,
                            first_name: first_name().trim().to_string(),
                            last_name: last_name().trim().to_string(),
                            birth_date: birth_date(),
                        };
                        spawn(async move {
                            let result = api.join_group_invitation(&account.access_token, &body).await;
                            if lifecycle_epoch() != epoch || generation() != operation_generation { return; }
                            submitting.set(false);
                            match result {
                                Ok(_) => {
                                    first_name.set(String::new());
                                    last_name.set(String::new());
                                    birth_date.set(String::new());
                                    group_success.set(true);
                                }
                                Err(problem) => error.set(Some(safe_group_error(&problem).into())),
                            }
                        });
                    },
                        div { class: "form-field",
                            label { class: "field-label", r#for: "group-first-name", "Имя" }
                            input { id: "group-first-name", class: "field-input", autocomplete: "given-name", maxlength: "120", value: "{first_name}", disabled: submitting(), oninput: move |event| { first_name.set(event.value()); group_request_id.set(None); error.set(None); } }
                        }
                        div { class: "form-field",
                            label { class: "field-label", r#for: "group-last-name", "Фамилия" }
                            input { id: "group-last-name", class: "field-input", autocomplete: "family-name", maxlength: "120", value: "{last_name}", disabled: submitting(), oninput: move |event| { last_name.set(event.value()); group_request_id.set(None); error.set(None); } }
                        }
                        div { class: "form-field",
                            label { class: "field-label", r#for: "group-birth-date", "Дата рождения" }
                            input { id: "group-birth-date", class: "field-input", r#type: "date", autocomplete: "bday", value: "{birth_date}", disabled: submitting(), oninput: move |event| { birth_date.set(event.value()); group_request_id.set(None); error.set(None); } }
                        }
                        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
                        button { class: "btn-primary", r#type: "submit", disabled: submitting() || first_name().trim().is_empty() || last_name().trim().is_empty() || birth_date().is_empty(),
                            if submitting() { "Отправка…" } else { "Отправить заявку" }
                        }
                    }
                } else {
                    form { onsubmit: move |event| {
                        event.prevent_default();
                        if submitting() || code().len() != 6 { return; }
                        let AccountSessionState::Authenticated(account) = submit_session.state() else {
                            error.set(Some("Сессия недоступна. Войдите снова.".into()));
                            return;
                        };
                        submitting.set(true);
                        error.set(None);
                        generation += 1;
                        let operation_generation = generation();
                        let epoch = lifecycle_epoch();
                        let body = AcceptWorkforceInvitationRequest { invitation_code: code() };
                        let api = submit_api.clone();
                        let session = submit_session.clone();
                        spawn(async move {
                            let result = api.accept_invitation(&account.access_token, &body).await;
                            if lifecycle_epoch() != epoch || generation() != operation_generation { return; }
                            match result {
                                Ok(value) if value.joined => match session.reload_bootstrap().await {
                                    Ok(_) => {
                                        if lifecycle_epoch() != epoch || generation() != operation_generation { return; }
                                        code.set(String::new());
                                        success.set(Some(value));
                                        lifecycle_epoch += 1;
                                    }
                                    Err(_) => error.set(Some("Организация добавлена, но список пока не обновился. Обновите страницу.".into())),
                                },
                                Ok(_) => error.set(Some("Приглашение недоступно. Проверьте код или запросите новый.".into())),
                                Err(problem) => error.set(Some(safe_error(&problem).into())),
                            }
                            submitting.set(false);
                        });
                    },
                        div { class: "form-field",
                            label { class: "field-label", r#for: "existing-account-invitation-code", "Код приглашения" }
                            input {
                                id: "existing-account-invitation-code",
                                class: "field-input invitation-code-input",
                                r#type: "text",
                                inputmode: "numeric",
                                autocomplete: "one-time-code",
                                maxlength: "6",
                                pattern: "[0-9]{6}",
                                value: "{code}",
                                disabled: submitting(),
                                oninput: move |event| {
                                    code.set(invitation_digits(&event.value()));
                                    error.set(None);
                                },
                            }
                        }
                        if let Some(message) = error() {
                            p { class: "journey-error", role: "alert", "{message}" }
                        }
                        button { class: "btn-primary", r#type: "submit", disabled: submitting() || code().len() != 6,
                            if submitting() { "Присоединение…" } else { "Присоединиться" }
                        }
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
    fn invitation_code_accepts_only_six_ascii_digits() {
        assert_eq!(invitation_digits(" 12-34 56 "), "123456");
        assert_eq!(invitation_digits("123456789"), "123456");
        assert_eq!(invitation_digits("１２３456"), "456");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn public_errors_do_not_enumerate_account_or_phone_state() {
        let text = safe_error(&WorkforceApiError::InvalidRequest).to_lowercase();
        for forbidden in ["телефон", "account", "существ", "пользователь"]
        {
            assert!(!text.contains(forbidden));
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn group_errors_are_non_enumerating() {
        let text = safe_group_error(&OrganizationWorkflowApiError::NotFound).to_lowercase();
        for forbidden in ["телефон", "аккаунт", "существ", "зарегистрирован"]
        {
            assert!(!text.contains(forbidden));
        }
        assert!(!GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn accepted_invitation_surface_uses_projection_without_permission_inputs() {
        let source = include_str!("join_organization.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("✓ Приглашение принято"));
        assert!(production.contains("joined.company_name"));
        assert!(production.contains("joined.venue_names"));
        assert!(production.contains("joined.position_name"));
        assert!(!production.contains("access_profile_id"));
        assert!(!production.contains("position_id"));
    }
}
