//! Evaluation types page redesigned around cards and form sheets.

use dioxus::prelude::*;

use crate::api;
use crate::types::{EvaluationItem, EvaluationTypeOption};

use super::shared::{ErrorView, LoadingView};

#[derive(Clone, PartialEq)]
enum View {
    List,
    Create,
    Edit { eval_type: EvaluationTypeOption },
}

fn accent_for_code(code: &str) -> (&'static str, &'static str, &'static str) {
    match code.bytes().next().unwrap_or(0) % 4 {
        0 => ("var(--amber)", "badge badge-amber", "icon-purple"),
        1 => ("var(--blue)", "badge badge-blue", "icon-blue"),
        2 => ("var(--green)", "badge badge-green", "icon-green"),
        _ => ("#c084fc", "badge badge-purple", "icon-purple"),
    }
}

#[component]
pub fn EvaluationTypesPage(token: String, on_back: EventHandler<()>) -> Element {
    let mut view = use_signal(|| View::List);
    let t = token.clone();
    let mut types_data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_evaluation_types(&tok).await }
    });
    let t2 = token.clone();
    let mut evaluations_data = use_resource(move || {
        let tok = t2.clone();
        async move { api::fetch_evaluations(&tok).await }
    });

    match view() {
        View::List => rsx! {
            EvaluationTypesList {
                types: types_data(),
                evaluations: evaluations_data(),
                on_back,
                on_create: move |_| view.set(View::Create),
                on_edit: move |eval_type: EvaluationTypeOption| view.set(View::Edit { eval_type }),
            }
        },
        View::Create => rsx! {
            EvaluationTypeForm {
                token,
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    types_data.restart();
                    evaluations_data.restart();
                    view.set(View::List);
                },
            }
        },
        View::Edit { eval_type } => rsx! {
            EvaluationTypeForm {
                token,
                eval_type,
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    types_data.restart();
                    evaluations_data.restart();
                    view.set(View::List);
                },
            }
        },
    }
}

