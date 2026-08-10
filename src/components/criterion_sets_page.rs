//! Criterion sets management with redesigned cards and import flow.

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::api;
use crate::types::{Criterion, CriterionSetOption};

use super::shared::{ErrorView, LoadingView};

#[derive(Clone, PartialEq)]
enum View {
    List,
    Create,
    Edit { set: CriterionSetOption },
    UploadExcel,
    AddGoogleSheet,
    AddGoogleFolder,
}

fn is_google_source(set: &CriterionSetOption) -> bool {
    set.source_type == "google_sheet" || set.source_type == "google_drive_folder"
}

fn criterion_meta(value_type: &str) -> (&'static str, &'static str) {
    match value_type {
        "boolean" => ("type-dot type-bool", "badge badge-green"),
        "number" => ("type-dot type-num", "badge badge-blue"),
        "string" => ("type-dot type-str", "badge badge-purple"),
        _ => ("type-dot type-str", "badge badge-muted"),
    }
}

#[component]
pub fn CriterionSetsPage(
    token: String,
    on_back: EventHandler<()>,
    on_open_ai: EventHandler<()>,
) -> Element {
    let mut view = use_signal(|| View::List);
    let t = token.clone();
    let mut sets_data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_criterion_sets(&tok, true).await }
    });
    let t2 = token.clone();
    let mut criteria_data = use_resource(move || {
        let tok = t2.clone();
        async move { api::fetch_all_criteria(&tok).await }
    });

    match view() {
        View::List => rsx! {
            CriterionSetsList {
                token: token.clone(),
                sets: sets_data(),
                criteria: criteria_data(),
                on_back,
                on_create: move |_| view.set(View::Create),
                on_edit: move |set: CriterionSetOption| view.set(View::Edit { set }),
                on_upload: move |_| view.set(View::UploadExcel),
                on_google_sheet: move |_| view.set(View::AddGoogleSheet),
                on_google_folder: move |_| view.set(View::AddGoogleFolder),
                on_open_ai,
                on_synced: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                },
            }
        },
        View::Create => rsx! {
            CriterionSetForm {
                token,
                criteria: criteria_data(),
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                    view.set(View::List);
                },
            }
        },
        View::Edit { set } => rsx! {
            CriterionSetForm {
                token,
                set,
                criteria: criteria_data(),
                on_back: move |_| view.set(View::List),
                on_saved: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                    view.set(View::List);
                },
            }
        },
        View::UploadExcel => rsx! {
            ExcelUploadView {
                token,
                on_back: move |_| view.set(View::List),
                on_done: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                    view.set(View::List);
                },
            }
        },
        View::AddGoogleSheet => rsx! {
            GoogleSheetView {
                token,
                on_back: move |_| view.set(View::List),
                on_done: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                    view.set(View::List);
                },
            }
        },
        View::AddGoogleFolder => rsx! {
            GoogleFolderView {
                token,
                on_back: move |_| view.set(View::List),
                on_done: move |_| {
                    sets_data.restart();
                    criteria_data.restart();
                    view.set(View::List);
                },
            }
        },
    }
}

