//! Canonical public-document links for Account authentication flows.

use dioxus::prelude::*;

const PRIVACY_URL: &str = "https://www.restos.space/privacy.html";
const PERSONAL_DATA_URL: &str = "https://www.restos.space/personal-data.html";
const TERMS_URL: &str = "https://www.restos.space/terms.html";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountLegalContext {
    Login,
    RegistrationSms,
    PasswordResetSms,
    AccountCreation,
    InvitationRegistration,
}

impl AccountLegalContext {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::RegistrationSms => "registration_sms",
            Self::PasswordResetSms => "password_reset_sms",
            Self::AccountCreation => "account_creation",
            Self::InvitationRegistration => "invitation_registration",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LegalLink {
    label: &'static str,
    url: &'static str,
}

const LEGAL_LINKS: [LegalLink; 3] = [
    LegalLink {
        label: "Политика конфиденциальности",
        url: PRIVACY_URL,
    },
    LegalLink {
        label: "Обработка персональных данных",
        url: PERSONAL_DATA_URL,
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
                for link in LEGAL_LINKS {
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
        assert_eq!(LEGAL_LINKS.len(), 3);
        for link in LEGAL_LINKS {
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
        let contexts = [
            AccountLegalContext::Login,
            AccountLegalContext::RegistrationSms,
            AccountLegalContext::PasswordResetSms,
            AccountLegalContext::AccountCreation,
            AccountLegalContext::InvitationRegistration,
        ];
        for context in contexts {
            assert!(approved_copy(context).is_none());
            assert!(!context.as_str().is_empty());
        }
        assert!(!NEUTRAL_TEST_COPY.is_empty());
    }
}
