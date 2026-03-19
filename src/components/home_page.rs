//! Dashboard / home page

use dioxus::prelude::*;
use crate::api;
use crate::auth::AuthState;
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
        0 => "av-amber",
        1 => "av-blue",
        2 => "av-green",
        _ => "av-purple",
    }
}

fn score_color(s: f64) -> &'static str {
    if s >= 80.0 { "var(--green)" } else if s >= 50.0 { "var(--amber)" } else { "var(--red)" }
}

#[component]
pub fn HomePage(token: String, on_navigate: EventHandler<String>) -> Element {
    let t = token.clone();
    let data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_analytics(&tok).await }
    });

    // Get user name from auth state for greeting
    let auth = AuthState::load();
    let user_name = auth.map(|a| a.name).unwrap_or_default();

    match data() {
        None => rsx! { LoadingView { message: "Загрузка...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(d)) => {
            let avg_str = format!("{:.1}", d.average_score);
            let scores  = d.top_employees.clone();

            rsx! {
                div { class: "app-screen",
                    div { class: "hero-glow" }
                    div { class: "screen-scroll",

                        // Top bar
                        div { style: "display:flex; align-items:center; justify-content:space-between; padding:16px 20px 0; position:relative; z-index:1;",
                            div {
                                div { class: "heading-lg", style: "margin-top:2px;", "Добро пожаловать" }
                                span { class: "label-text", "{user_name}" }
                            }
                            div {
                                class: "av av-amber",
                                title: "Выйти",
                                onclick: move |_| on_navigate.call("logout".to_string()),
                                style: "cursor:pointer;",
                                { initials(&user_name) }
                            }
                        }

                        // Stats row
                        div { class: "stats-row", style: "margin-top:16px;",
                            div { class: "stat-card",
                                span { class: "label-text", "Оценок" }
                                div { class: "stat-num", "{d.total_evaluations}" }
                                span { class: "caption-text", style: "margin-top:4px; display:block;", "всего" }
                            }
                            div { class: "stat-card",
                                span { class: "label-text", "Ср. балл" }
                                div { class: "stat-num", style: "color:var(--amber);",
                                    "{avg_str}" span { "%" }
                                }
                            }
                            div { class: "stat-card",
                                span { class: "label-text", "Сотруд." }
                                div { class: "stat-num", "{d.employees_count}" }
                            }
                        }

                        // Quick actions
                        div { class: "pad", style: "margin-top:24px;",
                            span { class: "label-text", "Быстрые действия" }
                            div { class: "actions-grid", style: "margin-top:10px;",
                                ActionCard { icon: "✏️", title: "Оценить", desc: "Новая оценка", amber: true,
                                    on_click: move |_: ()| on_navigate.call("form".to_string()) }
                                ActionCard { icon: "⭐", title: "Оценки", desc: "История",
                                    amber: false, on_click: move |_: ()| on_navigate.call("evaluations".to_string()) }
                                ActionCard { icon: "📈", title: "Аналитика", desc: "Статистика",
                                    amber: false, on_click: move |_: ()| on_navigate.call("analytics".to_string()) }
                                ActionCard { icon: "👥", title: "Команда", desc: "Сотрудники",
                                    amber: false, on_click: move |_: ()| on_navigate.call("employees".to_string()) }
                            }
                        }

                        // Top employees ranking
                        if !scores.is_empty() {
                            div { class: "pad", style: "margin-top:24px;",
                                div { class: "section-header",
                                    span { class: "label-text", "Рейтинг сотрудников" }
                                }
                                div { class: "card", style: "padding:4px 0;",
                                    for (i, emp) in scores.iter().take(4).enumerate() {
                                        {
                                            let medal = match i { 0 => "🥇", 1 => "🥈", 2 => "🥉", _ => "·" };
                                            let av_cls = av_color(&emp.name);
                                            let init   = initials(&emp.name);
                                            let clr    = score_color(emp.avg);
                                            let pct    = emp.avg;
                                            let name   = emp.name.clone();
                                            rsx! {
                                                div { class: "list-row", style: "padding:12px 16px;",
                                                    span { class: "rank-medal", "{medal}" }
                                                    div { class: "av av-sm {av_cls}", "{init}" }
                                                    div { style: "flex:1; min-width:0;",
                                                        div { class: "rank-name", "{name}" }
                                                        div { class: "prog-track", style: "margin-top:5px;",
                                                            div { class: "prog-fill", style: "width:{pct:.0}%;" }
                                                        }
                                                    }
                                                    span { style: "font-size:15px; font-weight:500; color:{clr}; flex-shrink:0;",
                                                        "{pct:.1}%"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { style: "height:20px;" }
                    }
                }
            }
        }
    }
}

#[component]
fn ActionCard(
    icon: &'static str,
    title: &'static str,
    desc: &'static str,
    amber: bool,
    on_click: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: if amber { "action-card action-card-amber" } else { "action-card" },
            onclick: move |_| on_click.call(()),
            div { class: "action-icon", "{icon}" }
            div { class: "action-title", "{title}" }
            div { class: "action-desc", "{desc}" }
        }
    }
}
