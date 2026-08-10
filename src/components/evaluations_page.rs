//! Evaluations overview with config entry cards.

use dioxus::prelude::*;

use serde_json::Value as JsonValue;

use crate::api;
use crate::types::{EvaluationDetail, EvaluationItem};

use super::shared::{ErrorView, LoadingView};

fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    match parts.as_slice() {
        [] => "?".to_string(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [a, b, ..] => format!(
            "{}{}",
            a.chars().next().unwrap_or('?'),
            b.chars().next().unwrap_or('?')
        )
        .to_uppercase(),
    }
}

fn av_color(name: &str) -> &'static str {
    match name.bytes().next().unwrap_or(0) % 4 {
        0 => "av-amber",
        1 => "av-blue",
        2 => "av-green",
        _ => "av-purple",
    }
}

fn score_color(s: f64) -> &'static str {
    if s >= 80.0 {
        "var(--green)"
    } else if s >= 60.0 {
        "var(--amber)"
    } else {
        "var(--red)"
    }
}

fn format_criterion_value(value_type: &str, value: &Option<JsonValue>) -> String {
    let Some(v) = value else {
        return "—".to_string();
    };
    match value_type {
        "boolean" => match v {
            JsonValue::Bool(true) => "Да".to_string(),
            JsonValue::Bool(false) => "Нет".to_string(),
            _ => v.to_string(),
        },
        "number" | "integer" | "float" => v
            .as_f64()
            .map(|n| format!("{}", n))
            .or_else(|| v.as_i64().map(|n| n.to_string()))
            .unwrap_or_else(|| v.to_string()),
        _ => match v.as_str() {
            Some(s) => s.to_string(),
            None if v.is_null() => "—".to_string(),
            None => v.to_string(),
        },
    }
}

