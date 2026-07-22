//! Input components for different criterion types.
//! BooleanInput: large Yes/No buttons.
//! NumberInput:  1–5 rating button grid.
//! TextInput:    textarea.
//! CommentInput: collapsible comment textarea.

use dioxus::prelude::*;

/// Yes / No toggle buttons
#[component]
pub fn BooleanInput(value: Signal<Option<bool>>, disabled: bool) -> Element {
    let current = value();
    rsx! {
        div { class: "eval-btns",
            button {
                r#type: "button",
                class: if current == Some(false) { "eval-btn eval-btn-no selected" }
                       else { "eval-btn eval-btn-no" },
                disabled,
                onclick: move |_| value.set(Some(false)),
                div { class: "eval-btn-icon", "✕" }
                div { class: "eval-btn-label", "Нет" }
            }
            button {
                r#type: "button",
                class: if current == Some(true) { "eval-btn eval-btn-yes selected" }
                       else { "eval-btn eval-btn-yes" },
                disabled,
                onclick: move |_| value.set(Some(true)),
                div { class: "eval-btn-icon", "✓" }
                div { class: "eval-btn-label", "Да" }
            }
        }
    }
}

/// 1–5 rating buttons
#[component]
pub fn NumberInput(value: Signal<Option<f64>>, placeholder: String, disabled: bool) -> Element {
    let current = value();
    rsx! {
        div {
            div { class: "label-text gap-sm", style: "margin-bottom:8px;", "Баллы (1–5)" }
            div { class: "num-rating",
                for n in 1..=5_u8 {
                    {
                        let nf = n as f64;
                        let is_sel = current == Some(nf);
                        rsx! {
                            button {
                                r#type: "button",
                                class: if is_sel { "num-btn active" } else { "num-btn" },
                                disabled,
                                onclick: move |_| {
                                    if value() == Some(nf) {
                                        value.set(None);
                                    } else {
                                        value.set(Some(nf));
                                    }
                                },
                                "{n}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Text / textarea input
#[component]
pub fn TextInput(value: Signal<Option<String>>, placeholder: String, disabled: bool) -> Element {
    rsx! {
        textarea {
            class: "input input-textarea",
            placeholder,
            disabled,
            rows: "3",
            value: value().unwrap_or_default(),
            oninput: move |e| {
                let v = e.value();
                value.set(if v.is_empty() { None } else { Some(v) });
            }
        }
    }
}

/// Collapsible optional comment
#[component]
pub fn CommentInput(value: Signal<Option<String>>, disabled: bool) -> Element {
    let mut expanded = use_signal(|| value().is_some());

    rsx! {
        div { style: "margin-top:12px;",
            if expanded() {
                div {
                    label { class: "input-label", "Комментарий" }
                    div { style: "display:flex; gap:8px;",
                        textarea {
                            class: "input",
                            rows: "2",
                            placeholder: "Добавьте комментарий...",
                            disabled,
                            value: value().unwrap_or_default(),
                            oninput: move |e| {
                                let v = e.value();
                                value.set(if v.is_empty() { None } else { Some(v) });
                            }
                        }
                        button {
                            r#type: "button",
                            style: "width:36px; height:36px; flex-shrink:0; background:var(--surface2); border:1px solid var(--border); border-radius:8px; color:var(--text3); cursor:pointer; font-size:14px; font-family:var(--sans); align-self:flex-start;",
                            disabled,
                            onclick: move |_| {
                                value.set(None);
                                expanded.set(false);
                            },
                            "✕"
                        }
                    }
                }
            } else {
                button {
                    r#type: "button",
                    class: "btn-ghost",
                    disabled,
                    onclick: move |_| expanded.set(true),
                    "+ Добавить комментарий"
                }
            }
        }
    }
}
