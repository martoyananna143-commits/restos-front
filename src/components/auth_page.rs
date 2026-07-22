use dioxus::prelude::*;
use crate::auth::AuthState;
use crate::types::OrgInfo;

#[derive(Clone, PartialEq)]
enum Tab { Login, Register }

#[component]
pub fn AuthPage(on_auth: EventHandler<AuthState>) -> Element {
    let mut tab = use_signal(|| Tab::Login);

    use_effect(move || {
        if let Some(window) = web_sys::window() {
            if let Ok(search) = window.location().search() {
                if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                    let wants_register = params
                        .get("auth")
                        .map(|v| v.eq_ignore_ascii_case("register"))
                        .unwrap_or(false);
                    if wants_register || params.get("invite_code").is_some() || params.get("invite").is_some() {
                        tab.set(Tab::Register);
                    }
                }
            }
        }
    });

    rsx! {
        div {
            class: "auth-root",
            div {
                class: "auth-card",
                // Header
                div {
                    class: "auth-header",
                    div { class: "auth-logo", "⭐" }
                    h1 { class: "auth-title", "Restos" }
                    p { class: "auth-subtitle", "Система замеров работы сотрудников" }
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
                        is_superuser: resp.is_superuser,
                        available_orgs: resp.available_orgs,
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
    let mut org_name = use_signal(String::new);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loading = use_signal(|| false);
    // None = not checked yet, Some(true) = standalone, Some(false) = org invite
    let mut is_standalone: Signal<Option<bool>> = use_signal(|| None);
    let mut org_hint = use_signal(|| Option::<String>::None);

    // Read invite code from URL on mount
    use_effect(move || {
        if invite_code().is_empty() {
            if let Some(window) = web_sys::window() {
                if let Ok(search) = window.location().search() {
                    if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                        let val = params
                            .get("invite_code")
                            .or_else(|| params.get("invite"))
                            .unwrap_or_default();
                        if !val.is_empty() {
                            invite_code.set(val);
                        }
                    }
                }
            }
        }
    });

    // When invite_code changes and is non-empty, fetch invite info
    use_effect(move || {
        let code = invite_code();
        if code.len() > 8 {
            spawn(async move {
                if let Ok(info) = crate::api::fetch_invite_info(&code).await {
                    is_standalone.set(Some(info.is_standalone));
                    org_hint.set(info.organization_name);
                }
            });
        }
    });

    let on_submit = move |_: Event<MouseData>| {
        let code = invite_code.read().clone();
        let name = full_name.read().clone();
        let l = login.read().clone();
        let p = password.read().clone();
        let standalone = is_standalone().unwrap_or(false);
        let oname = org_name.read().clone();

        if code.is_empty() || name.is_empty() || l.is_empty() || p.is_empty() {
            error.set(Some("Заполните все поля".into()));
            return;
        }
        if standalone && oname.trim().is_empty() {
            error.set(Some("Введите название организации".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        spawn(async move {
            let org_arg = if standalone { Some(oname.trim().to_string()) } else { None };
            match crate::api::register(&code, &name, &l, &p, org_arg.as_deref()).await {
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
                // Hint: show org name if regular invite, or standalone badge
                if let Some(standalone) = is_standalone() {
                    if standalone {
                        div { style: "font-size:12px; color:var(--accent); margin-top:4px;",
                            "✦ Приглашение для создания новой организации"
                        }
                    } else if let Some(hint) = org_hint.read().clone() {
                        div { style: "font-size:12px; color:var(--text3); margin-top:4px;",
                            "Организация: {hint}"
                        }
                    }
                }
            }

            // Show org name field only for standalone invites
            if is_standalone().unwrap_or(false) {
                div { class: "form-field",
                    label { class: "field-label", "Название организации" }
                    input {
                        class: "field-input",
                        r#type: "text",
                        placeholder: "Например: ООО Ромашка",
                        value: "{org_name}",
                        oninput: move |e| org_name.set(e.value()),
                    }
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
