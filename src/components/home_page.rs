//! Organization dashboard.

use dioxus::prelude::*;

use crate::api;
use crate::auth::AuthState;
use crate::types::{EvaluationDetail, EvaluationItem, OrgInfo};
use super::shared::{ErrorView, LoadingView};

fn score_tone(score: f64) -> &'static str {
    if score >= 80.0 { "score-good" } else if score >= 60.0 { "score-warn" } else { "score-bad" }
}

fn format_date(value: &str) -> String {
    let date = value.split('T').next().unwrap_or(value);
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3 {
        format!("{}.{}.{}", parts[2], parts[1], parts[0])
    } else {
        value.to_string()
    }
}

#[component]
pub fn HomePage(
    token: String,
    can_use_evaluations: bool,
    on_navigate: EventHandler<String>,
    on_switch_auth: EventHandler<AuthState>,
) -> Element {
    let t = token.clone();
    let data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_evaluations(&tok).await }
    });

    let auth = AuthState::load();
    let user_name = auth.as_ref().map(|state| state.name.clone()).unwrap_or_default();
    let is_superuser = auth.as_ref().map(|state| state.is_superuser).unwrap_or(false);
    let available_orgs: Vec<OrgInfo> = auth.as_ref().map(|state| state.available_orgs.clone()).unwrap_or_default();
    let current_org_id = auth.as_ref().map(|state| state.org_id).unwrap_or_default();
    let current_org_name = available_orgs
        .iter()
        .find(|org| org.id == current_org_id)
        .map(|org| org.name.clone())
        .unwrap_or_else(|| "Текущая организация".to_string());

    let mut org_menu_open = use_signal(|| false);
    let mut switching = use_signal(|| false);
    let mut switch_error: Signal<Option<String>> = use_signal(|| None);

    match data() {
        None => rsx! { LoadingView { message: "Загрузка главной...".to_string() } },
        Some(Err(error)) => rsx! { ErrorView { message: error } },
        Some(Ok(evaluations)) => {
            let is_personal_view = !can_use_evaluations;
            let mut completed: Vec<EvaluationItem> = evaluations
                .into_iter()
                .filter(|item| item.status.as_deref() == Some("completed") && item.score_percentage.is_some())
                .collect();
            completed.sort_by(|left, right| right.created_at.cmp(&left.created_at));
            let average = if completed.is_empty() {
                None
            } else {
                Some(completed.iter().filter_map(|item| item.score_percentage).sum::<f64>() / completed.len() as f64)
            };

            rsx! {
                div { class: "app-screen home-dashboard",
                    div { class: "screen-scroll dashboard-scroll",
                        header { class: "dashboard-header",
                            div { class: "dashboard-identity",
                                span { class: "dashboard-eyebrow", "{current_org_name}" }
                                h1 { "Здравствуйте, {user_name}" }
                                p { class: "dashboard-intro", "Вот актуальные результаты завершённых замеров." }
                                if is_superuser {
                                    span { class: "badge badge-amber", "Суперюзер" }
                                }
                            }
                            div { class: "dashboard-header-actions",
                                if available_orgs.len() > 1 {
                                    button {
                                        class: "organization-switcher",
                                        r#type: "button",
                                        aria_label: "Сменить организацию",
                                        aria_expanded: if org_menu_open() { "true" } else { "false" },
                                        onclick: move |_| org_menu_open.set(!org_menu_open()),
                                        span { aria_hidden: "true", "⌘" }
                                        span { "Организация" }
                                        span { class: if org_menu_open() { "switcher-chevron switcher-chevron--open" } else { "switcher-chevron" }, aria_hidden: "true", "⌄" }
                                    }
                                }
                                button {
                                    class: "dashboard-profile-button",
                                    r#type: "button",
                                    aria_label: "Открыть профиль",
                                    onclick: move |_| on_navigate.call("profile".to_string()),
                                    "{initials(&user_name)}"
                                }
                            }
                        }

                        if org_menu_open() && available_orgs.len() > 1 {
                            div { class: "organization-menu", role: "menu",
                                div { class: "organization-menu-title", "Выберите организацию" }
                                for org in available_orgs.iter() {
                                    {
                                        let org_id = org.id;
                                        let org_name = org.name.clone();
                                        let is_current = org_id == current_org_id;
                                        let tok = token.clone();
                                        rsx! {
                                            button {
                                                class: if is_current { "organization-option organization-option--active" } else { "organization-option" },
                                                r#type: "button",
                                                role: "menuitem",
                                                disabled: is_current || switching(),
                                                onclick: move |_| {
                                                    let tok = tok.clone();
                                                    spawn(async move {
                                                        switching.set(true);
                                                        switch_error.set(None);
                                                        match api::switch_org(&tok, org_id).await {
                                                            Ok(response) => {
                                                                let state = AuthState {
                                                                    access_token: response.access_token,
                                                                    employee_id: response.employee_id,
                                                                    org_id: response.org_id,
                                                                    name: response.name,
                                                                    is_admin: response.is_admin,
                                                                    is_superuser: response.is_superuser,
                                                                    available_orgs: response.available_orgs,
                                                                };
                                                                state.save();
                                                                org_menu_open.set(false);
                                                                on_switch_auth.call(state);
                                                            }
                                                            Err(error) => switch_error.set(Some(error)),
                                                        }
                                                        switching.set(false);
                                                    });
                                                },
                                                span { "{org_name}" }
                                                if is_current { span { aria_hidden: "true", "✓" } }
                                            }
                                        }
                                    }
                                }
                                if let Some(error) = switch_error() {
                                    p { class: "organization-error", role: "alert", "{error}" }
                                }
                            }
                        }

                        main { class: "dashboard-content",
                            section {
                                class: "score-panel",
                                aria_label: if is_personal_view { "Мой средний показатель" } else { "Средний показатель по заведению" },
                                ScoreRing { value: average, is_personal: is_personal_view }
                                div { class: "score-panel-copy",
                                    span { class: "dashboard-eyebrow", "Результат команды" }
                                    h2 {
                                        if is_personal_view { "Мой средний показатель" } else { "Средний показатель по заведению" }
                                    }
                                    p {
                                        if completed.is_empty() {
                                            "Завершённых замеров пока нет. Черновики не учитываются."
                                        } else {
                                            if is_personal_view {
                                                "Рассчитано по связанным с вами завершённым замерам без учёта черновиков."
                                            } else {
                                                "Рассчитано по завершённым замерам текущей организации без учёта черновиков."
                                            }
                                        }
                                    }
                                    if can_use_evaluations {
                                        button {
                                            class: "btn-primary score-action",
                                            r#type: "button",
                                            onclick: move |_| on_navigate.call("form".to_string()),
                                            "Новый замер"
                                        }
                                    }
                                }
                            }

                            section {
                                class: "recent-measurements",
                                aria_label: if is_personal_view { "Мои последние замеры" } else { "Последние завершённые замеры" },
                                div { class: "recent-measurements-head",
                                    div {
                                        span { class: "dashboard-eyebrow", "История" }
                                        h2 {
                                            if is_personal_view { "Мои последние замеры" } else { "Последние завершённые замеры" }
                                        }
                                    }
                                    if can_use_evaluations && !completed.is_empty() {
                                        button {
                                            class: "section-text-button",
                                            r#type: "button",
                                            onclick: move |_| on_navigate.call("evaluations".to_string()),
                                            "Все замеры"
                                        }
                                    }
                                }
                                if completed.is_empty() {
                                    div { class: "measurements-empty",
                                        div { class: "measurements-empty-icon", aria_hidden: "true", "◎" }
                                        h3 { "Пока нет завершённых замеров" }
                                        p { "Когда первый замер будет завершён, его результат появится здесь." }
                                    }
                                } else {
                                    div { class: "measurement-list",
                                        for item in completed.iter().take(5) {
                                            CompletedMeasurementRow { key: "{item.id}", token: token.clone(), item: item.clone() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ScoreRing(value: Option<f64>, is_personal: bool) -> Element {
    let shown = value.map(|score| score.clamp(0.0, 100.0));
    let offset = 339.292 - shown.unwrap_or(0.0) * 3.39292;
    let tone = shown.map(score_tone).unwrap_or("score-neutral");
    let label = shown.map(|score| format!("{score:.1}%")).unwrap_or_else(|| "—".to_string());
    let subject = if is_personal { "Мой средний показатель" } else { "Средний показатель по заведению" };
    let aria = shown
        .map(|score| format!("{subject}: {score:.1} процента"))
        .unwrap_or_else(|| format!("{subject} недоступен: завершённых замеров нет"));

    rsx! {
        div { class: "score-ring {tone}", role: "img", aria_label: "{aria}",
            svg { view_box: "0 0 140 140", "aria-hidden": "true",
                circle { class: "score-ring-shadow", cx: "70", cy: "70", r: "54" }
                circle { class: "score-ring-track", cx: "70", cy: "70", r: "54" }
                if shown.is_some() {
                    circle {
                        class: "score-ring-progress",
                        cx: "70", cy: "70", r: "54",
                        stroke_dasharray: "339.292",
                        stroke_dashoffset: "{offset}",
                    }
                }
            }
            div { class: "score-ring-center",
                strong { "{label}" }
                span { "средний показатель" }
            }
        }
    }
}

#[component]
fn CompletedMeasurementRow(token: String, item: EvaluationItem) -> Element {
    let id = item.id;
    let t = token.clone();
    let detail = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_evaluation_detail(&tok, id).await }
    });
    let score = item.score_percentage.unwrap_or_default();
    let tone = score_tone(score);
    let employee_name = item.evaluated_employee_name.clone().unwrap_or_else(|| "Сотрудник".to_string());
    let employee_initials = initials(&employee_name);
    let created_at = item.created_at.clone();
    let created_label = format_date(&created_at);
    let fallback_type = item.evaluation_type_name.clone().unwrap_or_else(|| "—".to_string());
    let detail_content = match detail() {
        None => rsx! { div { class: "measurement-meta measurement-meta--loading", "Загрузка деталей…" } },
        Some(Ok(info)) => rsx! { MeasurementMeta { detail: info } },
        Some(Err(_)) => rsx! {
            div { class: "measurement-meta",
                span { small { "Тип" } strong { "{fallback_type}" } }
                span { small { "Шаблон" } strong { "—" } }
                span { small { "Проверяющий" } strong { "—" } }
            }
        },
    };

    rsx! {
        article { class: "measurement-row",
            div { class: "measurement-primary",
                div { class: "measurement-avatar", aria_hidden: "true", "{employee_initials}" }
                div { class: "measurement-person",
                    h3 { "{employee_name}" }
                    time { datetime: "{created_at}", "{created_label}" }
                }
                span { class: "measurement-score {tone}", "{score:.1}%" }
            }
            {detail_content}
        }
    }
}

#[component]
fn MeasurementMeta(detail: EvaluationDetail) -> Element {
    let template = detail.criterion_set_name.clone().unwrap_or_else(|| "—".to_string());
    rsx! {
        div { class: "measurement-meta",
            span { small { "Тип" } strong { "{detail.evaluation_type_name}" } }
            span { small { "Шаблон" } strong { "{template}" } }
            span { small { "Проверяющий" } strong { "{detail.filled_by_employee_name}" } }
        }
    }
}

fn initials(name: &str) -> String {
    let mut parts = name.split_whitespace().filter_map(|part| part.chars().next());
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) => format!("{first}{second}").to_uppercase(),
        (Some(first), None) => first.to_uppercase().to_string(),
        _ => "?".to_string(),
    }
}