#[component]
fn EvaluationTypesList(
    types: Option<Result<Vec<EvaluationTypeOption>, String>>,
    evaluations: Option<Result<Vec<EvaluationItem>, String>>,
    on_back: EventHandler<()>,
    on_create: EventHandler<()>,
    on_edit: EventHandler<EvaluationTypeOption>,
) -> Element {
    match types {
        None => rsx! { LoadingView { message: "Загрузка типов замеров...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(types)) => {
            let evaluations = match evaluations {
                Some(Ok(items)) => items,
                _ => Vec::new(),
            };

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        ConfigPageHeader {
                            eyebrow: "Настройки замеров".to_string(),
                            title: "Типы замеров".to_string(),
                            action_label: Some("+"),
                            on_back,
                            on_action: Some(on_create),
                        }

                        div { class: "pad", style: "margin-top: 12px;",
                            p { class: "config-page-intro",
                                "Типы замеров задают контекст проведения замера и помогают структурировать сценарии."
                            }
                        }

                        div { class: "list-section config-list-section",
                            if types.is_empty() {
                                div { class: "empty-state",
                                    div { class: "empty-icon", "🏷" }
                                    p { class: "empty-text", "Нет типов замеров" }
                                    p { class: "caption-text", style: "text-align:center; max-width:220px;",
                                        "Создайте типы вроде ежемесячного, квартального или итогового замера."
                                    }
                                }
                            } else {
                                for eval_type in types {
                                    {
                                        let code = eval_type.code.clone().unwrap_or_else(|| eval_type.name.to_uppercase().replace(' ', "_"));
                                        let eval_count = evaluations.iter().filter(|item| item.evaluation_type_id == eval_type.id).count();
                                        rsx! {
                                            EvaluationTypeCard {
                                                eval_type,
                                                code,
                                                eval_count,
                                                on_edit,
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "pad", style: "margin-top: 4px;",
                            div { class: "config-entry-card dashed", onclick: move |_| on_create.call(()),
                                div { class: "config-icon icon-muted", "+" }
                                div { class: "config-entry-content",
                                    div { class: "config-entry-title", "Создать новый тип" }
                                    div { class: "config-entry-subtitle", "Например: Monthly, Quarterly, Probation" }
                                }
                            }
                        }

                        div { style: "height: 20px;" }
                    }
                }
            }
        }
    }
}

#[component]
fn EvaluationTypeCard(
    eval_type: EvaluationTypeOption,
    code: String,
    eval_count: usize,
    on_edit: EventHandler<EvaluationTypeOption>,
) -> Element {
    let (accent_color, badge_class, _) = accent_for_code(&code);
    let edit_top = eval_type.clone();
    let edit_primary = eval_type.clone();
    let edit_secondary = eval_type.clone();
    rsx! {
        div { class: "type-card",
            div { class: "type-card-top",
                div { class: "type-card-accent", style: "background:{accent_color};" }
                div { class: "type-card-body",
                    div { class: "type-card-title-row",
                        div {
                            div { class: "config-entry-title", "{eval_type.name}" }
                            div { class: "type-card-code", "{code}" }
                        }
                        button { class: "icon-btn btn-icon-sm", onclick: move |_| on_edit.call(edit_top.clone()), "···" }
                    }
                    if let Some(description) = &eval_type.description {
                        p { class: "type-card-description", "{description}" }
                    }
                    div { class: "type-card-badges",
                        span { class: "{badge_class}", "{code}" }
                        span { class: "badge badge-muted", "{eval_count} замеров" }
                    }
                }
            }
            div { class: "set-card-actions",
                button { class: "btn-ghost btn-outline-soft", onclick: move |_| on_edit.call(edit_primary.clone()), "Редактировать" }
                button { class: "btn-ghost btn-outline-soft", onclick: move |_| on_edit.call(edit_secondary.clone()), "Открыть" }
            }
        }
    }
}

#[component]
fn EvaluationTypeForm(
    token: String,
    on_back: EventHandler<()>,
    on_saved: EventHandler<()>,
    eval_type: Option<EvaluationTypeOption>,
) -> Element {
    let is_edit = eval_type.is_some();
    let mut name = use_signal(|| {
        eval_type
            .as_ref()
            .map(|item| item.name.clone())
            .unwrap_or_default()
    });
    let mut code = use_signal(|| {
        eval_type
            .as_ref()
            .and_then(|item| item.code.clone())
            .unwrap_or_default()
    });
    let mut description = use_signal(|| {
        eval_type
            .as_ref()
            .and_then(|item| item.description.clone())
            .unwrap_or_default()
    });
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Типы замеров".to_string(),
                    title: if is_edit { "Редактирование".to_string() } else { "Новый тип замера".to_string() },
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    {error().map(|message| rsx! {
                        div { class: "error-card", role: "alert", style: "margin: 0 0 16px 0;",
                            p { class: "error-text", "{message}" }
                        }
                    })}

                    div { class: "card config-form-card",
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Название" }
                            input {
                                class: "field-input",
                                r#type: "text",
                                placeholder: "Ежемесячный замер",
                                value: "{name()}",
                                oninput: move |e| {
                                    let value = e.value();
                                    name.set(value.clone());
                                    if !is_edit && code().is_empty() {
                                        code.set(value.to_uppercase().replace(' ', "_"));
                                    }
                                },
                            }
                        }
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Код" }
                            input {
                                class: "field-input code-field",
                                r#type: "text",
                                placeholder: "MONTHLY",
                                value: "{code()}",
                                oninput: move |e| code.set(e.value().to_uppercase().replace(' ', "_")),
                            }
                            p { class: "caption-text", style: "margin-top: 5px;", "Используйте заглавные буквы, цифры и символ `_`." }
                        }
                        div { class: "form-field",
                            label { class: "field-label", "Описание" }
                            textarea {
                                class: "field-input field-textarea",
                                placeholder: "Кратко опишите, когда применяется этот тип.",
                                value: "{description()}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                    }
                }

                div { class: "pad", style: "margin-top: 18px; display:flex; gap:10px;",
                    button {
                        class: "btn-ghost btn-outline-soft",
                        style: "flex:1;",
                        onclick: move |_| on_back.call(()),
                        "← Назад"
                    }
                    button {
                        class: "btn-primary",
                        style: "flex:2;",
                        disabled: saving() || name().is_empty() || code().is_empty(),
                        onclick: move |_| {
                            let tok = token.clone();
                            let current_name = name();
                            let current_code = code();
                            let current_description = description();
                            let current_description = if current_description.is_empty() { None } else { Some(current_description) };
                            let current_type = eval_type.clone();
                            saving.set(true);
                            error.set(None);
                            spawn(async move {
                                let result = if let Some(existing) = current_type {
                                    api::update_evaluation_type(
                                        &tok,
                                        existing.id,
                                        Some(&current_name),
                                        Some(&current_code),
                                        current_description.as_deref(),
                                    ).await
                                } else {
                                    api::create_evaluation_type(
                                        &tok,
                                        &current_name,
                                        &current_code,
                                        current_description.as_deref(),
                                    ).await
                                };

                                match result {
                                    Ok(_) => on_saved.call(()),
                                    Err(message) => {
                                        error.set(Some(message));
                                        saving.set(false);
                                    }
                                }
                            });
                        },
                        if saving() { "Сохранение..." } else if is_edit { "Сохранить изменения" } else { "Создать тип" }
                    }
                }

                div { style: "height: 20px;" }
            }
        }
    }
}

#[component]
fn ConfigPageHeader(
    eyebrow: String,
    title: String,
    on_back: EventHandler<()>,
    action_label: Option<&'static str>,
    on_action: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "config-page-header",
            button { class: "icon-btn", onclick: move |_| on_back.call(()), "←" }
            div { class: "config-page-header-copy",
                div { class: "section-overline", "{eyebrow}" }
                div { class: "config-page-title", "{title}" }
            }
            if let (Some(label), Some(action)) = (action_label, on_action) {
                button { class: "icon-btn icon-btn-amber", onclick: move |_| action.call(()), "{label}" }
            }
        }
    }
}
