use dioxus::prelude::*;
use crate::auth::AuthState;

#[derive(Clone, PartialEq)]
enum Tab { Login, Register }

#[component]
pub fn AuthPage(on_auth: EventHandler<AuthState>) -> Element {
    let mut tab = use_signal(|| Tab::Login);

    rsx! {
        div {
            class: "auth-root",
            div { class: "hero-glow" }
            div {
                class: "auth-card",
                // Header
                div {
                    class: "auth-header",
                    div { class: "auth-logo", "⭐" }
                    h1 { class: "auth-title", "Yarbot" }
                    p { class: "auth-subtitle", "Система оценки сотрудников" }
                }

                // Tab switcher
                div {
                    class: "tab-bar",
                    button {
                        class: if *tab.read() == Tab::Login { "tab active" } else { "tab" },
                        onclick: move |_| tab.set(Tab::Login),
                        "Вход"
                    }
                    button {
                        class: if *tab.read() == Tab::Register { "tab active" } else { "tab" },
                        onclick: move |_| tab.set(Tab::Register),
                        "Регистрация"
                    }
                }

                match *tab.read() {
                    Tab::Login => rsx! { LoginForm { on_auth } },
                    Tab::Register => rsx! { RegisterForm { on_auth } },
                }
            }
        }
    }
}

#[component]
fn LoginForm(on_auth: EventHandler<AuthState>) -> Element {
    let mut login = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    let on_submit = move |_: Event<MouseData>| {
        let l = login.read().clone();
        let p = password.read().clone();
        if l.is_empty() || p.is_empty() {
            error.set(Some("Заполните все поля".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        spawn(async move {
            match crate::api::login(&l, &p).await {
                Ok(resp) => {
                    let state = AuthState {
                        access_token: resp.access_token,
                        employee_id: resp.employee_id,
                        org_id: resp.org_id,
                        name: resp.name,
                        is_admin: resp.is_admin,
                    };
                    state.save();
                    on_auth.call(state);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "auth-form",
            div { class: "form-field",
                label { class: "field-label", "Логин" }
                input {
                    class: "field-input",
                    r#type: "text",
                    placeholder: "Ваш логин",
                    value: "{login}",
                    oninput: move |e| login.set(e.value()),
                }
            }
            div { class: "form-field",
                label { class: "field-label", "Пароль" }
                input {
                    class: "field-input",
                    r#type: "password",
                    placeholder: "••••••••",
                    value: "{password}",
                    oninput: move |e| password.set(e.value()),
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "error-msg", "{err}" }
            }
            button {
                class: "btn-primary w-full",
                disabled: *loading.read(),
                onclick: on_submit,
                if *loading.read() { "Вход..." } else { "Войти" }
            }
        }
    }
}

#[component]
fn RegisterForm(on_auth: EventHandler<AuthState>) -> Element {
    let mut invite_code = use_signal(String::new);
    let mut full_name = use_signal(String::new);
    let mut login = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);

    let on_submit = move |_: Event<MouseData>| {
        let code = invite_code.read().clone();
        let name = full_name.read().clone();
        let l = login.read().clone();
        let p = password.read().clone();
        if code.is_empty() || name.is_empty() || l.is_empty() || p.is_empty() {
            error.set(Some("Заполните все поля".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        spawn(async move {
            match crate::api::register(&code, &name, &l, &p).await {
                Ok(resp) => {
                    let state = AuthState {
                        access_token: resp.access_token,
                        employee_id: resp.employee_id,
                        org_id: resp.org_id,
                        name: resp.name,
                        is_admin: resp.is_admin,
                    };
                    state.save();
                    on_auth.call(state);
                }
                Err(e) => {
                    error.set(Some(e));
                    loading.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "auth-form",
            div { class: "form-field",
                label { class: "field-label", "Код приглашения" }
                input {
                    class: "field-input",
                    r#type: "text",
                    placeholder: "Введите код от администратора",
                    value: "{invite_code}",
                    oninput: move |e| invite_code.set(e.value()),
                }
            }
            div { class: "form-field",
                label { class: "field-label", "Полное имя" }
                input {
                    class: "field-input",
                    r#type: "text",
                    placeholder: "Иван Иванов",
                    value: "{full_name}",
                    oninput: move |e| full_name.set(e.value()),
                }
            }
            div { class: "form-field",
                label { class: "field-label", "Логин" }
                input {
                    class: "field-input",
                    r#type: "text",
                    placeholder: "Придумайте логин",
                    value: "{login}",
                    oninput: move |e| login.set(e.value()),
                }
            }
            div { class: "form-field",
                label { class: "field-label", "Пароль" }
                input {
                    class: "field-input",
                    r#type: "password",
                    placeholder: "Придумайте пароль",
                    value: "{password}",
                    oninput: move |e| password.set(e.value()),
                }
            }
            if let Some(err) = error.read().as_ref() {
                div { class: "error-msg", "{err}" }
            }
            button {
                class: "btn-primary w-full",
                disabled: *loading.read(),
                onclick: on_submit,
                if *loading.read() { "Регистрация..." } else { "Зарегистрироваться" }
            }
        }
    }
}
