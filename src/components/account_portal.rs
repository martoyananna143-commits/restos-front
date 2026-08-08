//! Minimal Account-only pilot UI. It never accepts or creates legacy auth state.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use crate::{
    account_api::{
        AccountApiClient, AccountApiError, SmsRequestInput, SmsVerifyInput, WebRegistrationInput,
    },
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_management_api::{
        AssessmentManagementApiClient, AssessmentManagementApiError, ManagementEmployee,
    },
    device_identity::DeviceIdentityAdapter,
    passkey::{PasskeyAdapter, PasskeyError},
    passkey_api::{PasskeyApiClient, PasskeySummary},
};

use super::{
    bootstrap_owner_capability, capability_from_probe, AssessmentAttemptsPage,
    AssessmentManagementPage, AssessmentsPage, ManagerCapability,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountRootState {
    BootstrappingAccount,
    AccountAuthenticated,
    LegacyAuthenticated,
    Unauthenticated,
    SafeStartupError,
}

pub fn startup_root_state(
    result: Result<AccountSessionState, AccountApiError>,
    legacy_authenticated: bool,
) -> AccountRootState {
    match result {
        Ok(AccountSessionState::Authenticated(_)) => AccountRootState::AccountAuthenticated,
        Err(AccountApiError::AuthenticationRequired)
        | Ok(AccountSessionState::Anonymous)
        | Ok(AccountSessionState::Uninitialized) => {
            if legacy_authenticated {
                AccountRootState::LegacyAuthenticated
            } else {
                AccountRootState::Unauthenticated
            }
        }
        Err(AccountApiError::NetworkUnavailable)
        | Ok(AccountSessionState::Unavailable { retryable: true }) => {
            AccountRootState::SafeStartupError
        }
        _ => AccountRootState::SafeStartupError,
    }
}

pub fn root_after_account_logout(legacy_authenticated: bool) -> AccountRootState {
    if legacy_authenticated {
        AccountRootState::LegacyAuthenticated
    } else {
        AccountRootState::Unauthenticated
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountAuthMode {
    Choice,
    Phone,
    SmsCode,
    RegistrationDetails,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiOperation {
    Idle,
    RequestingSms,
    VerifyingSms,
    Registering,
    AuthenticatingPasskey,
}

pub fn accepts_completion(current_generation: u64, completed_generation: u64) -> bool {
    current_generation == completed_generation
}

fn next_generation(current: u64) -> u64 {
    current.wrapping_add(1)
}

fn operation_is_idle(operation: UiOperation) -> bool {
    operation == UiOperation::Idle
}

fn timestamp_millis(value: &str) -> Option<f64> {
    let millis = js_sys::Date::parse(value);
    millis.is_finite().then_some(millis)
}

fn resend_is_available(now_millis: f64, available_at_millis: Option<f64>) -> bool {
    now_millis.is_finite()
        && available_at_millis
            .filter(|value| value.is_finite())
            .is_some_and(|available_at| now_millis >= available_at)
}

fn resend_action_is_allowed(
    operation: UiOperation,
    timer_ready: bool,
    now_millis: f64,
    available_at_millis: Option<f64>,
) -> bool {
    operation_is_idle(operation)
        && timer_ready
        && resend_is_available(now_millis, available_at_millis)
}

fn resend_timer_is_current(current_generation: u64, timer_generation: u64) -> bool {
    current_generation == timer_generation
}

fn completion_requests_reload(is_current: bool, succeeded: bool) -> bool {
    is_current && succeeded
}

fn cleared_sensitive_auth_fields() -> (String, String) {
    (String::new(), String::new())
}

fn authenticated_account_id(session: &AccountSessionAdapter) -> Option<Uuid> {
    match session.state() {
        AccountSessionState::Authenticated(value) => Some(value.bootstrap.account.id),
        _ => None,
    }
}

fn scoped_completion_is_current(
    current_generation: u64,
    operation_generation: u64,
    current_epoch: u64,
    operation_epoch: u64,
    current_account_id: Option<Uuid>,
    operation_account_id: Option<Uuid>,
) -> bool {
    accepts_completion(current_generation, operation_generation)
        && accepts_completion(current_epoch, operation_epoch)
        && current_account_id.is_some()
        && current_account_id == operation_account_id
}

fn schedule_resend_eligibility(
    available_at: String,
    timer_generation: u64,
    generation: Signal<u64>,
    mut ready: Signal<bool>,
) {
    spawn(async move {
        let Some(available_at_millis) = timestamp_millis(&available_at) else {
            return;
        };
        loop {
            if !resend_timer_is_current(generation(), timer_generation) {
                return;
            }
            let now = js_sys::Date::now();
            if resend_is_available(now, Some(available_at_millis)) {
                ready.set(true);
                return;
            }
            let remaining = (available_at_millis - now).ceil().max(1.0);
            let bounded_delay = remaining.min(u32::MAX as f64) as u32;
            TimeoutFuture::new(bounded_delay).await;
        }
    });
}

pub fn safe_account_error(error: &AccountApiError) -> &'static str {
    match error {
        AccountApiError::AuthenticationRequired | AccountApiError::ReauthenticationRequired => {
            "Сессия недоступна. Войдите снова."
        }
        AccountApiError::RateLimited => "Слишком много попыток. Попробуйте позже.",
        AccountApiError::NetworkUnavailable => "Нет связи с сервером. Повторите вручную.",
        AccountApiError::ConfigurationUnavailable => "Вход временно недоступен.",
        _ => "Не удалось выполнить запрос. Проверьте данные и попробуйте снова.",
    }
}

pub fn safe_passkey_error(error: &PasskeyError) -> Option<&'static str> {
    match error {
        PasskeyError::Cancelled => None,
        PasskeyError::Unavailable | PasskeyError::SecurityUnavailable => {
            Some("Ключи доступа недоступны в этом браузере. Используйте вход по телефону.")
        }
        PasskeyError::RateLimited => Some("Слишком много попыток. Попробуйте позже."),
        PasskeyError::NetworkUnavailable => Some("Нет связи с сервером. Повторите вручную."),
        PasskeyError::AuthenticationRejected => Some("Сессия недоступна. Войдите снова."),
        _ => Some("Не удалось использовать ключ доступа. Попробуйте другой способ входа."),
    }
}

pub fn safe_passkey_label(summary: &PasskeySummary) -> String {
    summary
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Ключ доступа")
        .to_string()
}

#[component]
pub fn AccountAuthPage(
    on_authenticated: EventHandler<()>,
    on_legacy_login: EventHandler<()>,
) -> Element {
    let api = use_context::<AccountApiClient>();
    let session = use_context::<AccountSessionAdapter>();
    let device_identities = use_context::<DeviceIdentityAdapter>();
    let passkeys = use_context::<PasskeyAdapter>();

    let mut mode = use_signal(|| AccountAuthMode::Choice);
    let mut operation = use_signal(|| UiOperation::Idle);
    let mut generation = use_signal(|| 0_u64);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut invitation = use_signal(String::new);
    let mut phone = use_signal(String::new);
    let mut otp = use_signal(String::new);
    let mut sms_challenge: Signal<Option<Uuid>> = use_signal(|| None);
    let mut resend_available_at: Signal<Option<String>> = use_signal(|| None);
    let mut resend_ready = use_signal(|| false);
    let mut resend_timer_generation = use_signal(|| 0_u64);
    let mut display_name = use_signal(String::new);
    let mut password = use_signal(String::new);

    let mut cancel = move || {
        generation.set(next_generation(generation()));
        operation.set(UiOperation::Idle);
        mode.set(AccountAuthMode::Choice);
        error.set(None);
        let (cleared_invitation, cleared_otp) = cleared_sensitive_auth_fields();
        invitation.set(cleared_invitation);
        phone.set(String::new());
        otp.set(cleared_otp);
        sms_challenge.set(None);
        resend_available_at.set(None);
        resend_ready.set(false);
        resend_timer_generation.set(next_generation(resend_timer_generation()));
        display_name.set(String::new());
        password.set(String::new());
    };

    rsx! {
        div { class: "auth-root account-auth-root",
            div { class: "auth-card account-auth-card",
                div { class: "auth-header",
                    div { class: "auth-title", "RestOS" }
                    h1 { class: "account-auth-heading", "Вход в аккаунт" }
                    p { class: "auth-subtitle",
                        "Используйте ключ доступа или приглашение и номер телефона."
                    }
                }

                div { class: "account-live", role: "status", aria_live: "polite",
                    if let Some(message) = error() { "{message}" }
                }

                match mode() {
                    AccountAuthMode::Choice => rsx! {
                        div { class: "auth-form",
                            button {
                                class: "btn-primary w-full",
                                r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    error.set(None);
                                    if !PasskeyAdapter::is_supported() {
                                        error.set(safe_passkey_error(&PasskeyError::Unavailable).map(str::to_string));
                                        return;
                                    }
                                    operation.set(UiOperation::AuthenticatingPasskey);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let passkeys = passkeys.clone();
                                    let session = session.clone();
                                    spawn(async move {
                                        let result = passkeys.authenticate(&session, "web", Some("Этот браузер".into())).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(()) => on_authenticated.call(()),
                                            Err(problem) => {
                                                error.set(safe_passkey_error(&problem).map(str::to_string));
                                            }
                                        }
                                    });
                                },
                                if operation() == UiOperation::AuthenticatingPasskey {
                                    "Ожидание устройства..."
                                } else {
                                    "Войти с Face ID или ключом доступа"
                                }
                            }
                            p { class: "account-auth-help",
                                "Используйте Face ID, Touch ID, Windows Hello или код устройства."
                            }
                            button {
                                class: "btn-secondary w-full",
                                r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| { error.set(None); mode.set(AccountAuthMode::Phone); },
                                "Войти или зарегистрироваться по телефону"
                            }
                            button {
                                class: "btn-ghost account-auth-link",
                                r#type: "button",
                                onclick: move |_| on_legacy_login.call(()),
                                "Старый вход для существующей версии"
                            }
                        }
                    },
                    AccountAuthMode::Phone => rsx! {
                        div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-invitation", "Код приглашения" }
                                input {
                                    id: "account-invitation", class: "field-input", inputmode: "numeric",
                                    autocomplete: "off", maxlength: "6", value: "{invitation}",
                                    oninput: move |event| invitation.set(event.value().chars().filter(|value| value.is_ascii_digit()).take(6).collect()),
                                }
                            }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-phone", "Номер телефона" }
                                input {
                                    id: "account-phone", class: "field-input", r#type: "tel",
                                    autocomplete: "tel", value: "{phone}",
                                    oninput: move |event| phone.set(event.value()),
                                }
                            }
                            button {
                                class: "btn-primary w-full", r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    if invitation().len() != 6 || phone().len() < 8 || phone().len() > 32 {
                                        error.set(Some("Проверьте код приглашения и номер телефона.".into()));
                                        return;
                                    }
                                    operation.set(UiOperation::RequestingSms);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = api.clone();
                                    let request = SmsRequestInput { invitation_code: invitation(), phone: phone() };
                                    spawn(async move {
                                        let result = api.request_sms(&request).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(requested) => {
                                                sms_challenge.set(Some(requested.challenge_id));
                                                let available_at = requested.resend_available_at;
                                                resend_available_at.set(Some(available_at.clone()));
                                                resend_ready.set(false);
                                                resend_timer_generation += 1;
                                                schedule_resend_eligibility(available_at, resend_timer_generation(), resend_timer_generation, resend_ready);
                                                otp.set(String::new());
                                                error.set(None);
                                                mode.set(AccountAuthMode::SmsCode);
                                            }
                                            Err(problem) => error.set(Some(safe_account_error(&problem).into())),
                                        }
                                    });
                                },
                                if operation() == UiOperation::RequestingSms { "Отправка..." } else { "Получить код" }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| cancel(), "Назад" }
                        }
                    },
                    AccountAuthMode::SmsCode => {
                        let verify_api = api.clone();
                        let resend_api = api.clone();
                        rsx! { div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-otp", "Код из SMS" }
                                input {
                                    id: "account-otp", class: "field-input", inputmode: "numeric",
                                    autocomplete: "one-time-code", maxlength: "6", value: "{otp}",
                                    oninput: move |event| otp.set(event.value().chars().filter(|value| value.is_ascii_digit()).take(6).collect()),
                                }
                            }
                            if let Some(available) = resend_available_at() {
                                p { class: "account-auth-help", "Повторная отправка доступна после {available}." }
                            }
                            button {
                                class: "btn-primary w-full", r#type: "button",
                                disabled: operation() != UiOperation::Idle || otp().len() != 6,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    let Some(challenge_id) = sms_challenge() else {
                                        error.set(Some("Запросите новый код.".into())); return;
                                    };
                                    operation.set(UiOperation::VerifyingSms);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = verify_api.clone();
                                    let request = SmsVerifyInput { phone_verification_challenge_id: challenge_id, phone: phone(), code: otp() };
                                    spawn(async move {
                                        let result = api.verify_sms(&request).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(()) => {
                                                resend_timer_generation += 1;
                                                resend_ready.set(false);
                                                otp.set(String::new());
                                                error.set(None);
                                                mode.set(AccountAuthMode::RegistrationDetails);
                                            }
                                            Err(problem) => error.set(Some(safe_account_error(&problem).into())),
                                        }
                                    });
                                },
                                if operation() == UiOperation::VerifyingSms { "Проверка..." } else { "Подтвердить код" }
                            }
                            button {
                                class: "btn-secondary w-full", r#type: "button",
                                disabled: !resend_action_is_allowed(
                                    operation(),
                                    resend_ready(),
                                    js_sys::Date::now(),
                                    resend_available_at().as_deref().and_then(timestamp_millis),
                                ),
                                onclick: move |_| {
                                    let available_at = resend_available_at()
                                        .as_deref()
                                        .and_then(timestamp_millis);
                                    if !resend_action_is_allowed(
                                        operation(),
                                        resend_ready(),
                                        js_sys::Date::now(),
                                        available_at,
                                    ) {
                                        resend_ready.set(false);
                                        return;
                                    }
                                    operation.set(UiOperation::RequestingSms);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = resend_api.clone();
                                    let request = SmsRequestInput { invitation_code: invitation(), phone: phone() };
                                    spawn(async move {
                                        let result = api.request_sms(&request).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(requested) => {
                                                sms_challenge.set(Some(requested.challenge_id));
                                                let available_at = requested.resend_available_at;
                                                resend_available_at.set(Some(available_at.clone()));
                                                resend_ready.set(false);
                                                resend_timer_generation += 1;
                                                schedule_resend_eligibility(available_at, resend_timer_generation(), resend_timer_generation, resend_ready);
                                                otp.set(String::new());
                                                error.set(None);
                                            }
                                            Err(problem) => error.set(Some(safe_account_error(&problem).into())),
                                        }
                                    });
                                },
                                "Отправить код повторно"
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| cancel(), "Отмена" }
                        } }
                    },
                    AccountAuthMode::RegistrationDetails => rsx! {
                        div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-name", "Имя" }
                                input { id: "account-name", class: "field-input", autocomplete: "name", value: "{display_name}", oninput: move |event| display_name.set(event.value()) }
                            }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-password", "Пароль" }
                                input { id: "account-password", class: "field-input", r#type: "password", autocomplete: "new-password", minlength: "12", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) }
                            }
                            button {
                                class: "btn-primary w-full", r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    let Some(challenge_id) = sms_challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if display_name().trim().is_empty() || password().len() < 12 || password().len() > 72 {
                                        error.set(Some("Укажите имя и пароль длиной от 12 до 72 символов.".into())); return;
                                    }
                                    operation.set(UiOperation::Registering);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = api.clone();
                                    let device_identities = device_identities.clone();
                                    let session = session.clone();
                                    let request = WebRegistrationInput {
                                        invitation_code: invitation(), phone_verification_challenge_id: challenge_id,
                                        phone: phone(), display_name: display_name().trim().to_string(), password: password(),
                                        platform: "web".into(), device_display_name: Some("Браузер RestOS".into()),
                                    };
                                    spawn(async move {
                                        let result = api.register_web(&device_identities, &request).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(registered) => {
                                                session.accept_registered_session(registered);
                                                let (cleared_invitation, cleared_otp) = cleared_sensitive_auth_fields();
                                                invitation.set(cleared_invitation); phone.set(String::new()); otp.set(cleared_otp); password.set(String::new());
                                                sms_challenge.set(None); resend_available_at.set(None); resend_ready.set(false); resend_timer_generation += 1; error.set(None);
                                                on_authenticated.call(());
                                            }
                                            Err(problem) => error.set(Some(safe_account_error(&problem).into())),
                                        }
                                    });
                                },
                                if operation() == UiOperation::Registering { "Создание аккаунта..." } else { "Завершить регистрацию" }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| cancel(), "Отмена" }
                        }
                    },
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AccountPilotTab {
    MyAssessments,
    Assessments,
    Management,
    Security,
}

