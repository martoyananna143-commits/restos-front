//! Shared Russian phone input. UI state contains national digits only; API values are E.164.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;

pub const RUSSIAN_PHONE_DIGITS: usize = 10;

pub fn russian_phone_national_digits(value: &str) -> String {
    let mut digits = value
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>();
    if digits.len() >= 11 && matches!(digits.as_bytes().first(), Some(b'7' | b'8')) {
        digits.remove(0);
    }
    digits.chars().take(RUSSIAN_PHONE_DIGITS).collect()
}

pub fn russian_phone_is_complete(value: &str) -> bool {
    russian_phone_national_digits(value).len() == RUSSIAN_PHONE_DIGITS
}

pub fn canonical_russian_phone(value: &str) -> Option<String> {
    let digits = russian_phone_national_digits(value);
    (digits.len() == RUSSIAN_PHONE_DIGITS).then(|| format!("+7{digits}"))
}

pub fn format_russian_phone_national(value: &str) -> String {
    let digits = russian_phone_national_digits(value);
    let mut formatted = String::with_capacity(15);
    for (index, digit) in digits.chars().enumerate() {
        match index {
            0 => formatted.push('('),
            3 => formatted.push_str(") "),
            6 | 8 => formatted.push('-'),
            _ => {}
        }
        formatted.push(digit);
    }
    formatted
}

fn stabilize_phone_caret(id: String, position: u32) {
    spawn(async move {
        TimeoutFuture::new(0).await;
        let Some(input) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&id))
            .and_then(|element| element.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let _ = input.set_selection_range(position, position);
    });
}

#[component]
pub fn RussianPhoneInput(
    id: String,
    label: String,
    value: String,
    on_change: EventHandler<String>,
    invalid: bool,
    disabled: bool,
    described_by: String,
) -> Element {
    let formatted = format_russian_phone_national(&value);
    let input_id = id.clone();
    let accessible_label = format!("{label}, код страны плюс семь");

    rsx! {
        div { class: "form-field russian-phone-field",
            label { class: "field-label", r#for: "{id}", "{label}" }
            div { class: "russian-phone-control",
                span { class: "russian-phone-prefix", aria_hidden: "true", "+7" }
                input {
                    id: "{id}",
                    class: "field-input russian-phone-input",
                    r#type: "tel",
                    inputmode: "tel",
                    autocomplete: "tel",
                    placeholder: "(___) ___-__-__",
                    value: "{formatted}",
                    disabled,
                    aria_label: "{accessible_label}",
                    aria_invalid: invalid,
                    aria_describedby: "{described_by}",
                    oninput: move |event| {
                        let digits = russian_phone_national_digits(&event.value());
                        let caret = format_russian_phone_national(&digits).len() as u32;
                        on_change.call(digits);
                        stabilize_phone_caret(input_id.clone(), caret);
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
    fn accepted_paste_and_autofill_shapes_are_identical() {
        for value in [
            "89991234567",
            "79991234567",
            "+79991234567",
            "9991234567",
            "+7 (999) 123-45-67",
        ] {
            assert_eq!(russian_phone_national_digits(value), "9991234567");
            assert_eq!(
                canonical_russian_phone(value).as_deref(),
                Some("+79991234567")
            );
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn typing_and_deletion_keep_a_bounded_national_value() {
        assert_eq!(russian_phone_national_digits("9"), "9");
        assert_eq!(russian_phone_national_digits("999123456789"), "9991234567");
        assert_eq!(russian_phone_national_digits("(999) 123-45-6"), "999123456");
        assert!(!russian_phone_is_complete("999123456"));
        assert!(russian_phone_is_complete("9991234567"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn visual_mask_is_deterministic() {
        assert_eq!(format_russian_phone_national(""), "");
        assert_eq!(format_russian_phone_national("9"), "(9");
        assert_eq!(format_russian_phone_national("999"), "(999");
        assert_eq!(format_russian_phone_national("9991"), "(999) 1");
        assert_eq!(
            format_russian_phone_national("9991234567"),
            "(999) 123-45-67"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn incomplete_values_never_create_an_api_phone() {
        assert_eq!(canonical_russian_phone(""), None);
        assert_eq!(canonical_russian_phone("999123456"), None);
        assert_eq!(
            canonical_russian_phone("9991234567").as_deref(),
            Some("+79991234567")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn component_contract_has_mobile_and_caret_attributes() {
        let source = include_str!("russian_phone_input.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for required in [
            "inputmode: \"tel\"",
            "autocomplete: \"tel\"",
            "r#type: \"tel\"",
            "set_selection_range",
            "russian-phone-prefix",
            "placeholder: \"(___) ___-__-__\"",
        ] {
            assert!(production.contains(required), "missing {required}");
        }
        assert!(!production.contains("localStorage"));
        assert!(!production.contains("sessionStorage"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn every_account_phone_flow_uses_the_shared_component() {
        let production =
            |source: &'static str| source.split("#[cfg(test)]").next().unwrap_or(source);
        let standalone = production(include_str!("standalone_account_auth.rs"));
        let invitation = production(include_str!("account_portal.rs"));
        let workforce = production(include_str!("workforce_onboarding.rs"));

        assert_eq!(standalone.matches("RussianPhoneInput {").count(), 2);
        assert_eq!(invitation.matches("RussianPhoneInput {").count(), 1);
        assert_eq!(workforce.matches("RussianPhoneInput {").count(), 1);
        for source in [standalone, invitation, workforce] {
            assert!(!source.contains("r#type: \"tel\""));
            assert!(!source.contains("inputmode: \"tel\""));
            assert!(!source.contains("autocomplete: \"tel\""));
        }
        assert!(standalone.contains("Mode::RegistrationPhone | Mode::ResetPhone"));
        assert!(standalone.contains("PasswordLoginInput { phone: canonical_phone"));
        assert!(invitation
            .contains("SmsRequestInput { invitation_code: invitation(), phone: canonical_phone"));
        assert!(workforce.contains("canonical_russian_phone(&employee_phone())"));
    }
}
