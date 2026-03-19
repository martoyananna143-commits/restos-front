//! Shared status views — loading spinner, error, success.

use dioxus::prelude::*;

#[component]
pub fn LoadingView(message: String) -> Element {
    rsx! {
        div { class: "loading-container",
            div { class: "spinner-dots",
                div { class: "dot dot-1" }
                div { class: "dot dot-2" }
                div { class: "dot dot-3" }
            }
            p { class: "loading-text", "{message}" }
        }
    }
}

#[component]
pub fn ErrorView(message: String) -> Element {
    rsx! {
        div { class: "error-container",
            div { class: "error-icon", "⚠️" }
            h2 { style: "font-size:18px; font-weight:500; color:var(--text);", "Ошибка" }
            p { style: "font-size:14px; color:var(--text2); font-weight:300;", "{message}" }
            button {
                class: "btn-retry",
                onclick: |_| {
                    let _ = web_sys::window()
                        .and_then(|w| w.location().reload().ok());
                },
                "Попробовать снова"
            }
        }
    }
}

#[component]
pub fn SuccessView(title: String, subtitle: String) -> Element {
    rsx! {
        div { class: "success-container",
            div { class: "success-icon", "✓" }
            h2 { style: "font-size:20px; font-weight:500; color:var(--green);", "{title}" }
            p { style: "font-size:14px; color:var(--text2); font-weight:300;", "{subtitle}" }
        }
    }
}
