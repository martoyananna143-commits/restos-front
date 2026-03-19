//! Criteria select page — placeholder (not used in standalone web app).

use dioxus::prelude::*;

#[component]
pub fn CriteriaSelectPage() -> Element {
    rsx! {
        div { class: "page-container",
            div { class: "empty-state",
                div { class: "empty-icon", "📋" }
                p { class: "empty-text", "Выбор критериев недоступен" }
            }
        }
    }
}
