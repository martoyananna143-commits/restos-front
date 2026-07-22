//! Placeholder for the upcoming internships workflow.

use dioxus::prelude::*;

#[component]
pub fn InternshipsPage() -> Element {
    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll internships-screen",
                header { class: "page-header page-header-spacious",
                    div {
                        div { class: "label-text", "Развитие команды" }
                        h1 { class: "page-title page-title-lg", "Текущие стажировки" }
                    }
                }
                section { class: "internships-empty", aria_label: "Стажировки пока недоступны",
                    div { class: "internships-empty-icon", aria_hidden: "true", "◷" }
                    h2 { "Раздел готовится" }
                    p { "Здесь появятся текущие стажировки, когда для них будет настроен рабочий процесс." }
                }
            }
        }
    }
}
