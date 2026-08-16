//! Standalone Account login, registration, and recovery. Secrets remain in component memory only.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;
use wasm_bindgen::JsCast;

use crate::{
    account_api::{
        AccountApiClient, AccountApiError, CreateFirstCompanyInput, PasswordLoginInput,
        PasswordResetCompleteInput, PasswordResetSmsRequestInput, SmsRequested,
        StandaloneRegistrationInput, StandaloneSmsRequestInput, StandaloneSmsVerifyInput,
    },
    account_session::AccountSessionAdapter,
    device_identity::DeviceIdentityAdapter,
    passkey::{PasskeyAdapter, PasskeyError},
};

use super::{
    account_legal_notice::{
        password_recovery_sms_request_allowed, registration_sms_request_allowed,
        AccountCreationLegalNotice, AccountLegalContext, AccountLegalNotice,
        PasswordRecoverySmsLegalControl, RegistrationSmsLegalControls,
    },
    account_portal::{safe_account_error, safe_passkey_error, InvitationAccountAuthPage},
    russian_phone_input::{canonical_russian_phone, russian_phone_is_complete, RussianPhoneInput},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Login,
    RegistrationPhone,
    RegistrationOtp,
    RegistrationDetails,
    AccountCreated,
    CreateCompany,
    ResetPhone,
    ResetOtp,
    ResetPassword,
    Invitation,
}

fn password_is_valid(value: &str) -> bool {
    (12..=72).contains(&value.as_bytes().len())
}

fn company_onboarding_input(
    company_name: &str,
    venue_name: &str,
) -> Option<CreateFirstCompanyInput> {
    let company_name = company_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if company_name.is_empty() || company_name.chars().count() > 255 {
        return None;
    }
    let venue_name = venue_name.split_whitespace().collect::<Vec<_>>().join(" ");
    if venue_name.chars().count() > 255 {
        return None;
    }
    Some(CreateFirstCompanyInput {
        company_name,
        venue_name: (!venue_name.is_empty()).then_some(venue_name),
        timezone: "Europe/Moscow".into(),
        locale: "ru-RU".into(),
    })
}

fn generic_login_error(error: &AccountApiError) -> &'static str {
    match error {
        AccountApiError::AuthenticationRequired
        | AccountApiError::PermissionDenied
        | AccountApiError::InvalidRequest => "Неверный номер телефона или пароль.",
        _ => safe_account_error(error),
    }
}

fn focus_auth_field(id: &'static str) {
    spawn(async move {
        TimeoutFuture::new(0).await;
        let Some(element) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(id))
        else {
            return;
        };
        if let Ok(element) = element.dyn_into::<web_sys::HtmlElement>() {
            let _ = element.focus();
        }
    });
}

fn arm_resend_timer(
    available_at: String,
    mut ready: Signal<bool>,
    generation: u64,
    current: Signal<u64>,
) {
    spawn(async move {
        let boundary = js_sys::Date::parse(&available_at);
        if !boundary.is_finite() {
            return;
        }
        loop {
            if current() != generation {
                return;
            }
            let remaining = boundary - js_sys::Date::now();
            if remaining <= 0.0 {
                ready.set(true);
                return;
            }
            TimeoutFuture::new(remaining.ceil().clamp(1.0, u32::MAX as f64) as u32).await;
        }
    });
}

