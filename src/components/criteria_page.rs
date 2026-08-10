//! Criteria management page with redesigned list and form.

use dioxus::prelude::*;

use crate::api;
use crate::types::Criterion;

use super::shared::{ErrorView, LoadingView};

#[derive(Clone, PartialEq)]
enum View {
    List,
    Create,
    Edit { criterion: Criterion },
}

#[derive(Clone, Copy, PartialEq)]
enum CriterionFilter {
    All,
    Boolean,
    Number,
    Text,
}

fn criterion_meta(value_type: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    match value_type {
        "boolean" => ("Да / Нет", "type-dot type-bool", "badge badge-green", "✓"),
        "number" => ("Число", "type-dot type-num", "badge badge-blue", "🔢"),
        "string" => ("Текст", "type-dot type-str", "badge badge-purple", "📝"),
        _ => ("Другое", "type-dot type-str", "badge badge-muted", "•"),
    }
}

#[component]
pub fn CriteriaPage(token: String, on_back: EventHandler<()>) -> Element {
    let mut view = use_signal(|| View::List);
    let t = token.clone();
    let mut data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_all_criteria(&tok).await }
    });

    match view() {
        View::List => rsx! {
            CriteriaList {
                data: data(),
                on_back,
                on_create: move |_| view.set(View::Create),
                on_edit: move |criterion: Criterion| view.set(View::Edit { criterion }),
            }
        },
        View::Create => rsx! {
            CriterionForm {
                token,
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    data.restart();
                    view.set(View::List);
                },
            }
        },
        View::Edit { criterion } => rsx! {
            CriterionForm {
                token,
                criterion,
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    data.restart();
                    view.set(View::List);
                },
            }
        },
    }
}

