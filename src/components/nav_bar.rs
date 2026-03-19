//! Bottom navigation bar — signal-driven, no URL tokens needed.

use dioxus::prelude::*;

#[component]
pub fn NavBar(active: String, on_navigate: EventHandler<String>) -> Element {
    rsx! {
        nav { class: "bottom-nav",
            NavItem {
                page: "home",
                icon: "🏠",
                label: "Главная",
                active: active == "home",
                on_navigate,
            }
            NavItem {
                page: "evaluations",
                icon: "⭐",
                label: "Оценки",
                active: active == "evaluations",
                on_navigate,
            }
            NavItem {
                page: "analytics",
                icon: "📈",
                label: "Аналитика",
                active: active == "analytics",
                on_navigate,
            }
            NavItem {
                page: "employees",
                icon: "👥",
                label: "Команда",
                active: active == "employees",
                on_navigate,
            }
        }
    }
}

#[component]
fn NavItem(
    page: &'static str,
    icon: &'static str,
    label: &'static str,
    active: bool,
    on_navigate: EventHandler<String>,
) -> Element {
    rsx! {
        div {
            class: if active { "nav-item active" } else { "nav-item" },
            onclick: move |_| on_navigate.call(page.to_string()),
            span { class: "nav-icon", "{icon}" }
            span { class: "nav-label", "{label}" }
            if active {
                div { class: "nav-dot" }
            }
        }
    }
}
