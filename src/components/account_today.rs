//! Production Account Today surface. It renders only server-backed data and honest empty states.

use dioxus::prelude::*;

use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    organization_workflow_api::{
        OrganizationTask, OrganizationWorkflowApiClient, OrganizationWorkflowApiError,
    },
    restaurant_metrics_api::{RestaurantMetricDashboard, RestaurantMetricsApiClient},
    workforce_api::{WorkforceApiClient, WorkforceApiError, WorkforceVenue},
};

#[derive(Clone, Debug)]
struct TodayData {
    venues: Result<Vec<WorkforceVenue>, WorkforceApiError>,
    dashboard: Result<RestaurantMetricDashboard, ()>,
    tasks: Result<Vec<OrganizationTask>, OrganizationWorkflowApiError>,
}

fn safe_venue_error(error: &WorkforceApiError) -> &'static str {
    match error {
        WorkforceApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        WorkforceApiError::PermissionDenied => "Список заведений недоступен для вашей роли.",
        WorkforceApiError::NetworkUnavailable => "Нет связи с сервером.",
        _ => "Не удалось загрузить заведения.",
    }
}

fn current_date_label() -> String {
    let date = js_sys::Date::new_0();
    let weekdays = [
        "воскресенье",
        "понедельник",
        "вторник",
        "среда",
        "четверг",
        "пятница",
        "суббота",
    ];
    let months = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    let weekday = weekdays.get(date.get_day() as usize).copied().unwrap_or("");
    let month = months.get(date.get_month() as usize).copied().unwrap_or("");
    format!("Сегодня {} {}, {}", date.get_date(), month, weekday)
}

fn greeting_for_hour(hour: u32) -> &'static str {
    match hour {
        6..=11 => "Доброе утро",
        12..=16 => "Добрый день",
        17..=23 => "Добрый вечер",
        _ => "Доброй ночи",
    }
}

fn current_greeting() -> &'static str {
    greeting_for_hour(js_sys::Date::new_0().get_hours())
}

fn metric_component_count(value: &RestaurantMetricDashboard) -> usize {
    value
        .metrics
        .iter()
        .map(|metric| metric.components.len())
        .sum()
}

