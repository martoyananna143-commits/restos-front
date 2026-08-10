//! Canonical public-document links for Account authentication flows.

use dioxus::prelude::*;

const PRIVACY_URL: &str = "https://www.restos.space/privacy.html";
const PERSONAL_DATA_URL: &str = "https://www.restos.space/personal-data.html";
const TERMS_URL: &str = "https://www.restos.space/terms.html";
const REGISTRATION_SMS_CONSENT_COPY: &str = "Я согласен(на) получить сервисное SMS с одноразовым кодом для подтверждения номера телефона. Рекламные и маркетинговые SMS RestOS не отправляет.";
const PASSWORD_RECOVERY_SMS_CONSENT_COPY: &str = "Я согласен(на) получить сервисное SMS с одноразовым кодом для восстановления доступа к RestOS.";

pub const fn registration_sms_request_allowed(
    personal_data_consent: bool,
    authorization_sms_consent: bool,
) -> bool {
    personal_data_consent && authorization_sms_consent
}

pub const fn password_recovery_sms_request_allowed(authorization_sms_consent: bool) -> bool {
    authorization_sms_consent
}

#[component]
pub fn RegistrationSmsLegalControls(
    mut personal_data_consent: Signal<bool>,
    mut authorization_sms_consent: Signal<bool>,
) -> Element {
    rsx! {
        fieldset { class: "account-legal-controls",
            legend { class: "sr-only", "Согласия для получения кода регистрации" }
            label { class: "account-legal-control",
                input {
                    r#type: "checkbox",
                    checked: personal_data_consent(),
                    onchange: move |event| personal_data_consent.set(event.checked()),
                }
                span {
                    "Я даю согласие на обработку моих персональных данных для регистрации, аутентификации и обеспечения безопасности RestOS в соответствии с "
                    a { href: PERSONAL_DATA_URL, target: "_blank", rel: "noopener noreferrer", "Согласием на обработку персональных данных" }
                    "."
                }
            }
            label { class: "account-legal-control",
                input {
                    r#type: "checkbox",
                    checked: authorization_sms_consent(),
                    onchange: move |event| authorization_sms_consent.set(event.checked()),
                }
                span { "{REGISTRATION_SMS_CONSENT_COPY}" }
            }
            p { class: "account-legal-help", "Для регистрации по номеру телефона требуется одноразовый код из SMS." }
        }
    }
}

#[component]
pub fn PasswordRecoverySmsLegalControl(mut authorization_sms_consent: Signal<bool>) -> Element {
    rsx! {
        fieldset { class: "account-legal-controls",
            legend { class: "sr-only", "Согласие для получения кода восстановления" }
            label { class: "account-legal-control",
                input {
                    r#type: "checkbox",
                    checked: authorization_sms_consent(),
                    onchange: move |event| authorization_sms_consent.set(event.checked()),
                }
                span { "{PASSWORD_RECOVERY_SMS_CONSENT_COPY}" }
            }
        }
    }
}

#[component]
pub fn AccountCreationLegalNotice() -> Element {
    rsx! {
        p { class: "account-legal-copy",
            "Нажимая «Создать аккаунт», вы принимаете "
            a { href: TERMS_URL, target: "_blank", rel: "noopener noreferrer", "Условия использования RestOS" }
            " и подтверждаете, что ознакомились с "
            a { href: PRIVACY_URL, target: "_blank", rel: "noopener noreferrer", "Политикой конфиденциальности" }
            "."
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountLegalContext {
    Login,
}

impl AccountLegalContext {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LegalLink {
    label: &'static str,
    url: &'static str,
}

const LOGIN_LINKS: [LegalLink; 2] = [
    LegalLink {
        label: "Политика конфиденциальности",
        url: PRIVACY_URL,
    },
    LegalLink {
        label: "Условия использования",
        url: TERMS_URL,
    },
];

const fn approved_copy(_context: AccountLegalContext) -> Option<&'static str> {
    None
}

#[component]
pub fn AccountLegalNotice(context: AccountLegalContext) -> Element {
    let context_name = context.as_str();
    let copy = approved_copy(context);

    rsx! {
        aside {
            class: "account-legal-notice",
            aria_label: "Юридическая информация",
            "data-legal-context": "{context_name}",
            if let Some(copy) = copy {
                p { class: "account-legal-copy", "{copy}" }
            }
            nav { class: "account-legal-links", aria_label: "Публичные документы RestOS",
                for link in LOGIN_LINKS {
                    a {
                        href: link.url,
                        target: "_blank",
                        rel: "noopener noreferrer",
                        "{link.label}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEUTRAL_TEST_COPY: &str = "Нейтральный тестовый текст";

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn canonical_links_are_https_allowlisted_and_credential_free() {
        assert_eq!(LOGIN_LINKS.len(), 2);
        for link in LOGIN_LINKS {
            assert!(link.url.starts_with("https://www.restos.space/"));
            assert!(!link.url.contains('?'));
            assert!(!link.url.contains('#'));
            assert!(!link.url.contains('@'));
            assert!(!link.label.is_empty());
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn every_auth_context_uses_the_same_links_without_unapproved_copy() {
        let contexts = [AccountLegalContext::Login];
        for context in contexts {
            assert!(approved_copy(context).is_none());
            assert!(!context.as_str().is_empty());
        }
        assert!(!NEUTRAL_TEST_COPY.is_empty());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn consent_controls_are_fail_closed_and_copy_is_exact() {
        assert!(!registration_sms_request_allowed(false, false));
        assert!(!registration_sms_request_allowed(true, false));
        assert!(!registration_sms_request_allowed(false, true));
        assert!(registration_sms_request_allowed(true, true));
        assert!(!password_recovery_sms_request_allowed(false));
        assert!(password_recovery_sms_request_allowed(true));
        assert_eq!(REGISTRATION_SMS_CONSENT_COPY, "Я согласен(на) получить сервисное SMS с одноразовым кодом для подтверждения номера телефона. Рекламные и маркетинговые SMS RestOS не отправляет.");
        assert_eq!(PASSWORD_RECOVERY_SMS_CONSENT_COPY, "Я согласен(на) получить сервисное SMS с одноразовым кодом для восстановления доступа к RestOS.");
    }
}
