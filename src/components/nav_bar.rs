//! Responsive application sidebar and mobile drawer.

use dioxus::prelude::*;

use crate::auth::AuthState;

#[component]
pub fn NavBar(
    active: String,
    show_evaluations: bool,
    is_employee_role: bool,
    open: bool,
    on_close: EventHandler<()>,
    on_logout: EventHandler<()>,
    on_navigate: EventHandler<String>,
) -> Element {
    let auth = AuthState::load();
    let user_name = auth.as_ref().map(|state| state.name.clone()).unwrap_or_default();
    let organization = auth
        .as_ref()
        .and_then(|state| state.available_orgs.iter().find(|org| org.id == state.org_id))
        .map(|org| org.name.clone())
        .unwrap_or_else(|| "Организация".to_string());

    rsx! {
        button {
            class: if open { "sidebar-overlay sidebar-overlay--open" } else { "sidebar-overlay" },
            r#type: "button",
            aria_label: "Закрыть меню",
            tabindex: if open { "0" } else { "-1" },
            onclick: move |_| on_close.call(()),
        }
        aside {
            class: if open { "app-sidebar app-sidebar--open" } else { "app-sidebar" },
            aria_label: "Основная навигация",
            div { class: "sidebar-head",
                div { class: "sidebar-brand",
                    BrandMark {}
                    div {
                        strong { "RestOS" }
                        span { "Управление качеством" }
                    }
                }
                button {
                    class: "sidebar-close",
                    r#type: "button",
                    aria_label: "Закрыть меню",
                    "data-drawer-close": "true",
                    onclick: move |_| on_close.call(()),
                    "×"
                }
            }
            nav { class: "sidebar-nav", aria_label: "Разделы приложения",
                NavItem { page: "home", label: "Главная", icon: "⌂", active: active == "home", on_navigate }
                if show_evaluations {
                    NavItem { page: "evaluations", label: "Замеры", icon: "◎", active: active == "evaluations" || active == "form", on_navigate }
                }
                NavItem {
                    page: "employees",
                    label: (if is_employee_role { "Мой профиль" } else { "Сотрудники" }).to_string(),
                    icon: "♙",
                    active: active == "employees",
                    on_navigate,
                }
                NavItem { page: "analytics", label: "Аналитика", icon: "↗", active: active == "analytics", on_navigate }
                NavItem { page: "internships", label: "Текущие стажировки", icon: "◷", active: active == "internships", on_navigate }
            }
            div { class: "sidebar-footer",
                button {
                    class: if active == "profile" { "sidebar-profile sidebar-profile--active" } else { "sidebar-profile" },
                    r#type: "button",
                    aria_label: "Открыть профиль",
                    onclick: move |_| on_navigate.call("profile".to_string()),
                    span { class: "sidebar-avatar", { initials(&user_name) } }
                    span { class: "sidebar-profile-copy",
                        strong { "{user_name}" }
                        small { "{organization}" }
                    }
                }
                button {
                    class: "sidebar-logout",
                    r#type: "button",
                    aria_label: "Выйти из аккаунта",
                    onclick: move |_| on_logout.call(()),
                    span { aria_hidden: "true", "↪" }
                    "Выйти"
                }
            }
        }
    }
}

#[component]
pub fn BrandMark() -> Element {
    rsx! {
        span { class: "brand-mark", aria_hidden: "true",
            svg { view_box: "0 0 48 48", "focusable": "false",
                path { d: "M14 31V19c0-4.4 3.6-8 8-8h11c2.2 0 4 1.8 4 4s-1.8 4-4 4H23c-.6 0-1 .4-1 1v11c0 2.2-1.8 4-4 4s-4-1.8-4-4Z" }
                circle { cx: "33", cy: "31", r: "4" }
            }
        }
    }
}

#[component]
fn NavItem(
    page: &'static str,
    icon: &'static str,
    label: String,
    active: bool,
    on_navigate: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            class: if active { "sidebar-nav-item sidebar-nav-item--active" } else { "sidebar-nav-item" },
            r#type: "button",
            aria_current: if active { "page" } else { "false" },
            onclick: move |_| on_navigate.call(page.to_string()),
            span { class: "sidebar-nav-icon", aria_hidden: "true", "{icon}" }
            span { "{label}" }
        }
    }
}

fn initials(name: &str) -> String {
    let mut parts = name.split_whitespace().filter_map(|part| part.chars().next());
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) => format!("{first}{second}").to_uppercase(),
        (Some(first), None) => first.to_uppercase().to_string(),
        _ => "?".to_string(),
    }
}