#[component]
pub fn AccountToday(
    manager_enabled: bool,
    on_assessments: EventHandler<()>,
    on_measure: EventHandler<()>,
    on_templates: EventHandler<()>,
    on_team: EventHandler<()>,
    on_dashboard: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let workforce = use_context::<WorkforceApiClient>();
    let metrics = use_context::<RestaurantMetricsApiClient>();
    let tasks_api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut reload = use_signal(|| 0_u64);

    let load_session = session.clone();
    let data = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let state = load_session.state();
        let workforce = workforce.clone();
        let metrics = metrics.clone();
        let tasks_api = tasks_api.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return None;
            };
            let company_id = account.selected_company.map(|value| value.0)?;
            let venues = workforce.venues(&account.access_token, company_id).await;
            let dashboard = metrics
                .dashboard(
                    &account.access_token,
                    company_id,
                    None,
                    true,
                    true,
                    None,
                    None,
                )
                .await
                .map_err(|_| ());
            let tasks = tasks_api
                .tasks(
                    &account.access_token,
                    company_id,
                    if manager_enabled { "review" } else { "mine" },
                )
                .await;
            (lifecycle_epoch() == epoch).then_some(TodayData {
                venues,
                dashboard,
                tasks,
            })
        }
    });

    let session_state = session.state();
    let selected_context = match &session_state {
        AccountSessionState::Authenticated(account) => {
            account.selected_company.and_then(|selected| {
                account
                    .bootstrap
                    .companies
                    .iter()
                    .find(|company| company.company_id == selected.0)
            })
        }
        _ => None,
    };
    let company_name = selected_context.map(|company| company.company_name.clone());
    let access_label = selected_context
        .map(|company| match company.relationship.as_str() {
            "owner" => "Владелец организации",
            "employee" if manager_enabled => "Руководитель в разрешённом scope",
            "employee" => "Сотрудник",
            _ => "Доступ определяется сервером",
        })
        .unwrap_or("Компания не выбрана");
    rsx! {
        section { class: "brand-today", aria_labelledby: "brand-today-title",
            header { class: "brand-today__header",
                div {
                    p { class: "brand-today__context", {company_name.as_deref().unwrap_or("RestOS")} }
                    h1 { id: "brand-today-title", "{current_greeting()}" }
                    p { class: "brand-today__date", "{current_date_label()}" }
                    p { class: "brand-today__access", "{access_label}" }
                }
            }

            div { class: "brand-today__grid",
                article { class: "brand-today__health brand-glass",
                    div { class: "brand-today__section-heading",
                        div {
                            p { class: "brand-today__eyebrow", "ОПЕРАЦИОННАЯ КАРТИНА" }
                            h2 { "Показатели за сегодня" }
                        }
                        span { class: "brand-status brand-status--normal", "Не рассчитано" }
                    }
                    button {
                        class: "brand-health-action", r#type: "button",
                        aria_label: if manager_enabled { "Открыть аналитику организации" } else { "Открыть мои оценки за сегодня" },
                        onclick: move |_| if manager_enabled { on_dashboard.call(()) } else { on_assessments.call(()) },
                        div { class: "brand-health-empty", role: "status",
                            div { class: "brand-health-ring", aria_hidden: "true", span { "—" } }
                            div {
                                strong { "Пока нет утверждённого итогового показателя" }
                                p { "Доступные компоненты показаны отдельно. Единый итог не рассчитывается без утверждённой формулы." }
                            }
                        }
                    }
                    if let Some(Some(today)) = data() {
                        if let Ok(dashboard) = &today.dashboard {
                            div { class: "brand-today__source-note",
                                strong { "Доступные компоненты: {metric_component_count(dashboard)}" }
                                ul {
                                    for metric in dashboard.metrics.iter() {
                                        for component in metric.components.iter() {
                                            li { "{metric.title}: {component.score_percent}% · {component.source_type}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                article { class: "brand-today__venues brand-glass brand-glass--secondary",
                    div { class: "brand-today__section-heading",
                        div { p { class: "brand-today__eyebrow", "КОНТЕКСТ" } h2 { "Заведения" } }
                        if manager_enabled {
                            button { class: "brand-text-action", r#type: "button", onclick: move |_| on_dashboard.call(()), "Показатели" }
                        }
                    }
                    match data() {
                        None => rsx! { div { class: "brand-state", role: "status", "Загрузка заведений..." } },
                        Some(None) => rsx! { div { class: "brand-state brand-state--attention", role: "status", "Контекст изменился. Обновите данные." } },
                        Some(Some(today)) => match &today.venues {
                            Ok(venues) if venues.is_empty() => rsx! { div { class: "brand-state", "Заведений пока нет." } },
                            Ok(venues) => rsx! {
                                ul { class: "brand-venue-list",
                                    for venue in venues.iter() {
                                        li { key: "today-venue-{venue.venue_id}",
                                            span { class: "brand-venue-mark", aria_hidden: "true", "R" }
                                            div { strong { "{venue.name}" } small { "Ресторан" } }
                                            span { class: "brand-status-dot", aria_hidden: "true" }
                                            span { class: "sr-only", "Доступно" }
                                        }
                                    }
                                }
                            },
                            Err(problem) => rsx! {
                                div { class: "brand-state brand-state--attention", role: "alert",
                                    p { "{safe_venue_error(problem)}" }
                                    button { class: "brand-button brand-button--secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" }
                                }
                            },
                        },
                    }
                }

                article { class: "brand-today__actions brand-glass brand-glass--secondary",
                    div { class: "brand-today__section-heading", div { p { class: "brand-today__eyebrow", "БЫСТРЫЕ ДЕЙСТВИЯ" } h2 { "Продолжить работу" } } }
                    div { class: "brand-action-grid",
                        button { class: "brand-quick-action", r#type: "button", onclick: move |_| on_assessments.call(()), span { aria_hidden: "true", "✓" } strong { "Мои оценки" } }
                        button { class: "brand-quick-action", r#type: "button", onclick: move |_| on_templates.call(()), span { aria_hidden: "true", "▦" } strong { "Шаблоны" } }
                        if manager_enabled {
                            button { class: "brand-quick-action brand-quick-action--primary", r#type: "button", onclick: move |_| on_measure.call(()), span { aria_hidden: "true", "+" } strong { "Сделать замер" } }
                            button { class: "brand-quick-action", r#type: "button", onclick: move |_| on_team.call(()), span { aria_hidden: "true", "◌" } strong { "Команда" } }
                        }
                    }
                }

                article { class: "brand-today__schedule brand-glass brand-glass--secondary",
                    div { class: "brand-today__section-heading", div { p { class: "brand-today__eyebrow", "СЕГОДНЯ" } h2 { "План дня" } } }
                    match data() {
                        Some(Some(today)) => match &today.tasks {
                            Ok(tasks) if tasks.is_empty() => rsx! { div { class: "brand-state", strong { if manager_enabled { "Нет задач на проверке" } else { "Активных задач пока нет" } } p { "Новые задачи появятся здесь автоматически." } } },
                            Ok(tasks) => rsx! { ul { class: "today-task-list",
                                for task in tasks.iter().take(5) {
                                    li { key: "today-task-{task.task_id}-{task.task_assignment_id:?}",
                                        div { strong { "{task.title}" } small { if task.assessment_attempt_id.is_some() { "По результатам замера" } else { "Рабочая задача" } } }
                                        span { class: "status-chip", "{task.assignment_status.as_deref().unwrap_or(&task.status)}" }
                                    }
                                }
                            } button { class: "brand-button brand-button--secondary", r#type: "button", onclick: move |_| on_team.call(()), if manager_enabled { "Открыть проверку" } else { "Открыть мои задачи" } } },
                            Err(_) => rsx! { div { class: "brand-state brand-state--attention", role: "alert", p { "Не удалось загрузить задачи." } button { class: "brand-button brand-button--secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" } } },
                        },
                        _ => rsx! { div { class: "brand-state", role: "status", "Загрузка задач…" } },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn user_journey_today_does_not_create_frontend_operational_score() {
        let source = include_str!("account_today.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("Пока нет утверждённого итогового показателя"));
        assert!(!production.contains("81–100"));
        assert!(!production.contains("82 / 100"));
    }

    #[wasm_bindgen_test]
    fn user_journey_greeting_boundaries_follow_local_user_time() {
        assert_eq!(greeting_for_hour(0), "Доброй ночи");
        assert_eq!(greeting_for_hour(6), "Доброе утро");
        assert_eq!(greeting_for_hour(12), "Добрый день");
        assert_eq!(greeting_for_hour(17), "Добрый вечер");
        assert_eq!(greeting_for_hour(23), "Добрый вечер");
    }

    #[wasm_bindgen_test]
    fn component_summary_counts_only_server_components() {
        let dashboard = RestaurantMetricDashboard {
            company_id: uuid::Uuid::nil(),
            venue_id: None,
            source_type: None,
            section_code: None,
            from_at: None,
            to_at: "2026-08-13T00:00:00Z".into(),
            timezone: "Europe/Moscow".into(),
            comparison_from_at: None,
            comparison_to_at: None,
            aggregation_status: "components_only".into(),
            metrics: Vec::new(),
        };
        assert_eq!(metric_component_count(&dashboard), 0);
    }
}
