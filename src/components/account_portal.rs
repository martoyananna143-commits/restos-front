//! Minimal Account-only pilot UI. It never accepts or creates legacy auth state.

use std::{collections::HashSet, rc::Rc};

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{closure::Closure, JsCast};

use crate::{
    account_api::{
        AccountApiClient, AccountApiError, InvitationAcceptance, PasswordLoginInput,
        SmsRequestInput, SmsVerifyInput, WebRegistrationInput,
    },
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_management_api::{
        AssessmentManagementApiClient, AssessmentManagementApiError, ManagementEmployee,
    },
    device_identity::DeviceIdentityAdapter,
    navigation::{
        active_parent, assessment_result_id, authorized_target, default_child, navigable_target,
        resolve_hash, route_for, visible_items, MobilePlacement, NavigationIcon, NavigationId,
        NavigationProductState, NavigationRole, NavigationZone,
    },
    organization_access_api::{OrganizationAccessApiClient, OrganizationAccessProfile},
    passkey::{PasskeyAdapter, PasskeyError},
    passkey_api::{PasskeyApiClient, PasskeySummary},
    workforce_api::{AcceptWorkforceInvitationRequest, WorkforceApiClient, WorkforceApiError},
};

use super::{
    account_legal_notice::{
        registration_sms_request_allowed, AccountCreationLegalNotice, AccountLegalContext,
        AccountLegalNotice, RegistrationSmsLegalControls,
    },
    bootstrap_owner_capability, capability_from_probe,
    russian_phone_input::{canonical_russian_phone, russian_phone_is_complete, RussianPhoneInput},
    AssessmentAttemptsPage, AssessmentLibrarySection, AssessmentListView, AssessmentResultPage,
    AssessmentsPage, JoinOrganizationPage, ManagerCapability, MetricsNavigationView,
    OrganizationSettingsPage, OrganizationSettingsSection, RestaurantMetricsDashboardPage,
    TeamArea, TeamManagementPage,
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
    ExistingAccount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiOperation {
    Idle,
    RequestingSms,
    VerifyingSms,
    Registering,
    AuthenticatingPasskey,
    JoiningExistingAccount,
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

fn safe_existing_invitation_error(error: &WorkforceApiError) -> &'static str {
    match error {
        WorkforceApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        WorkforceApiError::NetworkUnavailable => "Нет связи с сервером. Повторите вручную.",
        WorkforceApiError::Conflict => "Приглашение уже принято этим аккаунтом.",
        WorkforceApiError::PermissionDenied
        | WorkforceApiError::InvalidRequest
        | WorkforceApiError::InternalError => {
            "Приглашение недоступно. Проверьте код или запросите новый."
        }
    }
}

fn safe_existing_login_error(error: &AccountApiError) -> &'static str {
    match error {
        AccountApiError::AuthenticationRequired
        | AccountApiError::PermissionDenied
        | AccountApiError::InvalidRequest => "Неверный номер телефона или пароль.",
        _ => safe_account_error(error),
    }
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
pub fn InvitationAccountAuthPage(
    on_authenticated: EventHandler<()>,
    on_legacy_login: EventHandler<()>,
    on_standalone: EventHandler<()>,
) -> Element {
    let api = use_context::<AccountApiClient>();
    let session = use_context::<AccountSessionAdapter>();
    let device_identities = use_context::<DeviceIdentityAdapter>();
    let workforce_api = use_context::<WorkforceApiClient>();
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
    let mut password = use_signal(String::new);
    let mut acceptance = use_signal(|| None::<InvitationAcceptance>);
    let mut personal_data_consent = use_signal(|| false);
    let mut authorization_sms_consent = use_signal(|| false);

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
        password.set(String::new());
        acceptance.set(None);
        personal_data_consent.set(false);
        authorization_sms_consent.set(false);
    };

    rsx! {
        div { class: "auth-root account-auth-root",
            div { class: "account-auth-shell",
                aside { class: "account-auth-brand", aria_hidden: "true",
                    div { class: "account-auth-brand__mark" }
                    div {
                        strong { "RestOS" }
                        span { "Операционная система ресторана" }
                    }
                }
                div { class: "auth-card account-auth-card",
                div { class: "auth-header",
                    img { class: "account-auth-mark", src: "/icons/icon-192.png", alt: "" }
                    div { class: "auth-title", "RestOS" }
                    h1 { class: "account-auth-heading", "Вас пригласили в RestOS" }
                    p { class: "auth-subtitle",
                        "Введите код из SMS. Код не передаётся в ссылке и доступен только вам."
                    }
                }

                div { class: if error().is_some() { "account-live account-live--error" } else { "account-live" }, role: if error().is_some() { "alert" } else { "status" }, aria_live: "polite",
                    if let Some(message) = error() { "{message}" }
                }

                if let Some(joined) = acceptance() {
                    div { class: "auth-form invitation-accepted", role: "status",
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
                            class: "btn-primary w-full",
                            r#type: "button",
                            onclick: move |_| {
                                if let Some(window) = web_sys::window() {
                                    let _ = window.location().set_hash("/today");
                                }
                                on_authenticated.call(());
                            },
                            "Перейти в RestOS"
                        }
                    }
                } else { match mode() {
                    AccountAuthMode::Choice => rsx! {
                        div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-invitation", "Введите код из SMS" }
                                input {
                                    id: "account-invitation", class: "field-input", inputmode: "numeric",
                                    autocomplete: "one-time-code", maxlength: "6", value: "{invitation}",
                                    oninput: move |event| {
                                        invitation.set(event.value().chars().filter(|value| value.is_ascii_digit()).take(6).collect());
                                        error.set(None);
                                    },
                                }
                            }
                            button {
                                class: "btn-primary w-full",
                                r#type: "button",
                                disabled: operation() != UiOperation::Idle || invitation().len() != 6,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle || invitation().len() != 6 { return; }
                                    error.set(None);
                                    mode.set(AccountAuthMode::Phone);
                                },
                                "Продолжить"
                            }
                            button {
                                class: "btn-ghost account-auth-link",
                                r#type: "button",
                                onclick: move |_| {
                                    error.set(None);
                                    phone.set(String::new());
                                    password.set(String::new());
                                    mode.set(AccountAuthMode::ExistingAccount);
                                },
                                "У меня уже есть аккаунт"
                            }
                            AccountLegalNotice { context: AccountLegalContext::Login }
                        }
                    },
                    AccountAuthMode::Phone => rsx! {
                        div { class: "auth-form",
                            RussianPhoneInput {
                                id: "account-phone".to_string(),
                                label: "Номер телефона".to_string(),
                                value: phone(),
                                invalid: false,
                                disabled: operation() != UiOperation::Idle,
                                described_by: String::new(),
                                on_change: move |digits| { phone.set(digits); error.set(None); }
                            }
                            RegistrationSmsLegalControls { personal_data_consent, authorization_sms_consent }
                            button {
                                class: "btn-primary w-full", r#type: "button",
                                disabled: operation() != UiOperation::Idle || invitation().len() != 6 || !russian_phone_is_complete(&phone()) || !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()),
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    if !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) { return; }
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else {
                                        error.set(Some("Проверьте код приглашения и номер телефона.".into()));
                                        return;
                                    };
                                    if invitation().len() != 6 { error.set(Some("Проверьте код приглашения и номер телефона.".into())); return; }
                                    operation.set(UiOperation::RequestingSms);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = api.clone();
                                    let request = SmsRequestInput { invitation_code: invitation(), phone: canonical_phone, personal_data_consent: true, authorization_sms_consent: true };
                                    personal_data_consent.set(false);
                                    authorization_sms_consent.set(false);
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
                            button {
                                class: "btn-ghost account-auth-link",
                                r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| on_standalone.call(()),
                                "У меня уже есть аккаунт"
                            }
                            button {
                                class: "btn-ghost account-auth-link",
                                r#type: "button",
                                onclick: move |_| {
                                    phone.set(String::new());
                                    personal_data_consent.set(false);
                                    authorization_sms_consent.set(false);
                                    error.set(None);
                                    mode.set(AccountAuthMode::Choice);
                                },
                                "Назад"
                            }
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
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let api = verify_api.clone();
                                    let request = SmsVerifyInput { phone_verification_challenge_id: challenge_id, phone: canonical_phone, code: otp() };
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
                            RegistrationSmsLegalControls { personal_data_consent, authorization_sms_consent }
                            button {
                                class: "btn-secondary w-full", r#type: "button",
                                disabled: !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) || !resend_action_is_allowed(
                                    operation(),
                                    resend_ready(),
                                    js_sys::Date::now(),
                                    resend_available_at().as_deref().and_then(timestamp_millis),
                                ),
                                onclick: move |_| {
                                    let available_at = resend_available_at()
                                        .as_deref()
                                        .and_then(timestamp_millis);
                                    if !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) || !resend_action_is_allowed(
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
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let api = resend_api.clone();
                                    let request = SmsRequestInput { invitation_code: invitation(), phone: canonical_phone, personal_data_consent: true, authorization_sms_consent: true };
                                    personal_data_consent.set(false);
                                    authorization_sms_consent.set(false);
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
                            p { class: "account-auth-help", "Имя, организация, ресторан, должность и доступ уже заданы руководителем." }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "account-password", "Пароль" }
                                input { id: "account-password", class: "field-input", r#type: "password", autocomplete: "new-password", minlength: "12", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) }
                            }
                            AccountCreationLegalNotice {}
                            button {
                                class: "btn-primary w-full", r#type: "button",
                                disabled: operation() != UiOperation::Idle,
                                onclick: move |_| {
                                    if operation() != UiOperation::Idle { return; }
                                    let Some(challenge_id) = sms_challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if password().len() < 12 || password().len() > 72 {
                                        error.set(Some("Укажите пароль длиной от 12 до 72 символов.".into())); return;
                                    }
                                    operation.set(UiOperation::Registering);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = api.clone();
                                    let device_identities = device_identities.clone();
                                    let session = session.clone();
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let request = WebRegistrationInput {
                                        invitation_code: invitation(), phone_verification_challenge_id: challenge_id,
                                        phone: canonical_phone, password: password(),
                                        platform: "web".into(), device_display_name: Some("Браузер RestOS".into()),
                                    };
                                    spawn(async move {
                                        let result = api.register_web(&device_identities, &request).await;
                                        if !accepts_completion(generation(), operation_generation) { return; }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(registered) => {
                                                let joined = registered.acceptance.clone();
                                                session.accept_registered_session(registered.session);
                                                let (cleared_invitation, cleared_otp) = cleared_sensitive_auth_fields();
                                                invitation.set(cleared_invitation); phone.set(String::new()); otp.set(cleared_otp); password.set(String::new());
                                                sms_challenge.set(None); resend_available_at.set(None); resend_ready.set(false); resend_timer_generation += 1; error.set(None);
                                                acceptance.set(Some(joined));
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
                    AccountAuthMode::ExistingAccount => {
                        let login_api = api.clone();
                        let login_identities = device_identities.clone();
                        let login_session = session.clone();
                        let accept_api = workforce_api.clone();
                        rsx! {
                            form {
                                class: "auth-form",
                                onsubmit: move |event| {
                                    event.prevent_default();
                                    if operation() != UiOperation::Idle || invitation().len() != 6 {
                                        return;
                                    }
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else {
                                        error.set(Some("Проверьте номер телефона и пароль.".into()));
                                        return;
                                    };
                                    if password().is_empty() {
                                        error.set(Some("Проверьте номер телефона и пароль.".into()));
                                        return;
                                    }
                                    operation.set(UiOperation::JoiningExistingAccount);
                                    generation += 1;
                                    let operation_generation = generation();
                                    let api = login_api.clone();
                                    let identities = login_identities.clone();
                                    let session = login_session.clone();
                                    let workforce = accept_api.clone();
                                    let login = PasswordLoginInput {
                                        phone: canonical_phone,
                                        password: password(),
                                        platform: "web".into(),
                                        device_display_name: Some("Браузер RestOS".into()),
                                    };
                                    let accept = AcceptWorkforceInvitationRequest {
                                        invitation_code: invitation(),
                                    };
                                    spawn(async move {
                                        let registered = match api.password_login(&identities, &login).await {
                                            Ok(registered) => registered,
                                            Err(problem) => {
                                                if accepts_completion(generation(), operation_generation) {
                                                    operation.set(UiOperation::Idle);
                                                    error.set(Some(safe_existing_login_error(&problem).into()));
                                                }
                                                return;
                                            }
                                        };
                                        let result = workforce
                                            .accept_invitation(&registered.access_token, &accept)
                                            .await;
                                        if !accepts_completion(generation(), operation_generation) {
                                            return;
                                        }
                                        operation.set(UiOperation::Idle);
                                        match result {
                                            Ok(joined) if joined.joined => {
                                                session.accept_registered_session(registered);
                                                match session.reload_bootstrap().await {
                                                    Ok(_) => {
                                                        let accepted = InvitationAcceptance {
                                                            company_name: joined.company_name,
                                                            position_name: joined.position_name,
                                                            venue_names: joined.venue_names,
                                                        };
                                                        let (cleared_invitation, cleared_otp) = cleared_sensitive_auth_fields();
                                                        invitation.set(cleared_invitation);
                                                        phone.set(String::new());
                                                        otp.set(cleared_otp);
                                                        password.set(String::new());
                                                        error.set(None);
                                                        acceptance.set(Some(accepted));
                                                    }
                                                    Err(_) => error.set(Some("Приглашение принято. Обновите страницу, чтобы продолжить.".into())),
                                                }
                                            }
                                            Ok(_) => error.set(Some("Приглашение недоступно. Проверьте код или запросите новый.".into())),
                                            Err(problem) => error.set(Some(safe_existing_invitation_error(&problem).into())),
                                        }
                                    });
                                },
                                RussianPhoneInput {
                                    id: "existing-invitation-phone".to_string(),
                                    label: "Номер телефона".to_string(),
                                    value: phone(),
                                    invalid: false,
                                    disabled: operation() != UiOperation::Idle,
                                    described_by: String::new(),
                                    on_change: move |digits| {
                                        phone.set(digits);
                                        error.set(None);
                                    }
                                }
                                div { class: "form-field",
                                    label { class: "field-label", r#for: "existing-invitation-password", "Пароль" }
                                    input {
                                        id: "existing-invitation-password",
                                        class: "field-input",
                                        r#type: "password",
                                        autocomplete: "current-password",
                                        maxlength: "72",
                                        value: "{password}",
                                        disabled: operation() != UiOperation::Idle,
                                        oninput: move |event| {
                                            password.set(event.value());
                                            error.set(None);
                                        },
                                    }
                                }
                                button {
                                    class: "btn-primary w-full",
                                    r#type: "submit",
                                    disabled: operation() != UiOperation::Idle || !russian_phone_is_complete(&phone()) || password().is_empty(),
                                    if operation() == UiOperation::JoiningExistingAccount {
                                        "Присоединение..."
                                    } else {
                                        "Войти и принять приглашение"
                                    }
                                }
                                button {
                                    class: "btn-ghost account-auth-link",
                                    r#type: "button",
                                    disabled: operation() != UiOperation::Idle,
                                    onclick: move |_| {
                                        phone.set(String::new());
                                        password.set(String::new());
                                        error.set(None);
                                        mode.set(AccountAuthMode::Choice);
                                    },
                                    "Назад"
                                }
                                button {
                                    class: "btn-ghost account-auth-link",
                                    r#type: "button",
                                    disabled: operation() != UiOperation::Idle,
                                    onclick: move |_| on_standalone.call(()),
                                    "Войти без приглашения"
                                }
                                button {
                                    class: "btn-ghost account-auth-link",
                                    r#type: "button",
                                    disabled: operation() != UiOperation::Idle,
                                    onclick: move |_| on_legacy_login.call(()),
                                    "Старый вход для существующей версии"
                                }
                            }
                        }
                    },
                } }
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct AccountHashListener {
    window: web_sys::Window,
    callback: Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for AccountHashListener {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback(
            "hashchange",
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

fn initial_account_navigation() -> NavigationId {
    #[cfg(target_arch = "wasm32")]
    if web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .is_some_and(|hash| hash == "#/invite")
    {
        return NavigationId::JoinOrganization;
    }
    if super::join_organization::group_invitation_token().is_some() {
        return NavigationId::JoinOrganization;
    }
    #[cfg(target_arch = "wasm32")]
    if let Some(hash) = web_sys::window()
        .and_then(|window| window.location().hash().ok())
        .filter(|hash| !hash.is_empty())
    {
        return resolve_hash(&hash).unwrap_or(NavigationId::Today);
    }
    NavigationId::Today
}

#[cfg(target_arch = "wasm32")]
fn install_account_hash_listener(mut active: Signal<NavigationId>) -> Option<AccountHashListener> {
    let window = web_sys::window()?;
    let listener_window = window.clone();
    let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
        let next = listener_window
            .location()
            .hash()
            .ok()
            .map_or(NavigationId::Today, |hash| {
                if hash == "#/invite" {
                    NavigationId::JoinOrganization
                } else {
                    resolve_hash(&hash).unwrap_or(NavigationId::Today)
                }
            });
        active.set(next);
    });
    window
        .add_event_listener_with_callback("hashchange", callback.as_ref().unchecked_ref())
        .ok()?;
    Some(AccountHashListener { window, callback })
}

fn navigate_account(mut active: Signal<NavigationId>, id: NavigationId) {
    let Some(target) = navigable_target(id) else {
        return;
    };
    active.set(target);
    #[cfg(target_arch = "wasm32")]
    if let Some(route) = route_for(target) {
        if let Some(window) = web_sys::window() {
            let _ = window.location().set_hash(route.trim_start_matches('#'));
        }
    }
}

fn current_assessment_result_id() -> Option<Uuid> {
    #[cfg(target_arch = "wasm32")]
    {
        return web_sys::window()
            .and_then(|window| window.location().hash().ok())
            .and_then(|hash| assessment_result_id(&hash));
    }
    #[allow(unreachable_code)]
    None
}

fn navigate_assessment_result(mut active: Signal<NavigationId>, attempt_id: Uuid) {
    active.set(NavigationId::AssessmentResult);
    #[cfg(target_arch = "wasm32")]
    if let Some(window) = web_sys::window() {
        let _ = window
            .location()
            .set_hash(&format!("/assessments/results/{attempt_id}"));
    }
}

fn role_from_profile(profile: OrganizationAccessProfile) -> NavigationRole {
    match profile {
        OrganizationAccessProfile::Owner => NavigationRole::Owner,
        OrganizationAccessProfile::OrganizationManager => NavigationRole::OrganizationManager,
        OrganizationAccessProfile::VenueManager => NavigationRole::VenueManager,
        OrganizationAccessProfile::EmployeeUnassigned
        | OrganizationAccessProfile::EmployeeVenue
        | OrganizationAccessProfile::Unsupported => NavigationRole::Employee,
    }
}

fn assessment_list_view(id: NavigationId) -> AssessmentListView {
    match id {
        NavigationId::AssessmentsHistory => AssessmentListView::History,
        NavigationId::AssessmentsPlan => AssessmentListView::Planned,
        _ => AssessmentListView::Active,
    }
}

fn assessment_list_key(id: NavigationId) -> &'static str {
    match id {
        NavigationId::AssessmentsHistory => "assessments-history",
        NavigationId::AssessmentsPlan => "assessments-plan",
        _ => "assessments-active",
    }
}

#[component]
fn AccountNavigationTree(
    role: NavigationRole,
    active: NavigationId,
    mut expanded: Signal<HashSet<NavigationId>>,
    logging_out: bool,
    on_navigate: EventHandler<NavigationId>,
    on_logout: EventHandler<()>,
) -> Element {
    let visible = visible_items(role);
    let active_root = active_parent(active);
    rsx! {
        nav { class: "brand-navigation-scroll", aria_label: "Разделы аккаунта",
            for root in visible.iter().copied().filter(|item| item.zone == NavigationZone::Main && item.parent_id.is_none()) {
                { let children = visible.iter().copied().filter(|item| item.parent_id == Some(root.id)).collect::<Vec<_>>();
                  let parent_active = active_root == root.id;
                  let is_expanded = parent_active || expanded().contains(&root.id);
                  let is_standalone_current = parent_active && children.is_empty();
                  let root_id = root.id;
                  let root_target = default_child(root_id).unwrap_or(root_id);
                  let navigate = on_navigate.clone();
                  rsx! {
                    div { class: if parent_active { "brand-nav-group is-active" } else { "brand-nav-group" },
                        div { class: "brand-nav-parent-row",
                            button {
                                class: if parent_active { "brand-nav-item brand-nav-parent is-active" } else { "brand-nav-item brand-nav-parent" },
                                r#type: "button",
                                aria_current: if is_standalone_current { "page" } else { "false" },
                                onclick: move |_| navigate.call(root_target),
                                NavigationIcon { icon: root.icon }
                                span { "{root.label}" }
                            }
                            if !children.is_empty() {
                                button {
                                    class: "brand-nav-expand",
                                    r#type: "button",
                                    aria_label: if is_expanded { format!("Свернуть раздел {}", root.label) } else { format!("Развернуть раздел {}", root.label) },
                                    aria_expanded: is_expanded,
                                    onclick: move |_| {
                                        let mut next = expanded();
                                        if !next.insert(root_id) { next.remove(&root_id); }
                                        expanded.set(next);
                                    },
                                    span { aria_hidden: "true", if is_expanded { "−" } else { "+" } }
                                }
                            }
                        }
                        if is_expanded && !children.is_empty() {
                            div { class: "brand-nav-children",
                                for child in children {
                                    { let child_id = child.id; let navigate = on_navigate.clone(); rsx! {
                                        button {
                                            class: if active == child_id { "brand-nav-item brand-nav-child is-active" } else { "brand-nav-item brand-nav-child" },
                                            r#type: "button",
                                            aria_current: if active == child_id { "page" } else { "false" },
                                            onclick: move |_| navigate.call(child_id),
                                            NavigationIcon { icon: child.icon, class: "navigation-icon navigation-icon--child".to_string() }
                                            span { "{child.label}" }
                                            if child.state == NavigationProductState::ComingSoon { small { class: "brand-nav-state", "Скоро" } }
                                        }
                                    } }
                                }
                            }
                        }
                    }
                  }
                }
            }
        }
        div { class: "brand-account-actions",
            p { "Аккаунт" }
            for item in visible.iter().copied().filter(|item| item.zone == NavigationZone::Service) {
                if item.id == NavigationId::Logout {
                    { let logout = on_logout.clone(); rsx! {
                        button { class: "brand-nav-item", r#type: "button", disabled: logging_out, onclick: move |_| logout.call(()),
                            NavigationIcon { icon: item.icon }
                            span { if logging_out { "Выход..." } else { "Выйти" } }
                        }
                    } }
                } else {
                    { let item_id = item.id; let navigate = on_navigate.clone(); rsx! {
                        button {
                            class: if active == item_id { "brand-nav-item is-active" } else { "brand-nav-item" },
                            r#type: "button", aria_current: if active == item_id { "page" } else { "false" },
                            onclick: move |_| navigate.call(item_id),
                            NavigationIcon { icon: item.icon }
                            span { "{item.label}" }
                        }
                    } }
                }
            }
        }
    }
}

#[component]
fn AccountMobileNavigation(
    role: NavigationRole,
    active: NavigationId,
    on_navigate: EventHandler<NavigationId>,
    on_more: EventHandler<()>,
) -> Element {
    let active_root = active_parent(active);
    let primary = visible_items(role)
        .into_iter()
        .filter(|item| {
            item.zone == NavigationZone::Main
                && item.parent_id.is_none()
                && item.mobile == MobilePlacement::Primary
        })
        .collect::<Vec<_>>();
    rsx! {
        nav { class: "brand-mobile-nav", aria_label: "Основная навигация",
            for item in primary {
                { let item_id = item.id; let target = default_child(item_id).unwrap_or(item_id); let navigate = on_navigate.clone(); rsx! {
                    button { class: if active_root == item_id { "active" } else { "" }, r#type: "button", onclick: move |_| navigate.call(target),
                        NavigationIcon { icon: item.icon }
                        small { "{item.label}" }
                    }
                } }
            }
            button { r#type: "button", aria_label: "Открыть все разделы", onclick: move |_| on_more.call(()),
                NavigationIcon { icon: crate::navigation::NavigationIconId::ListTree }
                small { "Ещё" }
            }
        }
    }
}

#[component]
fn AccountProfilePage(role: NavigationRole) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let company_name = match session.state() {
        AccountSessionState::Authenticated(value) => value
            .selected_company
            .and_then(|selected| {
                value
                    .bootstrap
                    .companies
                    .into_iter()
                    .find(|company| company.company_id == selected.0)
            })
            .map(|company| company.company_name),
        _ => None,
    };
    let role_label = match role {
        NavigationRole::Employee => "Сотрудник",
        NavigationRole::VenueManager => "Менеджер ресторана",
        NavigationRole::OrganizationManager => "Менеджер организации",
        NavigationRole::Owner => "Владелец",
    };
    rsx! { section { class: "journey-page", aria_labelledby: "account-profile-title",
        header { class: "journey-hero", p { class: "management-eyebrow", "АККАУНТ" } h1 { id: "account-profile-title", "Профиль" } p { "Текущий рабочий контекст без служебных идентификаторов." } }
        article { class: "journey-panel", h2 { "Доступ" } p { strong { "Роль: " } "{role_label}" } if let Some(name) = company_name { p { strong { "Организация: " } "{name}" } } }
    } }
}

#[component]
pub fn AccountPilotShell(on_logout: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let management_api = use_context::<AssessmentManagementApiClient>();
    let access_api = use_context::<OrganizationAccessApiClient>();
    let active = use_signal(initial_account_navigation);
    #[cfg(target_arch = "wasm32")]
    let _hash_listener = use_hook(move || install_account_hash_listener(active).map(Rc::new));
    let mut logging_out = use_signal(|| false);
    let mut lifecycle_epoch = use_signal(|| 0_u64);
    let mut launched_attempt =
        use_signal(|| None::<crate::assessment_attempt_api::AttemptDocument>);
    let mut capability_generation = use_signal(|| 0_u64);
    let mut navigation_open = use_signal(|| false);
    let mut expanded = use_signal(HashSet::<NavigationId>::new);
    use_context_provider(|| lifecycle_epoch);

    let probe_session = session.clone();
    let probe_api = management_api.clone();
    let probe_access_api = access_api.clone();
    let role_probe = use_resource(move || {
        let generation = capability_generation();
        let epoch = lifecycle_epoch();
        let state = probe_session.state();
        let api = probe_api.clone();
        let access_api = probe_access_api.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return (generation, epoch, None, Ok(NavigationRole::Employee));
            };
            let Some(company_id) = account.selected_company.map(|value| value.0) else {
                return (generation, epoch, None, Ok(NavigationRole::Employee));
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
                    Ok(NavigationRole::Owner),
                );
            }
            if relationship != Some("employee") {
                return (
                    generation,
                    epoch,
                    Some(company_id),
                    Ok(NavigationRole::Employee),
                );
            }
            let employee_profile_id = account
                .bootstrap
                .companies
                .iter()
                .find(|company| company.company_id == company_id)
                .and_then(|company| company.employee_profile_id);
            let access_result = access_api
                .employees(&account.access_token, company_id)
                .await;
            if let Ok(employees) = access_result {
                let role = employee_profile_id
                    .and_then(|profile_id| {
                        employees
                            .into_iter()
                            .find(|employee| employee.employee_profile_id == profile_id)
                    })
                    .map(|employee| role_from_profile(employee.profile))
                    .unwrap_or(NavigationRole::Employee);
                return (generation, epoch, Some(company_id), Ok(role));
            }
            let result: Result<Vec<ManagementEmployee>, AssessmentManagementApiError> = api
                .employees(&account.access_token, company_id, None, 1, None)
                .await;
            (
                generation,
                epoch,
                Some(company_id),
                match capability_from_probe(&result) {
                    ManagerCapability::Authorized => Ok(NavigationRole::VenueManager),
                    ManagerCapability::Hidden => Ok(NavigationRole::Employee),
                    ManagerCapability::Checking | ManagerCapability::Error => Err(()),
                },
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
    let role_result = if owner_capability == ManagerCapability::Authorized {
        Some(Ok(NavigationRole::Owner))
    } else {
        role_probe().and_then(|(generation, epoch, company_id, role)| {
            (generation == capability_generation()
                && epoch == lifecycle_epoch()
                && company_id == selected_company)
                .then_some(role)
        })
    };
    let navigation_role = role_result
        .and_then(Result::ok)
        .unwrap_or(NavigationRole::Employee);
    let manager_capability = match role_result {
        Some(Ok(role)) if role.is_manager() => ManagerCapability::Authorized,
        Some(Ok(_)) => ManagerCapability::Hidden,
        Some(Err(_)) => ManagerCapability::Error,
        None => ManagerCapability::Checking,
    };
    let company_session = session.clone();
    if role_result.is_some()
        && active() != NavigationId::AssessmentResult
        && authorized_target(route_for(active()).unwrap_or("#/today"), navigation_role) != active()
    {
        navigate_account(active, NavigationId::Today);
    }

    let navigate = EventHandler::new(move |id: NavigationId| {
        launched_attempt.set(None);
        expanded.with_mut(|items| {
            items.insert(active_parent(id));
        });
        navigate_account(active, id);
        navigation_open.set(false);
    });
    let assessment_route_key = assessment_list_key(active());

    rsx! {
        div {
            class: if navigation_open() { "account-pilot brand-account-shell brand-account-shell--nav-open app-root" } else { "account-pilot brand-account-shell app-root" },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    navigation_open.set(false);
                }
            },
            div { class: "brand-account-ambient", aria_hidden: "true" }
            button {
                class: "brand-home-logo",
                r#type: "button",
                aria_label: "На главную RestOS",
                onclick: move |_| {
                    launched_attempt.set(None);
                    navigate_account(active, NavigationId::Today);
                    navigation_open.set(false);
                },
                img { src: "/brand/r-icon-master.png", alt: "" }
                strong { "RestOS" }
            }
            div {
                class: "brand-nav-edge-trigger",
                tabindex: "0",
                aria_label: "Открыть боковое меню",
                onmouseenter: move |_| navigation_open.set(true),
                onfocus: move |_| navigation_open.set(true),
                span { class: "sr-only", "Открыть боковое меню" }
            }
            button {
                class: "brand-nav-toggle",
                r#type: "button",
                aria_label: "Открыть меню",
                aria_expanded: navigation_open(),
                onclick: move |_| navigation_open.set(true),
                "☰"
            }
            if navigation_open() {
                button { class: "brand-nav-backdrop", r#type: "button", aria_label: "Закрыть меню", onclick: move |_| navigation_open.set(false) }
            }
            aside {
                class: if navigation_open() { "account-pilot-header brand-account-nav is-open" } else { "account-pilot-header brand-account-nav" },
                onmouseenter: move |_| navigation_open.set(true),
                onmouseleave: move |_| navigation_open.set(false),
                div { class: "brand-account-logo",
                    button {
                        class: "brand-account-logo__action",
                        r#type: "button",
                        aria_label: "На главную RestOS",
                        onclick: move |_| {
                            launched_attempt.set(None);
                            navigate_account(active, NavigationId::Today);
                            navigation_open.set(false);
                        },
                        img { src: "/brand/r-icon-master.png", alt: "" }
                        div { strong { "RestOS" } small { "Операционная система ресторана" } }
                    }
                }
                if companies.len() > 1 {
                    label { class: "brand-company-switcher",
                        span { "Компания" }
                        select {
                            aria_label: "Выбрать компанию",
                            value: selected_company.map(|value| value.to_string()).unwrap_or_default(),
                            onchange: move |event| {
                                let Ok(company_id) = Uuid::parse_str(&event.value()) else { return; };
                                if company_session.select_company(crate::account_api::SelectedCompanyId(company_id)).is_ok() {
                                    lifecycle_epoch += 1;
                                    capability_generation += 1;
                                    launched_attempt.set(None);
                                    navigate_account(active, NavigationId::Today);
                                    navigation_open.set(false);
                                }
                            },
                            option { value: "", disabled: true, "Выберите компанию" }
                            for company in companies.iter() {
                                option { key: "company-{company.company_id}", value: "{company.company_id}", "{company.company_name}" }
                            }
                        }
                    }
                } else if let Some(company) = companies.first() {
                    p { class: "brand-company-label", "{company.company_name}" }
                }
                if manager_capability == ManagerCapability::Error {
                    button {
                        class: "btn-secondary", r#type: "button",
                        onclick: move |_| capability_generation += 1,
                        "Повторить проверку доступа"
                    }
                }
                AccountNavigationTree {
                    role: navigation_role,
                    active: active(),
                    expanded,
                    logging_out: logging_out(),
                    on_navigate: navigate,
                    on_logout: move |_| {
                        if logging_out() { return; }
                        logging_out.set(true);
                        lifecycle_epoch += 1;
                        let session = session.clone();
                        spawn(async move { let _ = session.logout().await; on_logout.call(()); });
                    }
                }
            }
            AccountMobileNavigation {
                role: navigation_role,
                active: active(),
                on_navigate: navigate,
                on_more: move |_| navigation_open.set(true),
            }
            main { class: "account-pilot-main brand-account-main",
                match active() {
                    NavigationId::Today => rsx! {
                        crate::components::AccountToday {
                            manager_enabled: manager_capability == ManagerCapability::Authorized,
                            on_assessments: move |_| navigate_account(active, NavigationId::AssessmentsActive),
                            on_measure: move |_| navigate_account(active, NavigationId::Measure),
                            on_templates: move |_| navigate_account(active, NavigationId::TemplateLibrary),
                            on_team: move |_| navigate_account(active, NavigationId::TeamTasks),
                            on_dashboard: move |_| navigate_account(active, NavigationId::AnalyticsBuilder),
                        }
                    },
                    NavigationId::AssessmentsActive | NavigationId::AssessmentsHistory | NavigationId::AssessmentsPlan => rsx! {
                        AssessmentAttemptsPage {
                            key: "{assessment_route_key}",
                            view: assessment_list_view(active()),
                            on_measure: move |_| { launched_attempt.set(None); navigate_account(active, NavigationId::Measure); },
                            on_team: move |_| { launched_attempt.set(None); navigate_account(active, NavigationId::TeamTasks); },
                            on_result: move |attempt_id| { launched_attempt.set(None); navigate_assessment_result(active, attempt_id); },
                            initial_attempt: launched_attempt()
                        }
                    },
                    NavigationId::AssessmentResult => rsx! {
                        if let Some(attempt_id) = current_assessment_result_id() {
                            AssessmentResultPage { attempt_id, on_back: move |_| navigate_account(active, NavigationId::AssessmentsHistory) }
                        } else {
                            div { class: "account-live account-live--error", role: "alert", "Результат недоступен." }
                        }
                    },
                    NavigationId::Measure => rsx! {
                        crate::components::MeasurementLauncher {
                            on_cancel: move |_| navigate_account(active, NavigationId::AssessmentsActive),
                            on_attempt: move |value| { launched_attempt.set(Some(value)); navigate_account(active, NavigationId::AssessmentsActive); }
                        }
                    },
                    NavigationId::TemplateLibrary | NavigationId::CompanyTemplates => rsx! { AssessmentsPage {
                        section: if active() == NavigationId::CompanyTemplates { AssessmentLibrarySection::Company } else { AssessmentLibrarySection::Library },
                        on_section_change: move |section| navigate_account(active, match section { AssessmentLibrarySection::Library => NavigationId::TemplateLibrary, AssessmentLibrarySection::Company => NavigationId::CompanyTemplates })
                    } },
                    NavigationId::JoinOrganization => rsx! { JoinOrganizationPage {} },
                    NavigationId::OrganizationEmployees | NavigationId::OrganizationVenues | NavigationId::OrganizationAccess | NavigationId::OrganizationInvitations => rsx! { OrganizationSettingsPage {
                        section: match active() { NavigationId::OrganizationVenues => OrganizationSettingsSection::Venues, NavigationId::OrganizationAccess => OrganizationSettingsSection::Positions, NavigationId::OrganizationInvitations => OrganizationSettingsSection::Invitations, _ => OrganizationSettingsSection::Employees },
                        on_section_change: move |section| navigate_account(active, match section { OrganizationSettingsSection::Employees => NavigationId::OrganizationEmployees, OrganizationSettingsSection::Venues => NavigationId::OrganizationVenues, OrganizationSettingsSection::Positions => NavigationId::OrganizationAccess, OrganizationSettingsSection::Invitations => NavigationId::OrganizationInvitations })
                    } },
                    NavigationId::AnalyticsBuilder | NavigationId::AnalyticsResults | NavigationId::AnalyticsRestaurant | NavigationId::AnalyticsSources => rsx! {
                        RestaurantMetricsDashboardPage {
                            view: match active() { NavigationId::AnalyticsResults => MetricsNavigationView::Results, NavigationId::AnalyticsRestaurant => MetricsNavigationView::Restaurant, NavigationId::AnalyticsSources => MetricsNavigationView::Sources, _ => MetricsNavigationView::Builder },
                            on_walkthrough_started: move |_| navigate_account(active, NavigationId::AssessmentsActive)
                        }
                    },
                    NavigationId::TeamShifts | NavigationId::TeamTasks | NavigationId::TeamCalendar => rsx! {
                        TeamManagementPage {
                            can_manage: manager_capability == ManagerCapability::Authorized,
                            area: match active() { NavigationId::TeamShifts => TeamArea::Shifts, NavigationId::TeamCalendar => TeamArea::Calendar, _ => TeamArea::Tasks },
                            on_area_change: move |area| navigate_account(active, match area { TeamArea::Shifts => NavigationId::TeamShifts, TeamArea::Tasks => NavigationId::TeamTasks, TeamArea::Calendar => NavigationId::TeamCalendar })
                        }
                    },
                    NavigationId::Profile => rsx! { AccountProfilePage { role: navigation_role } },
                    NavigationId::Security => rsx! { PasskeySecurityPanel {} },
                    NavigationId::Assessments | NavigationId::Templates | NavigationId::Organization | NavigationId::Analytics | NavigationId::Team | NavigationId::Logout => rsx! { div { role: "status", "Открываем раздел…" } },
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
            let result = match token {
                Some(token) => api.list(&token).await,
                None => Err(AccountApiError::AuthenticationRequired),
            };
            if lifecycle_epoch() != list_epoch {
                return Err(AccountApiError::AuthenticationRequired);
            }
            result
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

    #[wasm_bindgen_test]
    fn account_sidebar_uses_hover_edge_without_a_logo_overlapping_close_icon() {
        let source = include_str!("account_portal.rs");
        let shell = source
            .split_once("pub fn AccountPilotShell")
            .and_then(|(_, source)| source.split_once("fn PasskeySecurityPanel"))
            .map(|(source, _)| source)
            .unwrap_or_default();

        assert!(shell.contains("brand-nav-edge-trigger"));
        assert!(shell.contains("onmouseenter: move |_| navigation_open.set(true)"));
        assert!(shell.contains("onmouseleave: move |_| navigation_open.set(false)"));
        let escape_handler = shell
            .find("event.key() == Key::Escape")
            .expect("the account shell must own the Escape handler");
        let sidebar = shell
            .find("aside {")
            .expect("the account shell must render the sidebar");
        assert!(escape_handler < sidebar);
        assert!(!shell.contains("if navigation_open() { \"×\" }"));
        assert!(!shell.contains("aria_label: \"Скрыть меню\""));
    }

    #[wasm_bindgen_test]
    fn assessment_list_routes_have_distinct_component_lifecycle_keys() {
        let active = assessment_list_key(NavigationId::AssessmentsActive);
        let history = assessment_list_key(NavigationId::AssessmentsHistory);
        let planned = assessment_list_key(NavigationId::AssessmentsPlan);

        assert_ne!(active, history);
        assert_ne!(history, planned);
        assert_ne!(active, planned);
        assert_eq!(
            assessment_list_view(NavigationId::AssessmentsHistory),
            AssessmentListView::History
        );
    }

    #[wasm_bindgen_test]
    fn authenticated_shell_always_exposes_a_home_logo_without_network_side_effects() {
        let source = include_str!("account_portal.rs");
        let shell = source
            .split_once("pub fn AccountPilotShell")
            .and_then(|(_, source)| source.split_once("fn PasskeySecurityPanel"))
            .map(|(source, _)| source)
            .unwrap_or_default();

        assert!(shell.contains("class: \"brand-home-logo\""));
        assert!(shell.matches("aria_label: \"На главную RestOS\"").count() >= 2);
        assert!(
            shell
                .matches("navigate_account(active, NavigationId::Today)")
                .count()
                >= 3
        );
        assert!(shell.matches("launched_attempt.set(None)").count() >= 3);
        assert!(shell.contains("class: \"brand-account-logo__action\""));
        assert!(!shell.contains("AccountApiClient::"));
    }

    #[wasm_bindgen_test]
    fn user_journey_has_six_work_sections_and_separate_account_actions() {
        let source = include_str!("account_portal.rs");
        let shell = source
            .split_once("pub fn AccountPilotShell")
            .and_then(|(_, source)| source.split_once("fn PasskeySecurityPanel"))
            .map(|(source, _)| source)
            .unwrap_or_default();

        assert!(shell.contains("AccountNavigationTree"));
        assert!(shell.contains("AccountMobileNavigation"));
        assert!(shell.contains("role: navigation_role"));
        assert!(shell.contains("active: active()"));
        assert!(!shell.contains("AccountPilotTab"));
    }

    #[wasm_bindgen_test]
    fn employee_task_navigation_does_not_require_manager_capability() {
        let employee = visible_items(NavigationRole::Employee);
        assert!(employee
            .iter()
            .any(|item| item.id == NavigationId::TeamTasks));
        assert!(!employee
            .iter()
            .any(|item| item.id == NavigationId::Analytics));
        assert!(!employee
            .iter()
            .any(|item| item.id == NavigationId::Organization));
    }

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
    fn invitation_entry_is_code_only_and_success_routes_to_today() {
        let source = include_str!("account_portal.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("Вас пригласили в RestOS"));
        assert!(production.contains("Введите код из SMS"));
        assert!(production.contains("autocomplete: \"one-time-code\""));
        assert!(production.contains("use_signal(|| AccountAuthMode::Choice)"));
        assert_eq!(production.matches("id: \"account-invitation\"").count(), 1);
        assert!(production.contains("invitation().len() != 6"));
        assert!(production.contains("AccountAuthMode::ExistingAccount"));
        assert!(production.contains("api.password_login(&identities, &login)"));
        assert!(production.contains("accept_invitation(&registered.access_token, &accept)"));
        assert!(production.contains("session.reload_bootstrap().await"));
        assert_eq!(
            safe_existing_login_error(&AccountApiError::AuthenticationRequired),
            safe_existing_login_error(&AccountApiError::InvalidRequest)
        );
        assert!(production.contains("window.location().set_hash(\"/today\")"));
        assert!(!production.contains("invite?code="));
        assert!(!production.contains("#/invite/"));
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
