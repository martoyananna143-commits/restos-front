//! Evaluations overview with config entry cards.

use dioxus::prelude::*;

use crate::api;
use crate::types::EvaluationItem;

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
        ).to_uppercase(),
    }
}

fn av_color(name: &str) -> &'static str {
    match name.bytes().next().unwrap_or(0) % 4 {
        0 => "av-amber", 1 => "av-blue", 2 => "av-green", _ => "av-purple",
    }
}

fn score_color(s: f64) -> &'static str {
    if s >= 80.0 { "var(--green)" } else if s >= 50.0 { "var(--amber)" } else { "var(--red)" }
}

#[component]
pub fn EvaluationsPage(
    token: String,
    on_start_eval: EventHandler<i64>,
    on_open_criteria: EventHandler<()>,
    on_open_sets: EventHandler<()>,
    on_open_types: EventHandler<()>,
) -> Element {
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
        None => rsx! { LoadingView { message: "Загрузка оценок...".to_string() } },
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
            let filtered: Vec<&EvaluationItem> = evals.iter().filter(|ev| {
                match filter_val.as_str() {
                    "completed" => ev.score_percentage.is_some(),
                    "in_progress" => ev.score_percentage.is_none(),
                    _ => true,
                }
            }).collect();

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        div { class: "page-header page-header-spacious",
                            div {
                                div { class: "label-text", "Ваши оценки" }
                                div { class: "page-title page-title-lg", "Оценки" }
                            }
                            div { class: "header-actions",
                                button {
                                    class: "icon-btn icon-btn-amber",
                                    onclick: move |_| on_open_criteria.call(()),
                                    title: "Настройки оценок",
                                    "⚙"
                                }
                                button {
                                    class: "icon-btn icon-btn-solid",
                                    onclick: move |_| on_start_eval.call(0),
                                    title: "Новая оценка",
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
                                    p { class: "empty-text", "Нет оценок" }
                                    p { class: "caption-text", style: "max-width:220px; text-align:center;", "Создайте первую оценку, чтобы начать отслеживать прогресс команды" }
                                    button {
                                        class: "btn-primary",
                                        style: "margin-top:16px;",
                                        onclick: move |_| on_start_eval.call(0),
                                        "Начать первую оценку"
                                    }
                                }
                            } else {
                                for ev in filtered.iter() {
                                    EvalCard { item: (*ev).clone() }
                                }
                            }
                        }

                        div { class: "config-overview",
                            div { class: "section-header-row",
                                div {
                                    div { class: "section-overline", "Настройка оценок" }
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
                                title: "Типы оценок".to_string(),
                                subtitle: if types_items.is_empty() {
                                    "Создайте первый тип оценки".to_string()
                                } else {
                                    types_items.iter().take(3).map(|item| item.name.clone()).collect::<Vec<_>>().join(", ")
                                },
                                badge: types_count.to_string(),
                                badge_class: "badge badge-muted".to_string(),
                                on_click: move |_| on_open_types.call(()),
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
fn EvalCard(item: EvaluationItem) -> Element {
    let name = item.evaluated_employee_name.clone().unwrap_or_else(|| "Неизвестно".into());
    let init = initials(&name);
    let av_cls = av_color(&name);
    let type_name = item.evaluation_type_name.clone().unwrap_or_else(|| "Оценка".into());
    let status = item.status.clone().unwrap_or_else(|| "completed".into());

    // Date formatting (just show raw string, first 10 chars)
    let date = if item.created_at.len() >= 10 { &item.created_at[..10] } else { &item.created_at };

    rsx! {
        div { class: "eval-card",
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