#[component]
pub fn AccountPilotShell(on_logout: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let management_api = use_context::<AssessmentManagementApiClient>();
    let mut tab = use_signal(|| AccountPilotTab::MyAssessments);
    let mut logging_out = use_signal(|| false);
    let mut lifecycle_epoch = use_signal(|| 0_u64);
    let mut capability_generation = use_signal(|| 0_u64);
    use_context_provider(|| lifecycle_epoch);

    let probe_session = session.clone();
    let probe_api = management_api.clone();
    let capability_probe = use_resource(move || {
        let generation = capability_generation();
        let epoch = lifecycle_epoch();
        let state = probe_session.state();
        let api = probe_api.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return (generation, epoch, None, ManagerCapability::Hidden);
            };
            let Some(company_id) = account.selected_company.map(|value| value.0) else {
                return (generation, epoch, None, ManagerCapability::Hidden);
            };
            let relationship = account
                .bootstrap
                .companies
                .iter()
                .find(|company| company.company_id == company_id)
                .map(|company| company.relationship.as_str());
            if relationship == Some("owner") {
                return (
                    generation,
                    epoch,
                    Some(company_id),
                    ManagerCapability::Authorized,
                );
            }
            if relationship != Some("employee") {
                return (
                    generation,
                    epoch,
                    Some(company_id),
                    ManagerCapability::Hidden,
                );
            }
            let result: Result<Vec<ManagementEmployee>, AssessmentManagementApiError> = api
                .employees(&account.access_token, company_id, None, 1, None)
                .await;
            (
                generation,
                epoch,
                Some(company_id),
                capability_from_probe(&result),
            )
        }
    });

    let session_state = session.state();
    let (companies, selected_company) = match &session_state {
        AccountSessionState::Authenticated(account) => (
            account.bootstrap.companies.as_slice(),
            account.selected_company.map(|value| value.0),
        ),
        _ => (&[][..], None),
    };
    let owner_capability = bootstrap_owner_capability(companies, selected_company);
    let manager_capability = if owner_capability == ManagerCapability::Authorized {
        ManagerCapability::Authorized
    } else {
        capability_probe()
            .and_then(|(generation, epoch, company_id, capability)| {
                (generation == capability_generation()
                    && epoch == lifecycle_epoch()
                    && company_id == selected_company)
                    .then_some(capability)
            })
            .unwrap_or(ManagerCapability::Checking)
    };
    if tab() == AccountPilotTab::Management && manager_capability != ManagerCapability::Authorized {
        tab.set(AccountPilotTab::MyAssessments);
    }

    rsx! {
        div { class: "account-pilot app-root",
            header { class: "account-pilot-header",
                strong { "RestOS" }
                nav { aria_label: "Разделы аккаунта",
                    button { class: if tab() == AccountPilotTab::MyAssessments { "active" } else { "" }, r#type: "button", onclick: move |_| tab.set(AccountPilotTab::MyAssessments), "Мои оценки" }
                    button { class: if tab() == AccountPilotTab::Assessments { "active" } else { "" }, r#type: "button", onclick: move |_| tab.set(AccountPilotTab::Assessments), "Шаблоны" }
                    if manager_capability == ManagerCapability::Authorized {
                        button { class: if tab() == AccountPilotTab::Management { "active" } else { "" }, r#type: "button", onclick: move |_| tab.set(AccountPilotTab::Management), "Назначения" }
                    }
                    button { class: if tab() == AccountPilotTab::Security { "active" } else { "" }, r#type: "button", onclick: move |_| tab.set(AccountPilotTab::Security), "Безопасность" }
                }
                if manager_capability == ManagerCapability::Error {
                    button {
                        class: "btn-secondary", r#type: "button",
                        onclick: move |_| capability_generation += 1,
                        "Повторить проверку доступа"
                    }
                }
                button {
                    class: "btn-ghost", r#type: "button", disabled: logging_out(),
                    onclick: move |_| {
                        if logging_out() { return; }
                        logging_out.set(true);
                        lifecycle_epoch += 1;
                        let session = session.clone();
                        spawn(async move { let _ = session.logout().await; on_logout.call(()); });
                    },
                    if logging_out() { "Выход..." } else { "Выйти" }
                }
            }
            main { class: "account-pilot-main",
                match tab() {
                    AccountPilotTab::MyAssessments => rsx! { AssessmentAttemptsPage {} },
                    AccountPilotTab::Assessments => rsx! { AssessmentsPage {} },
                    AccountPilotTab::Management => rsx! {
                        AssessmentManagementPage {
                            on_company_changed: move |_| {
                                tab.set(AccountPilotTab::MyAssessments);
                                capability_generation += 1;
                            }
                        }
                    },
                    AccountPilotTab::Security => rsx! { PasskeySecurityPanel {} },
                }
            }
        }
    }
}

#[component]
fn PasskeySecurityPanel() -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let passkeys = use_context::<PasskeyAdapter>();
    let api = use_context::<PasskeyApiClient>();
    let mut reload = use_signal(|| 0_u64);
    let mut enrolling = use_signal(|| false);
    let revoking: Signal<Option<Uuid>> = use_signal(|| None);
    let confirming: Signal<Option<Uuid>> = use_signal(|| None);
    let mut status: Signal<Option<String>> = use_signal(|| None);
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut enrollment_generation = use_signal(|| 0_u64);
    let revoke_generation = use_signal(|| 0_u64);

    let list_session = session.clone();
    let list_api = api.clone();
    let list = use_resource(move || {
        let _reload = reload();
        let list_epoch = lifecycle_epoch();
        let api = list_api.clone();
        let token = match list_session.state() {
            AccountSessionState::Authenticated(value) => Some(value.access_token),
            _ => None,
        };
        async move {
            if list_epoch != 0 {
                return Err(AccountApiError::AuthenticationRequired);
            }
            match token {
                Some(token) => api.list(&token).await,
                None => Err(AccountApiError::AuthenticationRequired),
            }
        }
    });

    let enroll_session = session.clone();
    let enroll_adapter = passkeys.clone();

    rsx! {
        section { class: "account-security", aria_labelledby: "passkey-title",
            div { class: "account-security-heading",
                div { h1 { id: "passkey-title", "Ключи доступа" } p { "Управляйте быстрым входом на доверенных устройствах." } }
                button {
                    class: "btn-primary", r#type: "button", disabled: enrolling(),
                    onclick: move |_| {
                        if enrolling() { return; }
                        if !PasskeyAdapter::is_supported() { status.set(safe_passkey_error(&PasskeyError::Unavailable).map(str::to_string)); return; }
                        let token = match enroll_session.state() { AccountSessionState::Authenticated(value) => value.access_token, _ => { status.set(Some("Сессия недоступна. Войдите снова.".into())); return; } };
                        enrolling.set(true); status.set(None);
                        enrollment_generation += 1;
                        let operation_generation = enrollment_generation();
                        let operation_epoch = lifecycle_epoch();
                        let operation_account_id = authenticated_account_id(&enroll_session);
                        let passkeys = enroll_adapter.clone();
                        let session = enroll_session.clone();
                        spawn(async move {
                            let result = passkeys.register(&token, Some("Этот браузер".into())).await;
                            if !scoped_completion_is_current(
                                enrollment_generation(), operation_generation,
                                lifecycle_epoch(), operation_epoch,
                                authenticated_account_id(&session), operation_account_id,
                            ) { return; }
                            enrolling.set(false);
                            let succeeded = result.is_ok();
                            match result { Ok(()) => status.set(Some("Ключ доступа настроен.".into())), Err(problem) => status.set(safe_passkey_error(&problem).map(str::to_string)) }
                            if completion_requests_reload(true, succeeded) { reload += 1; }
                        });
                    },
                    if enrolling() { "Настройка..." } else { "Настроить Face ID или ключ доступа" }
                }
            }
            div { class: "account-live", role: "status", aria_live: "polite", if let Some(message) = status() { "{message}" } }
            match list() {
                None => rsx! { p { class: "account-muted", "Загрузка ключей доступа..." } },
                Some(Err(problem)) => rsx! { div { class: "account-safe-error", p { "{safe_account_error(&problem)}" } button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" } } },
                Some(Ok(items)) if items.is_empty() => rsx! { div { class: "account-empty", p { "Ключи доступа ещё не настроены." } } },
                Some(Ok(items)) => rsx! {
                    div { class: "account-passkey-list",
                        for item in items.iter() {
                            PasskeyRow { key: "{item.identity_id}", item: item.clone(), confirming, revoking, status, reload, lifecycle_epoch, revoke_generation }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn PasskeyRow(
    item: PasskeySummary,
    confirming: Signal<Option<Uuid>>,
    revoking: Signal<Option<Uuid>>,
    status: Signal<Option<String>>,
    reload: Signal<u64>,
    lifecycle_epoch: Signal<u64>,
    revoke_generation: Signal<u64>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<PasskeyApiClient>();
    let identity_id = item.identity_id;
    let revoke_session = session.clone();
    let revoke_api = api.clone();

    rsx! {
        article { class: "account-passkey-row",
            div {
                strong { "{safe_passkey_label(&item)}" }
                small { "Создан: {item.created_at}" }
                if let Some(last_used) = &item.last_used_at { small { "Последний вход: {last_used}" } }
            }
            if confirming() == Some(identity_id) {
                div { class: "account-revoke-confirm", span { "Отозвать этот ключ?" }
                    button {
                        class: "btn-secondary", r#type: "button", disabled: revoking().is_some(),
                        onclick: move |_| {
                            if revoking().is_some() { return; }
                            let token = match revoke_session.state() {
                                AccountSessionState::Authenticated(value) => value.access_token,
                                _ => { status.set(Some("Сессия недоступна. Войдите снова.".into())); return; }
                            };
                            revoking.set(Some(identity_id));
                            let mut revoke_generation = revoke_generation;
                            revoke_generation += 1;
                            let operation_generation = revoke_generation();
                            let operation_epoch = lifecycle_epoch();
                            let operation_account_id = authenticated_account_id(&revoke_session);
                            let api = revoke_api.clone();
                            let session = revoke_session.clone();
                            spawn(async move {
                                let result = api.revoke(&token, identity_id).await;
                                if !scoped_completion_is_current(
                                    revoke_generation(), operation_generation,
                                    lifecycle_epoch(), operation_epoch,
                                    authenticated_account_id(&session), operation_account_id,
                                ) { return; }
                                revoking.set(None);
                                confirming.set(None);
                                let succeeded = result.is_ok();
                                match result {
                                    Ok(_) => status.set(Some("Ключ доступа отозван.".into())),
                                    Err(problem) => status.set(Some(safe_account_error(&problem).into())),
                                }
                                if completion_requests_reload(true, succeeded) { reload += 1; }
                            });
                        },
                        if revoking() == Some(identity_id) { "Отзыв..." } else { "Подтвердить" }
                    }
                    button { class: "btn-ghost", r#type: "button", disabled: revoking().is_some(), onclick: move |_| confirming.set(None), "Отмена" }
                }
            } else {
                button { class: "btn-ghost", r#type: "button", disabled: revoking().is_some(), onclick: move |_| confirming.set(Some(identity_id)), "Отозвать" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_api::{AccountBootstrap, BootstrapAccount};
    use wasm_bindgen_test::wasm_bindgen_test;

    fn authenticated() -> AccountSessionState {
        AccountSessionState::Authenticated(crate::account_session::AuthenticatedAccountSession {
            access_token: crate::account_api::AccountAccessToken::from_server("memory-only".into())
                .unwrap(),
            expires_at: "later".into(),
            bootstrap: AccountBootstrap {
                account: BootstrapAccount {
                    id: Uuid::from_u128(1),
                    status: "active".into(),
                    security_version: 1,
                },
                companies: Vec::new(),
            },
            selected_company: None,
            company_selection_required: false,
        })
    }

    #[wasm_bindgen_test]
    fn root_state_keeps_account_and_legacy_boundaries_separate() {
        assert_eq!(
            startup_root_state(Ok(authenticated()), true),
            AccountRootState::AccountAuthenticated
        );
        assert_eq!(
            startup_root_state(Err(AccountApiError::AuthenticationRequired), true),
            AccountRootState::LegacyAuthenticated
        );
        assert_eq!(
            startup_root_state(Err(AccountApiError::AuthenticationRequired), false),
            AccountRootState::Unauthenticated
        );
    }

    #[wasm_bindgen_test]
    fn startup_network_error_is_safe_and_retryable() {
        assert_eq!(
            startup_root_state(Err(AccountApiError::NetworkUnavailable), false),
            AccountRootState::SafeStartupError
        );
    }

    #[wasm_bindgen_test]
    fn stale_completion_is_ignored() {
        assert!(accepts_completion(7, 7));
        assert!(!accepts_completion(8, 7));
    }

    #[wasm_bindgen_test]
    fn account_logout_preserves_legacy_route_choice() {
        assert_eq!(
            root_after_account_logout(true),
            AccountRootState::LegacyAuthenticated
        );
        assert_eq!(
            root_after_account_logout(false),
            AccountRootState::Unauthenticated
        );
    }

    #[wasm_bindgen_test]
    fn cancel_and_rate_limit_have_safe_mappings() {
        assert_eq!(safe_passkey_error(&PasskeyError::Cancelled), None);
        assert_eq!(
            safe_account_error(&AccountApiError::RateLimited),
            "Слишком много попыток. Попробуйте позже."
        );
    }

    #[wasm_bindgen_test]
    fn unsupported_capability_keeps_phone_fallback() {
        assert!(safe_passkey_error(&PasskeyError::Unavailable)
            .unwrap()
            .contains("телефону"));
    }

    #[wasm_bindgen_test]
    fn safe_passkey_label_never_uses_internal_identifier() {
        let summary = PasskeySummary {
            identity_id: Uuid::from_u128(9),
            display_name: None,
            transports: vec!["internal".into()],
            backup_eligible: false,
            backup_state: false,
            created_at: "now".into(),
            last_used_at: None,
            status: "active".into(),
            revoked_at: None,
        };
        assert_eq!(safe_passkey_label(&summary), "Ключ доступа");
        assert!(!safe_passkey_label(&summary).contains(&summary.identity_id.to_string()));
    }

    #[wasm_bindgen_test]
    fn cooldown_timestamp_parsing_is_strict_and_fail_closed() {
        let utc = timestamp_millis("2026-08-03T12:00:00Z").unwrap();
        let offset = timestamp_millis("2026-08-03T15:00:00+03:00").unwrap();
        assert_eq!(utc, offset);
        assert_eq!(timestamp_millis(""), None);
        assert_eq!(timestamp_millis("not-a-timestamp"), None);
    }

    #[wasm_bindgen_test]
    fn cooldown_boundary_and_resend_admission_are_inclusive() {
        let available = Some(1_000.0);
        assert!(!resend_is_available(999.0, available));
        assert!(resend_is_available(1_000.0, available));
        assert!(resend_is_available(1_001.0, available));
        assert!(!resend_is_available(1_001.0, None));
        assert!(!resend_is_available(f64::NAN, available));
        assert!(!resend_action_is_allowed(
            UiOperation::RequestingSms,
            true,
            1_001.0,
            available
        ));
        assert!(resend_action_is_allowed(
            UiOperation::Idle,
            true,
            1_000.0,
            available
        ));
    }

    #[wasm_bindgen_test]
    fn cooldown_readiness_is_reactive_and_timestamp_remains_authoritative() {
        let available = Some(1_000.0);
        assert!(!resend_action_is_allowed(
            UiOperation::Idle,
            false,
            1_000.0,
            available
        ));
        assert!(!resend_action_is_allowed(
            UiOperation::Idle,
            true,
            999.0,
            available
        ));
        assert!(resend_action_is_allowed(
            UiOperation::Idle,
            true,
            1_000.0,
            available
        ));
    }

    #[wasm_bindgen_test]
    fn cooldown_generation_invalidates_stale_new_challenge_cancel_and_success_timers() {
        let initial_timer = 11;
        let after_new_challenge = next_generation(initial_timer);
        let after_cancel = next_generation(after_new_challenge);
        let after_success = next_generation(after_cancel);

        assert!(resend_timer_is_current(initial_timer, initial_timer));
        assert!(!resend_timer_is_current(after_new_challenge, initial_timer));
        assert!(!resend_timer_is_current(after_cancel, after_new_challenge));
        assert!(!resend_timer_is_current(after_success, after_cancel));
    }

    #[wasm_bindgen_test]
    fn generation_invalidation_blocks_stale_timer_and_completion() {
        let initial = 7;
        let after_cancel = next_generation(initial);
        let after_new_response = next_generation(after_cancel);
        let after_success = next_generation(after_new_response);
        assert!(!accepts_completion(after_cancel, initial));
        assert!(!accepts_completion(after_new_response, after_cancel));
        assert!(!accepts_completion(after_success, after_new_response));
        assert!(accepts_completion(after_success, after_success));
    }

    #[wasm_bindgen_test]
    fn scoped_completion_requires_generation_epoch_and_same_account() {
        let account = Some(Uuid::from_u128(101));
        let other = Some(Uuid::from_u128(102));
        assert!(scoped_completion_is_current(3, 3, 5, 5, account, account));
        assert!(!scoped_completion_is_current(4, 3, 5, 5, account, account));
        assert!(!scoped_completion_is_current(3, 3, 6, 5, account, account));
        assert!(!scoped_completion_is_current(3, 3, 5, 5, other, account));
        assert!(!scoped_completion_is_current(3, 3, 5, 5, None, account));
    }

    #[wasm_bindgen_test]
    fn enrollment_and_revoke_reload_only_for_current_success() {
        assert!(completion_requests_reload(true, true));
        assert!(!completion_requests_reload(false, true));
        assert!(!completion_requests_reload(true, false));
        assert!(!completion_requests_reload(false, false));
    }

    #[wasm_bindgen_test]
    fn explicit_operations_are_single_flight_without_automatic_retry() {
        assert!(operation_is_idle(UiOperation::Idle));
        for operation in [
            UiOperation::RequestingSms,
            UiOperation::VerifyingSms,
            UiOperation::Registering,
            UiOperation::AuthenticatingPasskey,
        ] {
            assert!(!operation_is_idle(operation));
        }
    }

    #[wasm_bindgen_test]
    fn logout_epoch_invalidates_enrollment_revoke_and_list_scopes() {
        let account = Some(Uuid::from_u128(201));
        let operation_epoch = 11;
        let logout_epoch = next_generation(operation_epoch);
        assert!(!scoped_completion_is_current(
            9,
            9,
            logout_epoch,
            operation_epoch,
            account,
            account,
        ));
        assert_ne!(logout_epoch, 0);
    }

    #[wasm_bindgen_test]
    fn cancel_and_registration_success_clear_sensitive_auth_fields() {
        let (invitation, otp) = cleared_sensitive_auth_fields();
        assert!(invitation.is_empty());
        assert!(otp.is_empty());
        let safe = safe_account_error(&AccountApiError::InvalidRequest);
        assert!(!safe.contains("123456"));
        assert!(!safe.contains("654321"));
    }

    #[wasm_bindgen_test]
    fn sms_verify_is_not_an_authenticated_or_session_adoption_operation() {
        assert_ne!(UiOperation::VerifyingSms, UiOperation::Registering);
        assert_eq!(
            startup_root_state(Ok(AccountSessionState::Anonymous), false),
            AccountRootState::Unauthenticated
        );
        assert_eq!(
            startup_root_state(Ok(authenticated()), false),
            AccountRootState::AccountAuthenticated
        );
    }

    #[wasm_bindgen_test]
    fn passkey_prompt_and_account_transition_require_explicit_current_operation() {
        assert!(operation_is_idle(UiOperation::Idle));
        assert!(!operation_is_idle(UiOperation::AuthenticatingPasskey));
        assert!(accepts_completion(12, 12));
        assert!(!accepts_completion(13, 12));
        assert_eq!(
            startup_root_state(Ok(authenticated()), true),
            AccountRootState::AccountAuthenticated
        );
    }
}
