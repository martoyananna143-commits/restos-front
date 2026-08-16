//! Full-screen second-factor PIN entry (after login or when JWT ``pva`` expired).

use dioxus::prelude::*;

use crate::api;
use crate::auth::AuthState;

#[component]
pub fn PinStepUpScreen(token: String, on_success: EventHandler<AuthState>) -> Element {
    let mut pin = use_signal(String::new);
    let mut err = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    rsx! {
        div {
            class: "pin-gate-root",
            style: "position:fixed;inset:0;z-index:20000;display:flex;align-items:center;justify-content:center;padding:20px;background:rgba(14,15,17,0.94);backdrop-filter:blur(8px);",
            div {
                class: "pin-gate-card",
                style: "width:min(100%,400px);padding:22px;border-radius:20px;border:1px solid rgba(255,255,255,0.08);background:linear-gradient(180deg,rgba(255,255,255,0.06),rgba(255,255,255,0.03));box-shadow:0 18px 40px rgba(0,0,0,0.35);",
                div { class: "heading-md", style: "margin-bottom:8px;", "Дополнительный PIN" }
                div { class: "body-text", style: "color:var(--text3); font-size:14px; margin-bottom:16px; line-height:1.45;",
                    "Введите 6-значный PIN. Пароль входа остаётся прежним — PIN нужен как второй фактор и периодически после долгого перерыва."
                }
                if let Some(msg) = err() {
                    div { class: "error-msg", role: "alert", style: "margin-bottom:12px;", "{msg}" }
                }
                div { class: "form-field",
                    label { class: "field-label", "PIN-код" }
                    input {
                        class: "field-input",
                        r#type: "password",
                        inputmode: "numeric",
                        maxlength: "6",
                        autocomplete: "one-time-code",
                        placeholder: "••••••",
                        value: "{pin}",
                        oninput: move |e| {
                            let digits: String = e.value().chars().filter(|c| c.is_ascii_digit()).take(6).collect();
                            pin.set(digits);
                        },
                    }
                }
                button {
                    class: "btn-primary w-full",
                    style: "margin-top:8px;",
                    disabled: loading() || pin().len() != 6,
                    onclick: move |_| {
                        err.set(None);
                        let p = pin.read().clone();
                        if p.len() != 6 {
                            err.set(Some("Введите 6 цифр.".into()));
                            return;
                        }
                        loading.set(true);
                        let tok = token.clone();
                        spawn(async move {
                            match api::verify_web_pin(&tok, &p).await {
                                Ok(resp) => {
                                    let state = AuthState {
                                        access_token: resp.access_token,
                                        employee_id: resp.employee_id,
                                        org_id: resp.org_id,
                                        name: resp.name,
                                        is_admin: resp.is_admin,
                                        is_superuser: resp.is_superuser,
                                        available_orgs: resp.available_orgs,
                                    };
                                    state.save();
                                    loading.set(false);
                                    on_success.call(state);
                                }
                                Err(e) => {
                                    err.set(Some(e));
                                    loading.set(false);
                                }
                            }
                        });
                    },
                    if loading() { "Проверка..." } else { "Продолжить" }
                }
            }
        }
    }
}
