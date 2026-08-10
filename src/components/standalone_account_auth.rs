//! Standalone Account login, registration, and recovery. Secrets remain in component memory only.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;

use crate::{
    account_api::{
        AccountApiClient, AccountApiError, PasswordLoginInput, PasswordResetCompleteInput,
        SmsRequested, StandaloneRegistrationInput, StandaloneSmsRequestInput,
        StandaloneSmsVerifyInput,
    },
    account_session::AccountSessionAdapter,
    device_identity::DeviceIdentityAdapter,
    passkey::{PasskeyAdapter, PasskeyError},
};

use super::{
    account_legal_notice::{AccountLegalContext, AccountLegalNotice},
    account_portal::{safe_account_error, safe_passkey_error, InvitationAccountAuthPage},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Login,
    RegistrationPhone,
    RegistrationOtp,
    RegistrationDetails,
    AccountCreated,
    ResetPhone,
    ResetOtp,
    ResetPassword,
    Invitation,
}

fn phone_is_plausible(value: &str) -> bool {
    let digits = value.chars().filter(char::is_ascii_digit).count();
    (10..=15).contains(&digits) && value.len() <= 32
}

fn password_is_valid(value: &str) -> bool {
    (12..=72).contains(&value.as_bytes().len())
}

fn generic_login_error(error: &AccountApiError) -> &'static str {
    match error {
        AccountApiError::AuthenticationRequired
        | AccountApiError::PermissionDenied
        | AccountApiError::InvalidRequest => "Неверный номер телефона или пароль.",
        _ => safe_account_error(error),
    }
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
    let mut display_name = use_signal(String::new);
    let mut otp = use_signal(String::new);
    let mut challenge: Signal<Option<Uuid>> = use_signal(|| None);
    let mut resend_ready = use_signal(|| false);
    let mut resend_generation = use_signal(|| 0_u64);

    let mut reset_flow = move |next: Mode| {
        generation += 1;
        resend_generation += 1;
        busy.set(false);
        error.set(None);
        status.set(None);
        phone.set(String::new());
        password.set(String::new());
        display_name.set(String::new());
        otp.set(String::new());
        challenge.set(None);
        resend_ready.set(false);
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
        Mode::ResetPhone | Mode::ResetOtp | Mode::ResetPassword => "Восстановление пароля",
        Mode::Invitation => "Регистрация по приглашению",
    };

    rsx! {
        div { class: "auth-root account-auth-root",
            div { class: "auth-card account-auth-card",
                div { class: "auth-header",
                    div { class: "auth-title", "RestOS" }
                    h1 { class: "account-auth-heading", "{heading}" }
                    p { class: "auth-subtitle", "Безопасный вход в рабочий аккаунт RestOS." }
                }
                div { class: "account-live", role: "status", aria_live: "polite",
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
                        rsx! { div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "standalone-login-phone", "Номер телефона" }
                                input { id: "standalone-login-phone", class: "field-input", r#type: "tel", autocomplete: "tel", value: "{phone}", oninput: move |event| phone.set(event.value()) }
                            }
                            div { class: "form-field",
                                label { class: "field-label", r#for: "standalone-login-password", "Пароль" }
                                input { id: "standalone-login-password", class: "field-input", r#type: "password", autocomplete: "current-password", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) }
                            }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    if busy() || !phone_is_plausible(&phone()) || password().is_empty() { error.set(Some("Проверьте номер телефона и пароль.".into())); return; }
                                    busy.set(true); error.set(None); generation += 1;
                                    let current_generation = generation();
                                    let api = login_api.clone(); let identities = login_identities.clone(); let session = login_session.clone();
                                    let request = PasswordLoginInput { phone: phone(), password: password(), platform: "web".into(), device_display_name: Some("Браузер RestOS".into()) };
                                    spawn(async move {
                                        let result = api.password_login(&identities, &request).await;
                                        if generation() != current_generation { return; }
                                        busy.set(false); password.set(String::new());
                                        match result { Ok(registered) => { session.accept_registered_session(registered); on_authenticated.call(()); }, Err(problem) => error.set(Some(generic_login_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Вход..." } else { "Войти" }
                            }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy(),
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
                            button { class: "btn-ghost account-auth-link", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::ResetPhone), "Забыли пароль?" }
                            button { class: "btn-ghost account-auth-link", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::RegistrationPhone), "Создать аккаунт" }
                            button { class: "btn-ghost account-auth-link", r#type: "button", disabled: busy(), onclick: move |_| reset_flow(Mode::Invitation), "Регистрация по приглашению" }
                            button { class: "btn-ghost account-auth-link", r#type: "button", onclick: move |_| on_legacy_login.call(()), "Старый вход для существующей версии" }
                            AccountLegalNotice { context: AccountLegalContext::Login }
                        } }
                    },
                    Mode::RegistrationPhone | Mode::ResetPhone => {
                        let request_api = api.clone();
                        let is_registration = mode() == Mode::RegistrationPhone;
                        rsx! { div { class: "auth-form",
                            div { class: "form-field",
                                label { class: "field-label", r#for: "standalone-phone", "Номер телефона" }
                                input { id: "standalone-phone", class: "field-input", r#type: "tel", autocomplete: "tel", value: "{phone}", oninput: move |event| phone.set(event.value()) }
                            }
                            AccountLegalNotice { context: if is_registration { AccountLegalContext::RegistrationSms } else { AccountLegalContext::PasswordResetSms } }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    if busy() || !phone_is_plausible(&phone()) { error.set(Some("Проверьте номер телефона.".into())); return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = request_api.clone(); let request = StandaloneSmsRequestInput { phone: phone() };
                                    spawn(async move {
                                        let result = if is_registration { api.request_registration_sms(&request).await } else { api.request_password_reset_sms(&request).await };
                                        if generation() != current_generation { return; }
                                        busy.set(false);
                                        match result { Ok(requested) => { challenge.set(Some(requested.challenge_id)); otp.set(String::new()); resend_ready.set(false); resend_generation += 1; arm_resend_timer(requested.resend_available_at, resend_ready, resend_generation(), resend_generation); mode.set(if is_registration { Mode::RegistrationOtp } else { Mode::ResetOtp }); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Отправка..." } else { "Получить код" }
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
                                    let api = verify_api.clone(); let request = StandaloneSmsVerifyInput { challenge_id, phone: phone(), code: otp() };
                                    spawn(async move {
                                        let result = if is_registration { api.verify_registration_sms(&request).await } else { api.verify_password_reset_sms(&request).await };
                                        if generation() != current_generation { return; }
                                        busy.set(false); otp.set(String::new());
                                        match result { Ok(()) => { resend_generation += 1; resend_ready.set(false); mode.set(if is_registration { Mode::RegistrationDetails } else { Mode::ResetPassword }); }, Err(problem) => error.set(Some(safe_account_error(&problem).into())) }
                                    });
                                },
                                if busy() { "Проверка..." } else { "Подтвердить код" }
                            }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy() || !resend_ready(),
                                onclick: move |_| {
                                    if busy() || !resend_ready() { return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = resend_api.clone(); let request = StandaloneSmsRequestInput { phone: phone() };
                                    spawn(async move {
                                        let result = if is_registration { api.request_registration_sms(&request).await } else { api.request_password_reset_sms(&request).await };
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
                            div { class: "form-field", label { class: "field-label", r#for: "standalone-name", "Имя" } input { id: "standalone-name", class: "field-input", autocomplete: "name", maxlength: "255", value: "{display_name}", oninput: move |event| display_name.set(event.value()) } }
                            div { class: "form-field", label { class: "field-label", r#for: "standalone-new-password", "Пароль" } input { id: "standalone-new-password", class: "field-input", r#type: "password", autocomplete: "new-password", minlength: "12", maxlength: "72", value: "{password}", oninput: move |event| password.set(event.value()) } }
                            p { class: "account-auth-help", "От 12 до 72 байт. Не используйте пароль от других сервисов." }
                            AccountLegalNotice { context: AccountLegalContext::AccountCreation }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
                                onclick: move |_| {
                                    let Some(challenge_id) = challenge() else { error.set(Some("Запросите новый код.".into())); return; };
                                    if busy() || display_name().trim().is_empty() || !password_is_valid(&password()) { error.set(Some("Проверьте имя и длину пароля.".into())); return; }
                                    busy.set(true); error.set(None); generation += 1; let current_generation = generation();
                                    let api = register_api.clone(); let identities = register_identities.clone(); let session = register_session.clone();
                                    let request = StandaloneRegistrationInput { phone_verification_challenge_id: challenge_id, phone: phone(), display_name: display_name().trim().into(), password: password(), platform: "web".into(), device_display_name: Some("Браузер RestOS".into()) };
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
                            p { class: "account-auth-help", "Ключ доступа позволит входить с Face ID, Touch ID или кодом устройства." }
                            button { class: "btn-primary w-full", r#type: "button", disabled: busy(),
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
                                "Добавить Face ID или ключ доступа"
                            }
                            button { class: "btn-secondary w-full", r#type: "button", disabled: busy(), onclick: move |_| on_authenticated.call(()), "Продолжить без ключа доступа" }
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
                                    let api = reset_api.clone(); let request = PasswordResetCompleteInput { challenge_id, phone: phone(), new_password: password() };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn default_and_explicit_routes_are_distinct() {
        assert_eq!(Mode::Login, Mode::Login);
        assert_ne!(Mode::Login, Mode::RegistrationPhone);
        assert_ne!(Mode::RegistrationPhone, Mode::ResetPhone);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn phone_and_password_bounds_fail_closed() {
        assert!(phone_is_plausible("8 (999) 123-45-67"));
        assert!(!phone_is_plausible("123"));
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
}