#[component]
pub fn AccountAuthPage(
    on_authenticated: EventHandler<()>,
    on_legacy_login: EventHandler<()>,
) -> Element {
    let api = use_context::<AccountApiClient>();
    let session = use_context::<AccountSessionAdapter>();
    let identities = use_context::<DeviceIdentityAdapter>();
    let passkeys = use_context::<PasskeyAdapter>();

    let mut mode = use_signal(|| Mode::Login);
    let mut busy = use_signal(|| false);
    let mut generation = use_signal(|| 0_u64);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut status: Signal<Option<String>> = use_signal(|| None);
    let mut phone = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut password_visible = use_signal(|| false);
    let mut login_phone_invalid = use_signal(|| false);
    let mut login_password_invalid = use_signal(|| false);
    let mut display_name = use_signal(String::new);
    let mut company_name = use_signal(String::new);
    let mut venue_name = use_signal(String::new);
    let mut otp = use_signal(String::new);
    let mut challenge: Signal<Option<Uuid>> = use_signal(|| None);
    let mut resend_ready = use_signal(|| false);
    let mut resend_generation = use_signal(|| 0_u64);
    let mut personal_data_consent = use_signal(|| false);
    let mut authorization_sms_consent = use_signal(|| false);

    let mut reset_flow = move |next: Mode| {
        generation += 1;
        resend_generation += 1;
        busy.set(false);
        error.set(None);
        status.set(None);
        phone.set(String::new());
        password.set(String::new());
        password_visible.set(false);
        login_phone_invalid.set(false);
        login_password_invalid.set(false);
        display_name.set(String::new());
        company_name.set(String::new());
        venue_name.set(String::new());
        otp.set(String::new());
        challenge.set(None);
        resend_ready.set(false);
        personal_data_consent.set(false);
        authorization_sms_consent.set(false);
        mode.set(next);
    };

    if mode() == Mode::Invitation {
        return rsx! {
            InvitationAccountAuthPage {
                on_authenticated,
                on_legacy_login,
                on_standalone: move |_| reset_flow(Mode::Login),
            }
        };
    }

    let heading = match mode() {
        Mode::Login => "Вход в аккаунт",
        Mode::RegistrationPhone | Mode::RegistrationOtp | Mode::RegistrationDetails => {
            "Регистрация аккаунта"
        }
        Mode::AccountCreated => "Аккаунт создан",
        Mode::CreateCompany => "Создать организацию",
        Mode::ResetPhone | Mode::ResetOtp | Mode::ResetPassword => "Восстановление пароля",
        Mode::Invitation => "Регистрация по приглашению",
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
                    h1 { class: "account-auth-heading", "{heading}" }
                    p { class: "auth-subtitle", "Безопасный вход в рабочий аккаунт RestOS." }
                }
                div { id: "account-auth-feedback", tabindex: "-1", class: if error().is_some() { "account-live account-live--error semantic-error-surface" } else if status().is_some() { "account-live account-live--success" } else { "account-live" }, role: if error().is_some() { "alert" } else { "status" }, aria_live: if error().is_some() { "assertive" } else { "polite" },
                    if error().is_some() { span { class: "account-live__icon", aria_hidden: "true", "!" } }
                    if let Some(message) = error() { "{message}" }
                    if let Some(message) = status() { "{message}" }
                }

                match mode() {
                    Mode::Login => {
                        let login_api = api.clone();
                        let login_identities = identities.clone();
                        let login_session = session.clone();
                        let login_passkeys = passkeys.clone();
                        let passkey_session = session.clone();
                        rsx! { form { class: "auth-form account-login-form", aria_busy: busy(),
                            onsubmit: move |event| {
                                event.prevent_default();
                                if busy() { return; }
                                let phone_invalid = !russian_phone_is_complete(&phone());
                                let password_invalid = password().is_empty();
                                login_phone_invalid.set(phone_invalid);
                                login_password_invalid.set(password_invalid);
                                if phone_invalid || password_invalid {
                                    error.set(Some("Проверьте номер телефона и пароль.".into()));
                                    focus_auth_field(if phone_invalid { "standalone-login-phone" } else { "standalone-login-password" });
                                    return;
                                }
                                busy.set(true); error.set(None); generation += 1;
                                let current_generation = generation();
                                let api = login_api.clone(); let identities = login_identities.clone(); let session = login_session.clone();
                                let Some(canonical_phone) = canonical_russian_phone(&phone()) else { return; };
                                let request = PasswordLoginInput { phone: canonical_phone, password: password(), platform: "web".into(), device_display_name: Some("Браузер RestOS".into()) };
                                spawn(async move {
                                    let result = api.password_login(&identities, &request).await;
                                    if generation() != current_generation { return; }
                                    busy.set(false);
                                    match result {
                                        Ok(registered) => { password.set(String::new()); session.accept_registered_session(registered); on_authenticated.call(()); },
                                        Err(problem) => {
                                            login_phone_invalid.set(true);
                                            login_password_invalid.set(true);
                                            error.set(Some(generic_login_error(&problem).into()));
                                            focus_auth_field("account-auth-feedback");
                                        }
                                    }
                                });
                            },
                            RussianPhoneInput {
                                id: "standalone-login-phone".to_string(),
                                label: "Номер телефона".to_string(),
                                value: phone(),
                                invalid: login_phone_invalid(),
                                disabled: busy(),
                                described_by: if login_phone_invalid() { "account-auth-feedback".to_string() } else { String::new() },
                                on_change: move |digits| { phone.set(digits); login_phone_invalid.set(false); error.set(None); }
                            }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "standalone-login-password", "Пароль" }
                                div { class: "account-password-field",
                                    input { id: "standalone-login-password", class: "field-input", r#type: if password_visible() { "text" } else { "password" }, autocomplete: "current-password", maxlength: "72", value: "{password}", aria_invalid: login_password_invalid(), aria_describedby: if login_password_invalid() { "account-auth-feedback" } else { "" }, oninput: move |event| { password.set(event.value()); login_password_invalid.set(false); } }
                                    button { class: "account-password-toggle", r#type: "button", aria_label: if password_visible() { "Скрыть пароль" } else { "Показать пароль" }, aria_pressed: password_visible(), onclick: move |_| password_visible.set(!password_visible()),
                                        if password_visible() { "Скрыть" } else { "Показать" }
                                    }
                                }
                            }
                            button { class: "btn-primary w-full", r#type: "submit", disabled: busy() || !russian_phone_is_complete(&phone()),
                                if busy() { "Вход..." } else { "Войти" }
                            }
                            button { class: "btn-secondary w-full account-passkey-action", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    if busy() { return; }
                                    error.set(None);
                                    if !PasskeyAdapter::is_supported() { error.set(safe_passkey_error(&PasskeyError::Unavailable).map(str::to_string)); return; }
                                    busy.set(true); generation += 1; let current_generation = generation();
                                    let passkeys = login_passkeys.clone(); let session = passkey_session.clone();
                                    spawn(async move {
                                        let result = passkeys.authenticate(&session, "web", Some("Этот браузер".into())).await;
                                        if generation() != current_generation { return; }
                                        busy.set(false);
                                        match result { Ok(()) => on_authenticated.call(()), Err(problem) => error.set(safe_passkey_error(&problem).map(str::to_string)) }
                                    });
                                },
                                "Войти с Face ID или ключом доступа"
                            }
                            button { class: "account-text-action", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::ResetPhone), "Забыли пароль?" }
                            div { class: "account-auth-separator", aria_hidden: "true" }
                            button { class: "btn-secondary w-full account-create-action", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::RegistrationPhone), "Создать аккаунт" }
                            button { class: "account-text-action", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::Invitation), "Регистрация по приглашению" }
                            details { class: "account-auth-disclosure",
                                summary { "Другие способы входа" }
                                button { class: "account-text-action", r#type: "button", onclick: move |_| on_legacy_login.call(()), "Старый вход для существующей версии" }
                            }
                            AccountLegalNotice { context: AccountLegalContext::Login }
                        } }
                    },
                    Mode::RegistrationPhone | Mode::ResetPhone => {
                        let request_api = api.clone();
                        let is_registration = mode() == Mode::RegistrationPhone;
                        rsx! { div { class: "auth-form",
                            RussianPhoneInput {
                                id: "standalone-phone".to_string(),
                                label: "Номер телефона".to_string(),
                                value: phone(),
                                invalid: false,
                                disabled: busy(),
                                described_by: String::new(),
                                on_change: move |digits| { phone.set(digits); error.set(None); }
                            }
                            if is_registration {
                                RegistrationSmsLegalControls { personal_data_consent, authorization_sms_consent }
                            } else {
                                PasswordRecoverySmsLegalControl { authorization_sms_consent }
                            }
                            button { class: "btn-primary w-full", r#type: "button",
                                disabled: busy() || !russian_phone_is_complete(&phone()) || if is_registration { !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) } else { !password_recovery_sms_request_allowed(authorization_sms_consent()) },
                                onclick: move |_| {
                                    if busy() { return; }
                                    let Some(request_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    if if is_registration { !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) } else { !password_recovery_sms_request_allowed(authorization_sms_consent()) } { return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = request_api.clone();
                                    personal_data_consent.set(false);
                                    authorization_sms_consent.set(false);
                                    spawn(async move {
                                        let result = if is_registration {
                                            api.request_registration_sms(&StandaloneSmsRequestInput { phone: request_phone, personal_data_consent: true, authorization_sms_consent: true }).await
                                        } else {
                                            api.request_password_reset_sms(&PasswordResetSmsRequestInput { phone: request_phone, authorization_sms_consent: true }).await
                                        };
                                        if generation() != current_generation { return; }
                                        busy.set(false);
                                        match result { Ok(requested) => { challenge.set(Some(requested.challenge_id)); otp.set(String::new()); resend_ready.set(false); resend_generation += 1; arm_resend_timer(requested.resend_available_at, resend_ready, resend_generation(), resend_generation); mode.set(if is_registration { Mode::RegistrationOtp } else { Mode::ResetOtp }); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Отправка..." } else { "Получить код" }
                            }
                            if if is_registration { !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) } else { !password_recovery_sms_request_allowed(authorization_sms_consent()) } {
                                p { class: "account-auth-disabled-reason", role: "status",
                                    if is_registration { "Подтвердите оба согласия, чтобы получить код." }
                                    else { "Подтвердите согласие на сервисное SMS, чтобы получить код." }
                                }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| reset_flow(Mode::Login), "Назад ко входу" }
                        } }
                    },
                    Mode::RegistrationOtp | Mode::ResetOtp => {
                        let verify_api = api.clone();
                        let resend_api = api.clone();
                        let is_registration = mode() == Mode::RegistrationOtp;
                        rsx! { div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "standalone-otp", "Код из SMS" }
                                input { id: "standalone-otp", class: "field-input", inputmode: "numeric", autocomplete: "one-time-code", maxlength: "6", value: "{otp}", oninput: move |event| otp.set(event.value().chars().filter(char::is_ascii_digit).take(6).collect()) }
                            }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy() || otp().len() != 6,
                                onclick: move |_| {
                                    let Some(challenge_id) = challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if busy() { return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let api = verify_api.clone(); let request = StandaloneSmsVerifyInput { challenge_id, phone: canonical_phone, code: otp() };
                                    spawn(async move {
                                        let result = if is_registration { api.verify_registration_sms(&request).await } else { api.verify_password_reset_sms(&request).await };
                                        if generation() != current_generation { return; }
                                        busy.set(false); otp.set(String::new());
                                        match result { Ok(()) => { resend_generation += 1; resend_ready.set(false); mode.set(if is_registration { Mode::RegistrationDetails } else { Mode::ResetPassword }); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Проверка..." } else { "Подтвердить код" }
                            }
                            if is_registration {
                                RegistrationSmsLegalControls { personal_data_consent, authorization_sms_consent }
                            } else {
                                PasswordRecoverySmsLegalControl { authorization_sms_consent }
                            }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy() || !resend_ready() || if is_registration { !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) } else { !password_recovery_sms_request_allowed(authorization_sms_consent()) },
                                onclick: move |_| {
                                    if busy() || !resend_ready() || if is_registration { !registration_sms_request_allowed(personal_data_consent(), authorization_sms_consent()) } else { !password_recovery_sms_request_allowed(authorization_sms_consent()) } { return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let Some(request_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let api = resend_api.clone();
                                    personal_data_consent.set(false); authorization_sms_consent.set(false);
                                    spawn(async move {
                                        let result = if is_registration {
                                            api.request_registration_sms(&StandaloneSmsRequestInput { phone: request_phone, personal_data_consent: true, authorization_sms_consent: true }).await
                                        } else {
                                            api.request_password_reset_sms(&PasswordResetSmsRequestInput { phone: request_phone, authorization_sms_consent: true }).await
                                        };
                                        if generation() != current_generation { return; }
                                        busy.set(false);
                                        match result { Ok(SmsRequested { challenge_id, resend_available_at, .. }) => { challenge.set(Some(challenge_id)); resend_ready.set(false); resend_generation += 1; arm_resend_timer(resend_available_at, resend_ready, resend_generation(), resend_generation); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                "Отправить код снова"
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| reset_flow(Mode::Login), "Отмена" }
                        } }
                    },
                    Mode::RegistrationDetails => {
                        let register_api = api.clone(); let register_identities = identities.clone(); let register_session = session.clone();
                        rsx! { div { class: "auth-form",
                            div { class: "form-field", label { class: "field-label", r#for: "standalone-name", "ФИО" } input { id: "standalone-name", class: "field-input", autocomplete: "name", maxlength: "255", value: "{display_name}", oninput: move |event| display_name.set(event.value()) } }
                            div { class: "form-field", label { class: "field-label", r#for: "standalone-new-password", "Пароль" } input { id: "standalone-new-password", class: "field-input", r#type: "password", autocomplete: "new-password", minlength: "12", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            p { class: "account-auth-help", "От 12 до 72 байт. Не используйте пароль от других сервисов." }
                            AccountCreationLegalNotice {}
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    let Some(challenge_id) = challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if busy() || display_name().trim().is_empty() || !password_is_valid(&password()) { error.set(Some("Проверьте имя и длину пароля.".into())); return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = register_api.clone(); let identities = register_identities.clone(); let session = register_session.clone();
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let request = StandaloneRegistrationInput { phone_verification_challenge_id: challenge_id, phone: canonical_phone, display_name: display_name().trim().into(), password: password(), platform: "web".into(), device_display_name: Some("Браузер RestOS".into()) };
                                    spawn(async move {
                                        let result = api.register_standalone(&identities, &request).await;
                                        if generation() != current_generation { return; }
                                        busy.set(false); password.set(String::new()); otp.set(String::new());
                                        match result { Ok(registered) => { session.accept_registered_session(registered); status.set(Some("Аккаунт готов. Добавьте ключ доступа или продолжите без него.".into())); mode.set(Mode::AccountCreated); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Создание аккаунта..." } else { "Создать аккаунт" }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| reset_flow(Mode::Login), "Отмена" }
                        } }
                    },
                    Mode::AccountCreated => {
                        let offer_passkeys = passkeys.clone(); let offer_session = session.clone();
                        rsx! { div { class: "auth-form",
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| { error.set(None); mode.set(Mode::CreateCompany); },
                                "Создать организацию"
                            }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    if super::join_organization::group_invitation_token().is_some() {
                                        on_authenticated.call(());
                                    } else {
                                        reset_flow(Mode::Invitation);
                                    }
                                },
                                "У меня есть приглашение"
                            }
                            p { class: "account-auth-help", "Ключ доступа позволит входить с Face ID, Touch ID или кодом устройства." }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    if busy() { return; }
                                    if !PasskeyAdapter::is_supported() { error.set(safe_passkey_error(&PasskeyError::Unavailable).map(str::to_string)); return; }
                                    let Ok(token) = offer_session.current_token() else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                                    busy.set(true); generation += 1; let current_generation = generation(); let passkeys = offer_passkeys.clone();
                                    spawn(async move {
                                        let result = passkeys.register(&token, Some("Этот браузер".into())).await;
                                        if generation() != current_generation { return; }
                                        busy.set(false);
                                        match result { Ok(()) => on_authenticated.call(()), Err(problem) => error.set(safe_passkey_error(&problem).map(str::to_string)) }
                                    });
                                },
                                "Настроить Face ID / ключ доступа"
                            }
                        } }
                    },
                    Mode::CreateCompany => {
                        let create_api = api.clone();
                        let create_session = session.clone();
                        rsx! { div { class: "auth-form company-onboarding-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "first-company-name", "Название организации" }
                                input { id: "first-company-name", class: "field-input", autocomplete: "organization", maxlength: "255", value: "{company_name}", oninput: move |event| company_name.set(event.value()) }
                            }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "first-venue-name", "Первый ресторан или объект (необязательно)" }
                                input { id: "first-venue-name", class: "field-input", maxlength: "255", value: "{venue_name}", oninput: move |event| venue_name.set(event.value()) }
                            }
                            div { class: "company-onboarding-defaults",
                                span { "Часовой пояс: Europe/Moscow" }
                                span { "Язык: ru-RU" }
                            }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    let Some(request) = company_onboarding_input(&company_name(), &venue_name()) else { error.set(Some("Проверьте название организации и объекта.".into())); return; };
                                    if busy() { return; }
                                    let Ok(token) = create_session.current_token() else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = create_api.clone(); let session = create_session.clone();
                                    spawn(async move {
                                        let created = api.create_first_company(&token, &request).await;
                                        if generation() != current_generation { return; }
                                        match created {
                                            Ok(_) => match session.reload_bootstrap().await {
                                                Ok(_) if generation() == current_generation => { busy.set(false); status.set(Some("Организация готова.".into())); on_authenticated.call(()); },
                                                Ok(_) => {},
                                                Err(problem) => { busy.set(false); error.set(Some(safe_account_error(&problem).into())); },
                                            },
                                            Err(problem) => { busy.set(false); error.set(Some(safe_account_error(&problem).into())); },
                                        }
                                    });
                                },
                                if busy() { "Создание..." } else { "Создать" }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", disabled: busy(), onclick: move |_| mode.set(Mode::AccountCreated), "Назад" }
                        } }
                    },
                    Mode::ResetPassword => {
                        let reset_api = api.clone();
                        rsx! { div { class: "auth-form",
                            div { class: "form-field", label { class: "field-label", r#for: "standalone-reset-password", "Новый пароль" } input { id: "standalone-reset-password", class: "field-input", r#type: "password", autocomplete: "new-password", minlength: "12", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    let Some(challenge_id) = challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if busy() || !password_is_valid(&password()) { error.set(Some("Пароль должен содержать от 12 до 72 байт.".into())); return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let Some(canonical_phone) = canonical_russian_phone(&phone()) else { error.set(Some("Проверьте номер телефона.".into())); return; };
                                    let api = reset_api.clone(); let request = PasswordResetCompleteInput { challenge_id, phone: canonical_phone, new_password: password() };
                                    spawn(async move {
                                        let result = api.complete_password_reset(&request).await;
                                        if generation() != current_generation { return; }
                                        busy.set(false); password.set(String::new());
                                        match result { Ok(()) => { status.set(Some("Пароль изменён. Войдите с новым паролем.".into())); challenge.set(None); phone.set(String::new()); mode.set(Mode::Login); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Сохранение..." } else { "Сохранить новый пароль" }
                            }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| reset_flow(Mode::Login), "Отмена" }
                        } }
                    },
                    Mode::Invitation => rsx! {},
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
    fn default_and_explicit_routes_are_distinct() {
        assert_eq!(Mode::Login, Mode::Login);
        assert_ne!(Mode::Login, Mode::RegistrationPhone);
        assert_ne!(Mode::RegistrationPhone, Mode::ResetPhone);
        assert_ne!(Mode::AccountCreated, Mode::CreateCompany);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn phone_and_password_bounds_fail_closed() {
        assert!(russian_phone_is_complete("8 (999) 123-45-67"));
        assert!(!russian_phone_is_complete("123"));
        assert!(password_is_valid("correct horse"));
        assert!(!password_is_valid("short"));
        assert!(!password_is_valid(&"x".repeat(73)));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn login_errors_do_not_enumerate_accounts() {
        assert_eq!(
            generic_login_error(&AccountApiError::AuthenticationRequired),
            generic_login_error(&AccountApiError::InvalidRequest)
        );
        assert_eq!(
            generic_login_error(&AccountApiError::PermissionDenied),
            generic_login_error(&AccountApiError::InvalidRequest)
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn first_company_input_is_explicit_and_optional_venue_is_honest() {
        let absent = company_onboarding_input("  RestOS Pilot  ", "  ").unwrap();
        assert_eq!(absent.company_name, "RestOS Pilot");
        assert_eq!(absent.venue_name, None);
        assert_eq!(absent.timezone, "Europe/Moscow");
        assert_eq!(absent.locale, "ru-RU");
        let present = company_onboarding_input("RestOS", " Первый ресторан ").unwrap();
        assert_eq!(present.venue_name.as_deref(), Some("Первый ресторан"));
        assert!(company_onboarding_input("", "Venue").is_none());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn login_uses_one_submit_primary_and_preserves_all_routes() {
        let source = include_str!("standalone_account_auth.rs");
        let login = source
            .split_once("Mode::Login => {")
            .and_then(|(_, source)| source.split_once("Mode::RegistrationPhone | Mode::ResetPhone"))
            .map(|(source, _)| source)
            .unwrap_or_default();

        assert_eq!(login.matches("class: \"btn-primary w-full\"").count(), 1);
        assert!(login.contains("r#type: \"submit\""));
        assert!(login.contains("onsubmit:"));
        assert!(login.contains("if busy() { return; }"));
        assert!(login.contains("Войти с Face ID или ключом доступа"));
        assert!(login.contains("Забыли пароль?"));
        assert!(login.contains("Создать аккаунт"));
        assert!(login.contains("Регистрация по приглашению"));
        assert!(login.contains("Другие способы входа"));
        assert!(login.contains("Старый вход для существующей версии"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn login_fields_have_native_input_and_accessibility_contracts() {
        let source = include_str!("standalone_account_auth.rs");
        let shared_phone = include_str!("russian_phone_input.rs");
        assert!(source.contains("RussianPhoneInput {"));
        assert!(shared_phone.contains("inputmode: \"tel\""));
        assert!(shared_phone.contains("autocomplete: \"tel\""));
        assert!(source.contains("autocomplete: \"current-password\""));
        assert!(source.contains("aria_label: if password_visible()"));
        assert!(source.contains("\"Показать пароль\""));
        assert!(source.contains("\"Скрыть пароль\""));
        assert!(source.contains("invalid: login_phone_invalid()"));
        assert!(source.contains("aria_invalid: login_password_invalid()"));
        assert!(source.contains("described_by: if login_phone_invalid()"));
        assert!(source.contains("focus_auth_field"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn auth_layout_uses_responsive_split_without_new_security_state() {
        let source = include_str!("standalone_account_auth.rs")
            .split_once("pub fn AccountAuthPage")
            .and_then(|(_, source)| source.split_once("#[cfg(test)]"))
            .map(|(source, _)| source)
            .unwrap_or_default();
        let css = include_str!("../../assets/styling/brand_foundation.css");
        assert!(source.contains("class: \"account-auth-shell\""));
        assert!(source.contains("class: \"account-auth-brand__mark\""));
        assert!(css.contains("grid-template-columns: minmax(0, 1.05fr) minmax(400px, 440px)"));
        assert!(css.contains("width: min(1120px, 100%)"));
        assert!(css.contains(".account-auth-brand { display: none; }"));
        assert!(!source.contains("localStorage"));
        assert!(!source.contains("sessionStorage"));
    }
}