#[component]
fn CriterionSetsList(
    token: String,
    sets: Option<Result<Vec<CriterionSetOption>, String>>,
    criteria: Option<Result<Vec<Criterion>, String>>,
    on_back: EventHandler<()>,
    on_create: EventHandler<()>,
    on_edit: EventHandler<CriterionSetOption>,
    on_upload: EventHandler<()>,
    on_google_sheet: EventHandler<()>,
    on_google_folder: EventHandler<()>,
    on_open_ai: EventHandler<()>,
    on_synced: EventHandler<()>,
) -> Element {
    match sets {
        None => rsx! { LoadingView { message: "Загрузка наборов...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(sets)) => {
            let criteria_lookup = match criteria {
                Some(Ok(items)) => items
                    .into_iter()
                    .map(|item| (item.id, item))
                    .collect::<std::collections::HashMap<_, _>>(),
                _ => std::collections::HashMap::new(),
            };

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        ConfigPageHeader {
                            eyebrow: "Настройки замеров".to_string(),
                            title: "Наборы критериев".to_string(),
                            action_label: Some("+"),
                            on_back,
                            on_action: Some(on_create),
                        }

                        div { class: "pad", style: "margin-top: 12px;",
                            div { class: "card info-callout-card",
                                span { class: "info-callout-icon", "💡" }
                                p { class: "info-callout-text",
                                    "Набор по умолчанию подставляется автоматически при создании нового замера."
                                }
                            }
                        }

                        div { class: "list-section config-list-section",
                            if sets.is_empty() {
                                div { class: "empty-state",
                                    div { class: "empty-icon", "📦" }
                                    p { class: "empty-text", "Нет наборов критериев" }
                                    p { class: "caption-text", style: "text-align:center; max-width:240px;",
                                        "Создайте набор вручную или импортируйте структуру из Excel."
                                    }
                                }
                            } else {
                                for set in sets {
                                    CriterionSetCard {
                                        token: token.clone(),
                                        set,
                                        criteria_lookup: criteria_lookup.clone(),
                                        on_edit,
                                        on_synced,
                                    }
                                }
                            }
                        }

                        div { class: "pad", style: "margin-top: 4px;",
                            div { class: "config-entry-card", onclick: move |_| on_open_ai.call(()),
                                div { class: "config-icon icon-amber", "🤖" }
                                div { class: "config-entry-content",
                                    div { class: "config-entry-title", "Создать набор через AI" }
                                    div { class: "config-entry-subtitle", "Откройте чат и сгенерируйте критерии автоматически" }
                                }
                                span { class: "config-entry-chevron", "›" }
                            }
                        }

                        div { class: "pad", style: "margin-top: 4px;",
                            div { class: "config-entry-card", onclick: move |_| on_google_sheet.call(()),
                                div { class: "config-icon icon-green", "📊" }
                                div { class: "config-entry-content",
                                    div { class: "config-entry-title", "Добавить замер из Google Таблицы" }
                                    div { class: "config-entry-subtitle", "Ссылка на таблицу: Блок | Критерий | Тип" }
                                }
                                span { class: "config-entry-chevron", "›" }
                            }
                        }

                        div { class: "pad", style: "margin-top: 4px;",
                            div { class: "config-entry-card", onclick: move |_| on_google_folder.call(()),
                                div { class: "config-icon icon-green", "📁" }
                                div { class: "config-entry-content",
                                    div { class: "config-entry-title", "Замеры из папки Google Drive" }
                                    div { class: "config-entry-subtitle", "Выберите таблицу из расшаренной папки" }
                                }
                                span { class: "config-entry-chevron", "›" }
                            }
                        }

                        div { class: "pad", style: "margin-top: 4px;",
                            div { class: "config-entry-card dashed", onclick: move |_| on_upload.call(()),
                                div { class: "config-icon icon-muted", "📤" }
                                div { class: "config-entry-content",
                                    div { class: "config-entry-title", "Импорт из Excel" }
                                    div { class: "config-entry-subtitle", "Загрузить наборы и критерии из .xlsx файла" }
                                }
                                span { class: "config-entry-chevron", "›" }
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
fn CriterionSetCard(
    token: String,
    set: CriterionSetOption,
    criteria_lookup: std::collections::HashMap<i64, Criterion>,
    on_edit: EventHandler<CriterionSetOption>,
    on_synced: EventHandler<()>,
) -> Element {
    let criterion_ids = set.criterion_ids.clone().unwrap_or_default();
    let criterion_items: Vec<Criterion> = criterion_ids
        .iter()
        .filter_map(|id| criteria_lookup.get(id).cloned())
        .collect();

    let edit_top = set.clone();
    let edit_bottom = set.clone();
    let google = is_google_source(&set);
    let mut syncing = use_signal(|| false);
    let mut sync_msg = use_signal(|| Option::<String>::None);
    let set_id = set.id;

    rsx! {
        div { class: if set.is_default { "set-card set-card-default" } else { "set-card" },
            div { class: "set-card-top",
                div { class: if set.is_default { "config-icon icon-amber" } else { "config-icon icon-muted" }, "📦" }
                div { class: "set-card-copy",
                    div { class: "set-card-title-row",
                        div { class: "config-entry-title", "{set.name}" }
                        if set.is_default {
                            span { class: "badge badge-amber", "По умолч." }
                        }
                        if google {
                            span { class: "badge badge-green", "Google" }
                        }
                    }
                    div { class: "config-entry-subtitle",
                        "{criterion_items.len()} критериев"
                        if google {
                            " · обновляется из таблицы"
                        }
                    }
                }
                button { class: "icon-btn btn-icon-sm", onclick: move |_| on_edit.call(edit_top.clone()), "···" }
            }

            if !criterion_items.is_empty() {
                div { class: "set-pill-grid",
                    for criterion in criterion_items.iter().take(6) {
                        {
                            let (dot_class, _) = criterion_meta(&criterion.value_type);
                            rsx! {
                                span { class: "criterion-pill",
                                    span { class: "{dot_class}" }
                                    "{criterion.name}"
                                }
                            }
                        }
                    }
                }
            }

            div { class: "set-card-actions",
                if google {
                    button {
                        class: "btn-ghost btn-outline-soft",
                        disabled: syncing(),
                        onclick: move |_| {
                            let tok = token.clone();
                            syncing.set(true);
                            spawn(async move {
                                match api::sync_google_criterion_set(&tok, set_id).await {
                                    Ok(_) => {
                                        on_synced.call(());
                                    }
                                    Err(e) => sync_msg.set(Some(e)),
                                }
                                syncing.set(false);
                            });
                        },
                        if syncing() { "Обновление..." } else { "↻ Обновить из Google" }
                    }
                }
                button { class: "btn-ghost btn-outline-soft", onclick: move |_| on_edit.call(edit_bottom.clone()), "Редактировать" }
            }
            if let Some(msg) = sync_msg() {
                div { class: "error-msg", style: "margin-top:8px; font-size:12px;", "{msg}" }
            }
        }
    }
}

#[component]
fn CriterionSetForm(
    token: String,
    criteria: Option<Result<Vec<Criterion>, String>>,
    on_back: EventHandler<()>,
    on_saved: EventHandler<()>,
    set: Option<CriterionSetOption>,
) -> Element {
    let is_edit = set.is_some();
    let mut name = use_signal(|| {
        set.as_ref()
            .map(|item| item.name.clone())
            .unwrap_or_default()
    });
    let mut description = use_signal(|| {
        set.as_ref()
            .and_then(|item| item.description.clone())
            .unwrap_or_default()
    });
    let mut is_default = use_signal(|| set.as_ref().map(|item| item.is_default).unwrap_or(false));
    let mut selected_ids = use_signal(|| {
        set.as_ref()
            .and_then(|item| item.criterion_ids.clone())
            .unwrap_or_default()
    });
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let criteria_list = match criteria {
        Some(Ok(items)) => items,
        Some(Err(e)) => {
            return rsx! { ErrorView { message: e } };
        }
        None => {
            return rsx! { LoadingView { message: "Загрузка критериев...".to_string() } };
        }
    };

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Наборы критериев".to_string(),
                    title: if is_edit { "Редактирование".to_string() } else { "Новый набор".to_string() },
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    {error().map(|message| rsx! {
                        div { class: "error-card", style: "margin: 0 0 16px 0;",
                            p { class: "error-text", "{message}" }
                        }
                    })}

                    div { class: "card config-form-card",
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Название набора" }
                            input {
                                class: "field-input",
                                r#type: "text",
                                placeholder: "Например: Аттестация 2024",
                                value: "{name()}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "form-field", style: "margin-bottom: 16px;",
                            label { class: "field-label", "Описание" }
                            textarea {
                                class: "field-input field-textarea",
                                placeholder: "Когда использовать этот набор",
                                value: "{description()}",
                                oninput: move |e| description.set(e.value()),
                            }
                        }
                        label { class: "checkbox-line", style: "margin-bottom: 4px;",
                            input {
                                r#type: "checkbox",
                                checked: is_default(),
                                onchange: move |e| is_default.set(e.checked()),
                            }
                            span { "Использовать как набор по умолчанию" }
                        }
                    }
                }

                div { class: "pad", style: "margin-top: 14px;",
                    div { class: "card config-form-card",
                        div { class: "section-overline", "Критерии в наборе" }
                        div { class: "criteria-checkbox-list",
                            for criterion in criteria_list {
                                CriterionSelectorRow {
                                    criterion: criterion.clone(),
                                    checked: selected_ids.read().contains(&criterion.id),
                                    on_toggle: move |criterion_id| {
                                        let mut ids = selected_ids();
                                        if let Some(index) = ids.iter().position(|id| *id == criterion_id) {
                                            ids.remove(index);
                                        } else {
                                            ids.push(criterion_id);
                                        }
                                        selected_ids.set(ids);
                                    },
                                }
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
                        disabled: saving() || name().is_empty() || selected_ids.read().is_empty(),
                        onclick: move |_| {
                            let tok = token.clone();
                            let current_name = name();
                            let current_description = description();
                            let current_description = if current_description.is_empty() {
                                None
                            } else {
                                Some(current_description)
                            };
                            let current_ids = selected_ids();
                            let current_default = is_default();
                            let current_set = set.clone();
                            saving.set(true);
                            error.set(None);
                            spawn(async move {
                                let result = if let Some(existing) = current_set {
                                    api::update_criterion_set(
                                        &tok,
                                        existing.id,
                                        Some(&current_name),
                                        current_description.as_deref(),
                                        Some(current_default),
                                        Some(&current_ids),
                                    ).await
                                } else {
                                    api::create_criterion_set(
                                        &tok,
                                        &current_name,
                                        current_description.as_deref(),
                                        current_default,
                                        &current_ids,
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
                        if saving() { "Сохранение..." } else if is_edit { "Сохранить набор" } else { "Создать набор" }
                    }
                }

                div { style: "height: 20px;" }
            }
        }
    }
}

#[component]
fn CriterionSelectorRow(
    criterion: Criterion,
    checked: bool,
    on_toggle: EventHandler<i64>,
) -> Element {
    let (dot_class, badge_class) = criterion_meta(&criterion.value_type);
    let label = match criterion.value_type.as_str() {
        "boolean" => "Да / Нет",
        "number" => "Число",
        "string" => "Текст",
        _ => "Другое",
    };

    rsx! {
        label { class: if checked { "criterion-selector checked" } else { "criterion-selector" },
            input {
                r#type: "checkbox",
                checked,
                onchange: move |_| on_toggle.call(criterion.id),
            }
            div { class: "config-item-content",
                div { class: "config-item-main",
                    span { class: "{dot_class}" }
                    span { class: "config-item-title", "{criterion.name}" }
                }
                div { class: "config-item-subtitle",
                    "{criterion.description.clone().unwrap_or_else(|| criterion.code.clone())}"
                }
            }
            span { class: "{badge_class}", "{label}" }
        }
    }
}

#[component]
fn GoogleSheetView(token: String, on_back: EventHandler<()>, on_done: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut preview = use_signal(|| None::<crate::types::GoogleSheetPreviewResponse>);
    let mut loading = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Наборы критериев".to_string(),
                    title: "Google Таблица".to_string(),
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    div { class: "card config-form-card",
                        div { class: "section-overline", "Формат таблицы" }
                        p { class: "body-text", "3 столбца: Блок | Критерий | Тип (да/нет или бальная система)" }
                        p { class: "caption-text", style: "margin-top:8px;",
                            "Таблица должна быть доступна по ссылке (просмотр). Изменения в таблице подхватываются при каждом старте замера."
                        }
                    }

                    if let Some(err) = error() {
                        div { class: "error-card", style: "margin-top:12px;",
                            p { class: "error-text", "{err}" }
                        }
                    }

                    div { class: "card config-form-card", style: "margin-top:12px;",
                        div { class: "form-field", style: "margin-bottom: 14px;",
                            label { class: "field-label", "Название замера" }
                            input {
                                class: "field-input",
                                placeholder: "Например: Аттестация официантов",
                                value: "{name()}",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "form-field", style: "margin-bottom: 14px;",
                            label { class: "field-label", "Ссылка на Google Таблицу" }
                            input {
                                class: "field-input",
                                r#type: "url",
                                placeholder: "https://docs.google.com/spreadsheets/d/...",
                                value: "{url()}",
                                oninput: move |e| url.set(e.value()),
                            }
                        }
                        button {
                            class: "btn-secondary w-full",
                            disabled: loading() || url().trim().is_empty(),
                            onclick: move |_| {
                                error.set(None);
                                preview.set(None);
                                let tok = token.clone();
                                let u = url().trim().to_string();
                                loading.set(true);
                                spawn(async move {
                                    match api::preview_google_criterion_set(&tok, &u).await {
                                        Ok(p) => preview.set(Some(p)),
                                        Err(e) => error.set(Some(e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            if loading() { "Проверка..." } else { "Проверить таблицу" }
                        }
                    }

                    if let Some(p) = preview() {
                        div { class: "card", style: "margin-top:12px;",
                            div { class: "heading-md", style: "margin-bottom:8px;",
                                "Найдено: {p.rows_count} критериев"
                            }
                            if !p.blocks.is_empty() {
                                div { class: "caption-text", style: "margin-bottom:10px;",
                                    "Блоки: {p.blocks.join(\", \")}"
                                }
                            }
                            for row in p.sample_rows.iter().take(8) {
                                div { style: "font-size:13px; padding:6px 0; border-bottom:1px solid rgba(255,255,255,0.06);",
                                    span { style: "color:var(--text3);", "[{row.block}] " }
                                    "{row.criterion} "
                                    span { class: "badge badge-muted", "{row.value_type}" }
                                }
                            }
                        }
                        {
                            let tok_save = token.clone();
                            rsx! {
                                button {
                                    class: "btn-primary w-full",
                                    style: "margin-top:16px;",
                                    disabled: saving() || name().trim().is_empty(),
                                    onclick: move |_| {
                                        error.set(None);
                                        let tok = tok_save.clone();
                                        let n = name().trim().to_string();
                                        let u = url().trim().to_string();
                                        saving.set(true);
                                        spawn(async move {
                                            match api::create_google_criterion_set(&tok, &n, &u).await {
                                                Ok(_) => on_done.call(()),
                                                Err(e) => error.set(Some(e)),
                                            }
                                            saving.set(false);
                                        });
                                    },
                                    if saving() { "Сохранение..." } else { "Сохранить замер" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum FolderStep {
    EnterUrl,
    PickFile,
}

#[component]
fn GoogleFolderView(
    token: String,
    on_back: EventHandler<()>,
    on_done: EventHandler<()>,
) -> Element {
    let mut step = use_signal(|| FolderStep::EnterUrl);
    let mut folder_url = use_signal(String::new);
    let mut files = use_signal(|| Vec::<crate::types::GoogleDriveFileItem>::new());
    let mut selected_file = use_signal(|| Option::<crate::types::GoogleDriveFileItem>::None);
    let mut name = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Наборы критериев".to_string(),
                    title: "Папка Google Drive".to_string(),
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    div { class: "card config-form-card",
                        p { class: "body-text", "Укажите ссылку на папку с таблицами. Папка должна быть расшарена на сервисный аккаунт (см. настройки сервера)." }
                    }

                    if let Some(err) = error() {
                        div { class: "error-card", style: "margin-top:12px;",
                            p { class: "error-text", "{err}" }
                        }
                    }

                    match step() {
                        FolderStep::EnterUrl => rsx! {
                            div { class: "card config-form-card", style: "margin-top:12px;",
                                div { class: "form-field",
                                    label { class: "field-label", "Ссылка на папку Google Drive" }
                                    input {
                                        class: "field-input",
                                        r#type: "url",
                                        placeholder: "https://drive.google.com/drive/folders/...",
                                        value: "{folder_url()}",
                                        oninput: move |e| folder_url.set(e.value()),
                                    }
                                }
                                button {
                                    class: "btn-primary w-full",
                                    style: "margin-top:12px;",
                                    disabled: loading() || folder_url().trim().is_empty(),
                                    onclick: move |_| {
                                        error.set(None);
                                        let tok = token.clone();
                                        let u = folder_url().trim().to_string();
                                        loading.set(true);
                                        spawn(async move {
                                            match api::browse_google_drive_folder(&tok, &u).await {
                                                Ok(resp) => {
                                                    files.set(resp.files);
                                                    step.set(FolderStep::PickFile);
                                                }
                                                Err(e) => error.set(Some(e)),
                                            }
                                            loading.set(false);
                                        });
                                    },
                                    if loading() { "Загрузка..." } else { "Показать таблицы в папке" }
                                }
                            }
                        },
                        FolderStep::PickFile => rsx! {
                            div { style: "margin-top:12px;",
                                if files().is_empty() {
                                    div { class: "empty-state",
                                        p { class: "empty-text", "В папке нет Google Таблиц" }
                                    }
                                } else {
                                    for file in files().iter().cloned() {
                                        {
                                            let f = file.clone();
                                            let selected = selected_file()
                                                .as_ref()
                                                .map(|s| s.file_id == f.file_id)
                                                .unwrap_or(false);
                                            rsx! {
                                                div {
                                                    class: if selected { "card config-entry-card" } else { "card config-entry-card dashed" },
                                                    style: "margin-bottom:8px; cursor:pointer;",
                                                    onclick: move |_| {
                                                        selected_file.set(Some(f.clone()));
                                                        if name().trim().is_empty() {
                                                            name.set(f.name.clone());
                                                        }
                                                    },
                                                    div { class: "config-entry-title", "{file.name}" }
                                                    if let Some(t) = file.modified_time.as_ref() {
                                                        div { class: "caption-text", "{t}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if selected_file().is_some() {
                                    div { class: "card config-form-card", style: "margin-top:12px;",
                                        div { class: "form-field",
                                            label { class: "field-label", "Название замера" }
                                            input {
                                                class: "field-input",
                                                value: "{name()}",
                                                oninput: move |e| name.set(e.value()),
                                            }
                                        }
                                        button {
                                            class: "btn-primary w-full",
                                            style: "margin-top:12px;",
                                            disabled: saving() || name().trim().is_empty(),
                                            onclick: move |_| {
                                                let Some(file) = selected_file() else { return };
                                                error.set(None);
                                                let tok = token.clone();
                                                let folder = folder_url().trim().to_string();
                                                let n = name().trim().to_string();
                                                let fid = file.file_id.clone();
                                                saving.set(true);
                                                spawn(async move {
                                                    match api::create_google_criterion_set_from_folder(
                                                        &tok, &folder, &fid, &n,
                                                    ).await {
                                                        Ok(_) => on_done.call(()),
                                                        Err(e) => error.set(Some(e)),
                                                    }
                                                    saving.set(false);
                                                });
                                            },
                                            if saving() { "Сохранение..." } else { "Сохранить замер" }
                                        }
                                    }
                                }
                                button {
                                    class: "btn-ghost w-full",
                                    style: "margin-top:12px;",
                                    onclick: move |_| step.set(FolderStep::EnterUrl),
                                    "← Другая папка"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn ExcelUploadView(token: String, on_back: EventHandler<()>, on_done: EventHandler<()>) -> Element {
    let mut uploading = use_signal(|| false);
    let mut result = use_signal(|| Option::<(bool, String)>::None);
    const INPUT_ID: &str = "excel-file-input";

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                ConfigPageHeader {
                    eyebrow: "Наборы критериев".to_string(),
                    title: "Импорт из Excel".to_string(),
                    on_back,
                    action_label: None,
                    on_action: None,
                }

                div { class: "pad", style: "margin-top: 12px;",
                    div { class: "card config-form-card",
                        div { class: "section-overline", "Формат файла" }
                        p { class: "body-text", "4 колонки: объединённый набор | субнабор | вопрос | тип" }
                        ul { class: "excel-rules-list",
                            li { "Колонка 1: название объединённого набора" }
                            li { "Колонка 2: название субнабора" }
                            li { "Колонка 3: вопрос критерия" }
                            li { "Колонка 4: тип значения (bool, str, num)" }
                        }
                    }
                }

                div { class: "pad", style: "margin-top: 16px;",
                    input {
                        id: INPUT_ID,
                        r#type: "file",
                        accept: ".xlsx,.xls",
                        style: "display:none;",
                        onchange: move |_| {
                            if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
                                if let Some(el) = doc.get_element_by_id(INPUT_ID) {
                                    if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                                        if let Some(files) = input.files() {
                                            if let Some(file) = files.get(0) {
                                                let tok = token.clone();
                                                uploading.set(true);
                                                result.set(None);
                                                spawn(async move {
                                                    let response = api::upload_excel_criterion_sets(&tok, file).await;
                                                    match response {
                                                        Ok(data) => result.set(Some((
                                                            true,
                                                            format!("Создано {} наборов и {} критериев", data.created_sets, data.created_criteria),
                                                        ))),
                                                        Err(message) => result.set(Some((false, message))),
                                                    }
                                                    uploading.set(false);
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    }

                    button {
                        class: "btn-primary",
                        disabled: uploading(),
                        onclick: move |_| {
                            if let Some(doc) = web_sys::window().and_then(|window| window.document()) {
                                if let Some(el) = doc.get_element_by_id(INPUT_ID) {
                                    if let Some(input) = el.dyn_ref::<web_sys::HtmlInputElement>() {
                                        input.click();
                                    }
                                }
                            }
                        },
                        if uploading() { "Загрузка..." } else { "Выбрать файл" }
                    }

                    {result().map(|(ok, message)| rsx! {
                        div { class: if ok { "card card-green upload-result-card" } else { "error-card upload-result-card" },
                            p { class: if ok { "body-text" } else { "error-text" }, "{message}" }
                            if ok {
                                button {
                                    class: "btn-secondary",
                                    style: "margin-top: 12px;",
                                    onclick: move |_| on_done.call(()),
                                    "Готово"
                                }
                            }
                        }
                    })}
                }
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
