//! Профиль: выход, установка / смена дополнительного PIN (пароль входа не меняется).

use dioxus::prelude::*;

use super::shared::{ErrorView, LoadingView};
use crate::api;
use crate::auth::AuthState;

#[component]
pub fn AccountPage(
    token: String,
    on_logout: EventHandler<()>,
    on_auth_update: EventHandler<AuthState>,
) -> Element {
    let t = token.clone();
    let mut profile = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_me(&tok).await }
    });

    let mut login_pw = use_signal(String::new);
    let mut new_pw = use_signal(String::new);
    let mut confirm_pw = use_signal(String::new);
    let mut form_error: Signal<Option<String>> = use_signal(|| None);
    let mut form_ok: Signal<Option<String>> = use_signal(|| None);
    let mut saving = use_signal(|| false);

    match profile() {
        None => rsx! { LoadingView { message: "Загрузка профиля...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(m)) => {
            let has_pin = m.has_pin;
            let login_disp = m.web_login.clone().unwrap_or_else(|| "—".to_string());
            let org_disp = m
                .organization_name
                .clone()
                .unwrap_or_else(|| "—".to_string());
            let role_label = if m.is_superuser {
                "Суперюзер"
            } else if m.is_admin {
                "Администратор"
            } else {
                "Сотрудник"
            };

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        div { class: "form-header-bar",
                            span { class: "heading-md", "Профиль" }
                        }
                        div { class: "pad", style: "padding-top: 4px;",

                            div { class: "card card-glass",
                                div { class: "section-header",
                                    span { class: "heading-md", "Аккаунт" }
                                    span { class: "badge badge-muted", "{role_label}" }
                                }
                                div { style: "display:flex; flex-direction:column; gap:10px; margin-top:12px;",
                                    div { style: "display:flex; justify-content:space-between; gap:12px; flex-wrap:wrap;",
                                        span { class: "label-text", "Имя" }
                                        span { style: "font-size:14px; color:var(--text); text-align:right;", "{m.name}" }
                                    }
                                    div { style: "display:flex; justify-content:space-between; gap:12px; flex-wrap:wrap;",
                                        span { class: "label-text", "Логин" }
                                        span { style: "font-size:14px; color:var(--text); text-align:right; word-break:break-all;", "{login_disp}" }
                                    }
                                    div { style: "display:flex; justify-content:space-between; gap:12px; flex-wrap:wrap;",
                                        span { class: "label-text", "Организация" }
                                        span { style: "font-size:14px; color:var(--text); text-align:right;", "{org_disp}" }
                                    }
                                }
                            }

                            div { class: "card", style: "margin-top:14px;",
                                div { class: "heading-md", style: "margin-bottom:4px;",
                                    if has_pin { "Смена дополнительного PIN" } else { "Установка дополнительного PIN" }
                                }
                                div { class: "body-text", style: "color:var(--text3); font-size:13px; margin-bottom:14px;",
                                    "Пароль для входа по логину не меняется. PIN — второй фактор: его периодически нужно вводить после входа или длительного перерыва (срок задаётся на сервере). Установка PIN: 6 цифр + подтверждение текущим паролем входа."
                                }

                                if let Some(err) = form_error() {
                                    div { class: "error-msg", role: "alert", style: "margin-bottom:10px;", "{err}" }
                                }
                                if let Some(ok) = form_ok() {
                                    div { class: "card card-green", style: "padding:10px 14px; margin-bottom:10px; font-size:13px;",
                                        "{ok}"
                                    }
                                }

                                div { style: "display:flex; flex-direction:column; gap:14px;",
                                    div { class: "form-field",
                                        label { class: "field-label", "Пароль входа (подтверждение)" }
                                        input {
                                            class: "field-input",
                                            r#type: "password",
                                            autocomplete: "current-password",
                                            placeholder: "Как при входе в приложение",
                                            value: "{login_pw}",
                                            oninput: move |e| login_pw.set(e.value()),
                                        }
                                    }
                                    div { class: "form-field",
                                        label { class: "field-label",
                                            if has_pin { "Новый PIN" } else { "PIN (6 цифр)" }
                                        }
                                        input {
                                            class: "field-input",
                                            r#type: "password",
                                            inputmode: "numeric",
                                            maxlength: "6",
                                            autocomplete: "new-password",
                                            placeholder: "6 цифр",
                                            value: "{new_pw}",
                                            oninput: move |e| {
                                                let digits: String = e.value().chars().filter(|c| c.is_ascii_digit()).take(6).collect();
                                                new_pw.set(digits);
                                            },
                                        }
                                    }
                                    div { class: "form-field",
                                        label { class: "field-label", "Повтор PIN" }
                                        input {
                                            class: "field-input",
                                            r#type: "password",
                                            inputmode: "numeric",
                                            maxlength: "6",
                                            autocomplete: "new-password",
                                            placeholder: "Ещё раз",
                                            value: "{confirm_pw}",
                                            oninput: move |e| {
                                                let digits: String = e.value().chars().filter(|c| c.is_ascii_digit()).take(6).collect();
                                                confirm_pw.set(digits);
                                            },
                                        }
                                    }

                                    button {
                                        class: "btn-primary w-full",
                                        disabled: saving(),
                                        onclick: move |_| {
                                            form_error.set(None);
                                            form_ok.set(None);
                                            let lp = login_pw().trim().to_string();
                                            let n = new_pw().trim().to_string();
                                            let c = confirm_pw().trim().to_string();
                                            if lp.is_empty() {
                                                form_error.set(Some("Введите пароль входа.".into()));
                                                return;
                                            }
                                            if n.len() != 6 || !n.chars().all(|ch| ch.is_ascii_digit()) {
                                                form_error.set(Some("PIN должен состоять ровно из 6 цифр.".into()));
                                                return;
                                            }
                                            if n != c {
                                                form_error.set(Some("PIN и подтверждение не совпадают.".into()));
                                                return;
                                            }
                                            let tok = token.clone();
                                            spawn(async move {
                                                saving.set(true);
                                                match api::set_web_pin(&tok, &lp, &n).await {
                                                    Ok(resp) => {
                                                        login_pw.set(String::new());
                                                        new_pw.set(String::new());
                                                        confirm_pw.set(String::new());
                                                        form_ok.set(Some("PIN сохранён. Сессия обновлена.".into()));
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
                                                        on_auth_update.call(state);
                                                        profile.restart();
                                                    }
                                                    Err(e) => form_error.set(Some(e)),
                                                }
                                                saving.set(false);
                                            });
                                        },
                                        if saving() { "Сохранение..." } else { "Сохранить PIN" }
                                    }
                                }
                            }

                            div { class: "card", style: "margin-top:14px; border:1px solid rgba(248,113,113,0.2); background:rgba(248,113,113,0.06);",
                                div { class: "heading-md", style: "margin-bottom:8px;", "Выход" }
                                div { class: "body-text", style: "color:var(--text3); font-size:13px; margin-bottom:14px;",
                                    "Вы выйдете из аккаунта на этом устройстве. Данные на сервере сохранятся."
                                }
                                button {
                                    class: "btn-secondary w-full",
                                    style: "border-color:rgba(248,113,113,0.35); color:var(--red);",
                                    onclick: move |_| on_logout.call(()),
                                    "Выйти из аккаунта"
                                }
                            }

                            div { style: "height:24px;" }
                        }
                    }
                }
            }
        }
    }
}