#[component]
fn CriteriaList(
    data: Option<Result<Vec<Criterion>, String>>,
    on_back: EventHandler<()>,
    on_create: EventHandler<()>,
    on_edit: EventHandler<Criterion>,
) -> Element {
    let mut search = use_signal(String::new);
    let mut filter = use_signal(|| CriterionFilter::All);

    match data {
        None => rsx! { LoadingView { message: "Загрузка критериев...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(criteria)) => {
            let query = search().to_lowercase();
            let filtered: Vec<Criterion> = criteria
                .iter()
                .filter(|criterion| {
                    let matches_filter = match filter() {
                        CriterionFilter::All => true,
                        CriterionFilter::Boolean => criterion.value_type == "boolean",
                        CriterionFilter::Number => criterion.value_type == "number",
                        CriterionFilter::Text => criterion.value_type == "string",
                    };
                    let matches_query = query.is_empty()
                        || criterion.name.to_lowercase().contains(&query)
                        || criterion
                            .description
                            .as_deref()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(&query)
                        || criterion.code.to_lowercase().contains(&query);
                    matches_filter && matches_query
                })
                .cloned()
                .collect();

            let bool_count = criteria
                .iter()
                .filter(|item| item.value_type == "boolean")
                .count();
            let num_count = criteria
                .iter()
                .filter(|item| item.value_type == "number")
                .count();
            let text_count = criteria
                .iter()
                .filter(|item| item.value_type == "string")
                .count();

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        ConfigPageHeader {
                            eyebrow: "Настройки замеров".to_string(),
                            title: "Критерии".to_string(),
                            action_label: Some("+"),
                            on_back,
                            on_action: Some(on_create),
                        }

                        div { class: "pad config-search-wrap",
                            div { class: "search-shell",
                                span { class: "search-icon", "🔍" }
                                input {
                                    class: "field-input search-input",
                                    r#type: "text",
                                    value: "{search()}",
                                    placeholder: "Поиск критериев...",
                                    oninput: move |e| search.set(e.value()),
                                }
                            }
                        }

                        div { class: "filter-row filter-row-tight",
                            FilterChip {
                                active: filter() == CriterionFilter::All,
                                label: format!("Все · {}", criteria.len()),
                                on_click: move |_| filter.set(CriterionFilter::All),
                            }
                            FilterChip {
                                active: filter() == CriterionFilter::Boolean,
                                label: format!("Да/Нет · {}", bool_count),
                                dot_class: Some("type-dot type-bool"),
                                on_click: move |_| filter.set(CriterionFilter::Boolean),
                            }
                            FilterChip {
                                active: filter() == CriterionFilter::Number,
                                label: format!("Число · {}", num_count),
                                dot_class: Some("type-dot type-num"),
                                on_click: move |_| filter.set(CriterionFilter::Number),
                            }
                            FilterChip {
                                active: filter() == CriterionFilter::Text,
                                label: format!("Текст · {}", text_count),
                                dot_class: Some("type-dot type-str"),
                                on_click: move |_| filter.set(CriterionFilter::Text),
                            }
                        }

                        div { class: "list-section config-list-section",
                            if filtered.is_empty() {
                                div { class: "empty-state",
                                    div { class: "empty-icon", "📋" }
                                    p { class: "empty-text",
                                        if criteria.is_empty() { "Нет критериев" } else { "Ничего не найдено" }
                                    }
                                    p { class: "caption-text", style: "text-align:center; max-width:220px;",
                                        if criteria.is_empty() {
                                            "Создайте критерий, чтобы затем использовать его в наборах и замерах"
                                        } else {
                                            "Попробуйте изменить строку поиска или сбросить фильтр"
                                        }
                                    }
                                }
                            } else {
                                for criterion in filtered {
                                    CriterionRow {
                                        criterion,
                                        on_edit,
                                    }
                                }
                            }
                        }

                        div { class: "pad", style: "margin-top: 16px;",
                            div { class: "card config-legend-card",
                                div { class: "section-overline", "Типы значений" }
                                div { class: "legend-row" ,
                                    LegendItem { dot_class: "type-dot type-bool", label: "Да / Нет" }
                                    LegendItem { dot_class: "type-dot type-num", label: "Числовое" }
                                    LegendItem { dot_class: "type-dot type-str", label: "Текстовое" }
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
fn CriterionRow(criterion: Criterion, on_edit: EventHandler<Criterion>) -> Element {
    let (label, dot_class, badge_class, _) = criterion_meta(&criterion.value_type);
    let description = criterion
        .description
        .clone()
        .unwrap_or_else(|| format!("Код: {}", criterion.code));

    rsx! {
        div { class: "config-item-card", onclick: move |_| on_edit.call(criterion.clone()),
            div { class: "drag-handle-ui", span {}, span {}, span {} }
            div { class: "config-item-content",
                div { class: "config-item-main",
                    span { class: "{dot_class}" }
                    span { class: "config-item-title", "{criterion.name}" }
                }
                div { class: "config-item-subtitle", "{description}" }
            }
            div { class: "config-item-side",
                span { class: "{badge_class}", "{label}" }
                button { class: "icon-btn btn-icon-sm", "···" }
            }
        }
    }
}

#[component]
fn CriterionForm(
    token: String,
    on_back: EventHandler<()>,
    on_saved: EventHandler<()>,
    criterion: Option<Criterion>,
) -> Element {
    let is_edit = criterion.is_some();
    let mut name = use_signal(|| {
        criterion
            .as_ref()
            .map(|c| c.name.clone())
            .unwrap_or_default()
    });
    let mut code = use_signal(|| {
        criterion
            .as_ref()
            .map(|c| c.code.clone())
            .unwrap_or_default()
    });
    let mut description = use_signal(|| {
        criterion
            .as_ref()
            .and_then(|c| c.description.clone())
            .unwrap_or_default()
    });
    let mut value_type = use_signal(|| {
        criterion
            .as_ref()
            .map(|c| c.value_type.clone())
            .unwrap_or_else(|| "boolean".to_string())
    });
    let mut is_required = use_signal(|| criterion.as_ref().map(|c| c.is_required).unwrap_or(true));
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Критерии".to_string(),
                    title: if is_edit { "Редактирование".to_string() } else { "Новый критерий".to_string() },
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    div { class: "card config-step-card",
                        div { class: "section-overline", "Тип значения" }
                        div { class: "type-choice-grid",
                            TypeChoiceCard {
                                active: value_type() == "boolean",
                                icon: "✓",
                                label: "Да / Нет",
                                tone_class: "type-choice-green",
                                on_click: move |_| value_type.set("boolean".to_string()),
                            }
                            TypeChoiceCard {
                                active: value_type() == "number",
                                icon: "🔢",
                                label: "Число",
                                tone_class: "type-choice-blue",
                                on_click: move |_| value_type.set("number".to_string()),
                            }
                            TypeChoiceCard {
                                active: value_type() == "string",
                                icon: "📝",
                                label: "Текст",
                                tone_class: "type-choice-purple",
                                on_click: move |_| value_type.set("string".to_string()),
                            }
                        }
                    }
                }

                div { class: "pad", style: "margin-top: 14px;",
                    {error().map(|message| rsx! {
                        div { class: "error-card", style: "margin: 0 0 16px 0;",
                            p { class: "error-text", "{message}" }
                        }
                    })}

                    div { class: "card config-form-card",
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Название" }
                            input {
                                class: "field-input",
                                r#type: "text",
                                placeholder: "Например: Пунктуальность",
                                value: "{name()}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Код" }
                            input {
                                class: "field-input",
                                r#type: "text",
                                placeholder: "punctuality",
                                value: "{code()}",
                                disabled: is_edit,
                                oninput: move |e| code.set(e.value()),
                            }
                            if is_edit {
                                p { class: "caption-text", style: "margin-top: 5px;", "Код зафиксирован после создания." }
                            }
                        }
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Описание" }
                            textarea {
                                class: "field-input field-textarea",
                                placeholder: "Краткое пояснение для оценщика",
                                value: "{description()}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                        label { class: "checkbox-line",
                            input {
                                r#type: "checkbox",
                                checked: is_required(),
                                onchange: move |e| is_required.set(e.checked()),
                            }
                            span { "Обязательный критерий" }
                        }
                    }
                }

                div { class: "pad", style: "margin-top: 18px; display:flex; gap:10px;" ,
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
                            let current_desc = description();
                            let current_desc = if current_desc.is_empty() { None } else { Some(current_desc) };
                            let current_type = value_type();
                            let required = is_required();
                            let current_criterion = criterion.clone();
                            saving.set(true);
                            error.set(None);
                            spawn(async move {
                                let result = if let Some(existing) = current_criterion {
                                    api::update_criterion(
                                        &tok,
                                        existing.id,
                                        Some(&current_name),
                                        None,
                                        current_desc.as_deref(),
                                        Some(&current_type),
                                        Some(required),
                                    ).await
                                } else {
                                    api::create_criterion(
                                        &tok,
                                        &current_name,
                                        &current_code,
                                        current_desc.as_deref(),
                                        &current_type,
                                        required,
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
                        if saving() { "Сохранение..." } else if is_edit { "Сохранить" } else { "Создать критерий" }
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

#[component]
fn FilterChip(
    label: String,
    active: bool,
    on_click: EventHandler<()>,
    dot_class: Option<&'static str>,
) -> Element {
    rsx! {
        button {
            class: if active { "chip active filter-chip" } else { "chip filter-chip" },
            onclick: move |_| on_click.call(()),
            if let Some(dot) = dot_class {
                span { class: "{dot}" }
            }
            "{label}"
        }
    }
}

#[component]
fn LegendItem(dot_class: &'static str, label: &'static str) -> Element {
    rsx! {
        div { class: "legend-item",
            span { class: "{dot_class}" }
            span { "{label}" }
        }
    }
}

#[component]
fn TypeChoiceCard(
    active: bool,
    icon: &'static str,
    label: &'static str,
    tone_class: &'static str,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: if active {
                "type-choice-card active"
            } else {
                "type-choice-card"
            },
            onclick: move |_| on_click.call(()),
            div { class: "type-choice-icon {tone_class}", "{icon}" }
            div { class: "type-choice-label", "{label}" }
        }
    }
}