#[component]
pub fn EvaluationsPage(
    token: String,
    on_start_eval: EventHandler<i64>,
    on_open_criteria: EventHandler<()>,
    on_open_sets: EventHandler<()>,
    on_open_types: EventHandler<()>,
    on_open_ai: EventHandler<()>,
) -> Element {
    let mut selected_id = use_signal(|| None::<i64>);

    let t = token.clone();
    let data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_evaluations(&tok).await }
    });
    let t2 = token.clone();
    let criteria = use_resource(move || {
        let tok = t2.clone();
        async move { api::fetch_all_criteria(&tok).await }
    });
    let t3 = token.clone();
    let sets = use_resource(move || {
        let tok = t3.clone();
        async move { api::fetch_criterion_sets(&tok, true).await }
    });
    let t4 = token.clone();
    let types = use_resource(move || {
        let tok = t4.clone();
        async move { api::fetch_evaluation_types(&tok).await }
    });

    let mut filter = use_signal(|| "all".to_string());

    match data() {
        None => rsx! { LoadingView { message: "Загрузка замеров...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(evals)) => {
            let filter_val = filter.read().clone();
            let criteria_count = match criteria() {
                Some(Ok(items)) => items.len(),
                _ => 0,
            };
            let set_items = match sets() {
                Some(Ok(items)) => items,
                _ => Vec::new(),
            };
            let sets_count = set_items.len();
            let default_sets = set_items.iter().filter(|set| set.is_default).count();
            let types_items = match types() {
                Some(Ok(items)) => items,
                _ => Vec::new(),
            };
            let types_count = types_items.len();
            let filtered: Vec<&EvaluationItem> = evals
                .iter()
                .filter(|ev| match filter_val.as_str() {
                    "completed" => ev.score_percentage.is_some(),
                    "in_progress" => ev.score_percentage.is_none(),
                    _ => true,
                })
                .collect();

            if let Some(eid) = selected_id() {
                return rsx! {
                    EvaluationDetailView {
                        token: token.clone(),
                        evaluation_id: eid,
                        on_back: move |_| selected_id.set(None),
                        on_continue: move |rid: i64| {
                            selected_id.set(None);
                            on_start_eval.call(rid);
                        },
                    }
                };
            }

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        div { class: "page-header page-header-spacious",
                            div {
                                div { class: "label-text", "Ваши замеры" }
                                div { class: "page-title page-title-lg", "Замеры" }
                            }
                            div { class: "header-actions",
                                button {
                                    class: "icon-btn icon-btn-amber",
                                    onclick: move |_| on_open_criteria.call(()),
                                    title: "Настройки замеров",
                                    "⚙"
                                }
                                button {
                                    class: "icon-btn",
                                    onclick: move |_| on_open_ai.call(()),
                                    title: "AI ассистент",
                                    "🤖"
                                }
                                button {
                                    class: "icon-btn icon-btn-solid",
                                    onclick: move |_| on_start_eval.call(0),
                                    title: "Новый замер",
                                    "+"
                                }
                            }
                        }

                        div { class: "filter-row",
                            for (val, lbl) in [("all","Все"), ("completed","Завершённые"), ("in_progress","Черновики")] {
                                {
                                    let v = val.to_string();
                                    let active = filter_val == val;
                                    let chip_label = if val == "all" {
                                        format!("{} · {}", lbl, evals.len())
                                    } else {
                                        lbl.to_string()
                                    };
                                    rsx! {
                                        button {
                                            class: if active { "chip active" } else { "chip" },
                                            onclick: move |_| filter.set(v.clone()),
                                            "{chip_label}"
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "list-section",
                            if filtered.is_empty() {
                                div { class: "empty-state",
                                    div { class: "empty-icon", "⭐" }
                                    p { class: "empty-text", "Нет замеров" }
                                    p { class: "caption-text", style: "max-width:220px; text-align:center;", "Создайте первый замер, чтобы начать отслеживать прогресс команды" }
                                    button {
                                        class: "btn-primary",
                                        style: "margin-top:16px;",
                                        onclick: move |_| on_start_eval.call(0),
                                        "Начать первый замер"
                                    }
                                }
                            } else {
                                for ev in filtered.iter() {
                                    {
                                        let id = ev.id;
                                        let it = (*ev).clone();
                                        rsx! {
                                            EvalCard {
                                                item: it,
                                                on_open: move |_| selected_id.set(Some(id)),
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "config-overview",
                            div { class: "section-header-row",
                                div {
                                    div { class: "section-overline", "Настройка замеров" }
                                }
                            }
                            ConfigEntryCard {
                                icon_class: "config-icon icon-blue".to_string(),
                                icon: "📋".to_string(),
                                title: "Критерии".to_string(),
                                subtitle: format!("{criteria_count} критериев настроено"),
                                badge: criteria_count.to_string(),
                                badge_class: "badge badge-muted".to_string(),
                                on_click: move |_| on_open_criteria.call(()),
                            }
                            ConfigEntryCard {
                                icon_class: "config-icon icon-amber".to_string(),
                                icon: "📦".to_string(),
                                title: "Наборы критериев".to_string(),
                                subtitle: if default_sets > 0 {
                                    format!("{sets_count} наборов · {default_sets} по умолчанию")
                                } else {
                                    format!("{sets_count} наборов")
                                },
                                badge: if default_sets > 0 { "По умолч.".to_string() } else { sets_count.to_string() },
                                badge_class: if default_sets > 0 { "badge badge-amber".to_string() } else { "badge badge-muted".to_string() },
                                on_click: move |_| on_open_sets.call(()),
                            }
                            ConfigEntryCard {
                                icon_class: "config-icon icon-purple".to_string(),
                                icon: "🏷".to_string(),
                                title: "Типы замеров".to_string(),
                                subtitle: if types_items.is_empty() {
                                    "Создайте первый тип замера".to_string()
                                } else {
                                    types_items.iter().take(3).map(|item| item.name.clone()).collect::<Vec<_>>().join(", ")
                                },
                                badge: types_count.to_string(),
                                badge_class: "badge badge-muted".to_string(),
                                on_click: move |_| on_open_types.call(()),
                            }
                            ConfigEntryCard {
                                icon_class: "config-icon icon-amber".to_string(),
                                icon: "🤖".to_string(),
                                title: "AI ассистент".to_string(),
                                subtitle: "Чат и создание наборов критериев через AI".to_string(),
                                badge: "AI".to_string(),
                                badge_class: "badge badge-amber".to_string(),
                                on_click: move |_| on_open_ai.call(()),
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
fn ConfigEntryCard(
    icon_class: String,
    icon: String,
    title: String,
    subtitle: String,
    badge: String,
    badge_class: String,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "config-entry-card",
            onclick: move |_| on_click.call(()),
            div { class: "{icon_class}", "{icon}" }
            div { class: "config-entry-content",
                div { class: "config-entry-title", "{title}" }
                div { class: "config-entry-subtitle", "{subtitle}" }
            }
            div { class: "config-entry-right",
                span { class: "{badge_class}", "{badge}" }
                span { class: "config-entry-chevron", "›" }
            }
        }
    }
}

#[component]
fn EvalCard(item: EvaluationItem, on_open: EventHandler<()>) -> Element {
    let name = item
        .evaluated_employee_name
        .clone()
        .unwrap_or_else(|| "Неизвестно".into());
    let init = initials(&name);
    let av_cls = av_color(&name);
    let type_name = item
        .evaluation_type_name
        .clone()
        .unwrap_or_else(|| "Замер".into());
    let status = item.status.clone().unwrap_or_else(|| "completed".into());

    // Date formatting (just show raw string, first 10 chars)
    let date = if item.created_at.len() >= 10 {
        &item.created_at[..10]
    } else {
        &item.created_at
    };

    rsx! {
        div {
            class: "eval-card eval-card-interactive",
            onclick: move |_| on_open.call(()),
            div { class: "eval-card-header",
                div { class: "av av-sm {av_cls}", "{init}" }
                div { class: "eval-info",
                    div { class: "eval-name", "{name}" }
                    div { class: "eval-meta", "{type_name} · {date}" }
                }
                if status == "in_progress" {
                    span { class: "badge badge-blue", "В процессе" }
                } else {
                    span { class: "badge badge-green", "Завершена" }
                }
            }

            if let Some(score) = item.score_percentage {
                {
                    let clr = score_color(score);
                    let stars = (score / 20.0).round() as usize;
                    rsx! {
                        div { class: "eval-score-row",
                            div { class: "prog-track", style: "flex:1;",
                                div { class: "prog-fill", style: "width:{score:.0}%; background:{clr};" }
                            }
                            span { style: "color:{clr}; font-weight:600; font-size:15px; width:52px; text-align:right;",
                                "{score:.1}%"
                            }
                        }
                        div { class: "eval-stars",
                            for i in 0..5_usize {
                                span { class: if i < stars { "star star-on" } else { "star star-off" }, "★" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EvaluationDetailView(
    token: String,
    evaluation_id: i64,
    on_back: EventHandler<()>,
    on_continue: EventHandler<i64>,
) -> Element {
    let t = token.clone();
    let res = use_resource(move || {
        let tok = t.clone();
        let eid = evaluation_id;
        async move { api::fetch_evaluation_detail(&tok, eid).await }
    });

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll", style: "padding-bottom: 28px;",
                div { class: "page-header page-header-spacious",
                    style: "display: flex; align-items: center; gap: 10px;",
                    button {
                        class: "icon-btn",
                        onclick: move |_| on_back.call(()),
                        title: "Назад к списку",
                        "←"
                    }
                    div {
                        div { class: "label-text", "Замер №{evaluation_id}" }
                        div { class: "page-title page-title-lg", "Детали" }
                    }
                }

                match res() {
                    None => rsx! {
                        LoadingView { message: "Загрузка замера...".to_string() }
                    },
                    Some(Err(e)) => rsx! {
                        div { class: "list-section",
                            ErrorView { message: e }
                            button {
                                class: "btn-primary",
                                style: "margin-top: 14px;",
                                onclick: move |_| on_back.call(()),
                                "Назад"
                            }
                        }
                    },
                    Some(Ok(d)) => rsx! {
                        EvaluationDetailBody {
                            detail: d,
                            token: token.clone(),
                            on_continue,
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn EvaluationDetailBody(
    detail: EvaluationDetail,
    token: String,
    on_continue: EventHandler<i64>,
) -> Element {
    let d = detail;
    let org = d.organization_name.clone();
    let eval_type = d.evaluation_type_name.clone();
    let set_name = d
        .criterion_set_name
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let evaluated = d.evaluated_employee_name.clone();
    let filled_by = d.filled_by_employee_name.clone();
    let date_str = d.evaluation_date.clone();
    let ev_comment = d.comment.clone().unwrap_or_default();
    let passed = d.passed_criteria;
    let failed = d.failed_criteria;
    let total = d.total_criteria;
    let eid = d.evaluation_id;
    let completed = d.status == "completed";
    let score_opt = d.score_percentage;
    let api_base = crate::api::get_api_base_pub();
    let excel_url = format!(
        "{}/api/web/evaluations/{}/export/excel?access_token={}",
        api_base, eid, token
    );
    let pdf_url = format!(
        "{}/api/web/evaluations/{}/export/pdf?access_token={}",
        api_base, eid, token
    );

    rsx! {
        if !completed {
            button {
                class: "btn-primary",
                style: "width: 100%; margin-top: 8px;",
                onclick: move |_| on_continue.call(eid),
                "Продолжить заполнение"
            }
        }

        div { class: "success-result-card", style: "margin-top: 8px;",
            div { style: "display:flex; align-items:center; gap:11px; padding-bottom:12px;",
                div { class: "eval-av", style: "width:44px; height:44px; font-size:14px;",
                    {initials(&evaluated)}
                }
                div {
                    div { style: "font-size:15px; font-weight:500;", "{evaluated}" }
                    div { style: "font-size:12px; color:var(--text3);", "{eval_type}" }
                }
            }
            div { class: "result-divider" }
            div { style: "padding-top:12px; display:flex; flex-direction:column; gap:9px;",
                div { class: "result-row",
                    span { class: "result-label", "Организация" }
                    span { class: "result-value", "{org}" }
                }
                div { class: "result-row",
                    span { class: "result-label", "Тип замера" }
                    span { class: "result-value", "{eval_type}" }
                }
                div { class: "result-row",
                    span { class: "result-label", "Набор критериев" }
                    span { class: "result-value", "{set_name}" }
                }
                div { class: "result-row",
                    span { class: "result-label", "Оцениваемый" }
                    span { class: "result-value", "{evaluated}" }
                }
                div { class: "result-row",
                    span { class: "result-label", "Заполнил" }
                    span { class: "result-value", "{filled_by}" }
                }
                div { class: "result-row",
                    span { class: "result-label", "Дата" }
                    span { class: "result-value", "{date_str}" }
                }
                if let Some(score) = score_opt {
                    {
                        let clr = score_color(score);
                        rsx! {
                            div { class: "result-row",
                                span { class: "result-label", "Итоговый балл" }
                                span { class: "result-score", style: "color:{clr};", "{score:.1}%" }
                            }
                        }
                    }
                }
                div { class: "result-row",
                    span { class: "result-label", "Критерии" }
                    span { class: "result-value", "пройдено {passed} · не пройдено {failed} · всего {total}" }
                }
                if !ev_comment.is_empty() {
                    div { style: "padding-top:4px;",
                        div { class: "result-label", style: "display:block; margin-bottom:6px;", "Комментарий к замеру" }
                        p { style: "font-size:14px; color:var(--text2); line-height:1.45; margin:0;",
                            "{ev_comment}"
                        }
                    }
                }
            }
        }

        if completed {
            div { class: "export-section",
                span { class: "export-section-label", "Скачать отчёт" }
                div { class: "export-cards-row",
                    a {
                        class: "export-card",
                        href: "{excel_url}",
                        download: "evaluation_{eid}.xlsx",
                        div { class: "export-card-icon", "📊" }
                        span { class: "export-card-label", "Excel" }
                        span { class: "export-card-sub", "Таблица с\nответами" }
                    }
                    a {
                        class: "export-card",
                        href: "{pdf_url}",
                        download: "evaluation_{eid}.pdf",
                        div { class: "export-card-icon", "📄" }
                        span { class: "export-card-label", "PDF" }
                        span { class: "export-card-sub", "Готовый\nотчёт" }
                    }
                }
            }
        }

        div { class: "section-header-row", style: "margin-top: 22px;",
            div {
                div { class: "section-overline", "Ответы по критериям" }
            }
        }

        div { class: "list-section", style: "gap: 10px;",
            if d.criteria.is_empty() {
                p { class: "caption-text", "Нет сохранённых ответов (замер ещё не завершён или критерии не заполнены)." }
            } else {
                for (i, c) in d.criteria.iter().enumerate() {
                    {
                        let title = c.name.clone();
                        let vt = c.value_type.clone();
                        let val = c.value.clone();
                        let cm = c.comment.clone();
                        let disp = format_criterion_value(&vt, &val);
                        rsx! {
                            div {
                                key: "{i}",
                                class: "eval-detail-criterion",
                                div { class: "eval-detail-crit-head",
                                    span { class: "eval-detail-crit-title", "{title}" }
                                    span { class: "eval-detail-crit-val", "{disp}" }
                                }
                                if !cm.is_empty() {
                                    p { class: "eval-detail-crit-comment", "{cm}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
