//! Analytics page

use dioxus::prelude::*;
use crate::api;
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
    if s >= 80.0 { "var(--green)" } else if s >= 60.0 { "var(--amber)" } else { "var(--red)" }
}

#[component]
pub fn AnalyticsPage(token: String) -> Element {
    let t = token.clone();
    let data = use_resource(move || {
        let tok = t.clone();
        async move { api::fetch_analytics(&tok).await }
    });

    let mut active_tab = use_signal(|| "employees".to_string());

    match data() {
        None => rsx! { LoadingView { message: "Загрузка аналитики...".to_string() } },
        Some(Err(e)) => rsx! { ErrorView { message: e } },
        Some(Ok(d)) => {
            let avg_str = format!("{:.1}", d.average_score);
            let tab = active_tab.read().clone();
            let top = d.top_employees.clone();
            let crit = d.criteria_stats.clone();
            let my_rows = d.my_evaluations.clone();

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",

                        div { class: "page-header",
                            div {
                                div { class: "page-title", "Аналитика" }
                                div { class: "page-subtitle", if d.is_personal_view { "Ваши результаты" } else { "Статистика организации" } }
                            }
                        }

                        // Summary cards
                        div { class: "stats-row",
                            div { class: "stat-card",
                                span { class: "label-text", "Замеров" }
                                div { class: "stat-num", "{d.total_evaluations}" }
                            }
                            div { class: "stat-card",
                                span { class: "label-text", if d.is_personal_view { "Ср. за месяц" } else { "Ср. балл" } }
                                div { class: "stat-num", style: "color:var(--amber);",
                                    if d.is_personal_view { "{d.monthly_average_score:.1}" } else { "{avg_str}" } span { "%" }
                                }
                            }
                            div { class: "stat-card",
                                span { class: "label-text", if d.is_personal_view { "Статус" } else { "Сотруд." } }
                                div { class: "stat-num", if d.is_personal_view { "Я" } else { "{d.employees_count}" } }
                            }
                        }

                        if !d.is_personal_view {
                            // Tabs
                            div { class: "tab-bar", style: "margin: 16px 20px 0;",
                                button {
                                    class: if tab == "employees" { "tab active" } else { "tab" },
                                    onclick: move |_| active_tab.set("employees".to_string()),
                                    "Сотрудники"
                                }
                                button {
                                    class: if tab == "criteria" { "tab active" } else { "tab" },
                                    onclick: move |_| active_tab.set("criteria".to_string()),
                                    "Критерии"
                                }
                            }
                        }

                        div { class: "pad", style: "margin-top:16px;",
                            if d.is_personal_view {
                                if my_rows.is_empty() {
                                    div { class: "empty-state",
                                        div { class: "empty-icon", "📈" }
                                        p { class: "empty-text", "Пока нет ваших завершённых замеров" }
                                    }
                                } else {
                                    div { class: "card", style: "padding:4px 0;",
                                        for row in my_rows.iter() {
                                            {
                                                let filler = row.filled_by_employee_name.clone().unwrap_or_else(|| "—".to_string());
                                                let date = if row.created_at.len() >= 10 { row.created_at[..10].to_string() } else { row.created_at.clone() };
                                                let score_text = row.score_percentage.map(|s| format!("{s:.1}%")).unwrap_or_else(|| "Черновик".to_string());
                                                let clr = row.score_percentage.map(score_color).unwrap_or("var(--text3)");
                                                rsx! {
                                                    div { class: "list-row", style: "padding:12px 16px;",
                                                        div { class: "av av-sm {av_color(&filler)}", "{initials(&filler)}" }
                                                        div { style: "flex:1; min-width:0;",
                                                            div { class: "rank-name", "Кто заполнил: {filler}" }
                                                            div { class: "caption-text", "Дата: {date}" }
                                                        }
                                                        span { style: "font-size:14px; font-weight:600; color:{clr};", "{score_text}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if !d.is_personal_view && tab == "employees" {
                                if top.is_empty() {
                                    div { class: "empty-state",
                                        div { class: "empty-icon", "📈" }
                                        p { class: "empty-text", "Нет данных" }
                                    }
                                } else {
                                    div { class: "card", style: "padding:4px 0;",
                                        for (i, emp) in top.iter().enumerate() {
                                            {
                                                let medal = match i { 0 => "🥇", 1 => "🥈", 2 => "🥉", _ => "·" };
                                                let av_cls = av_color(&emp.name);
                                                let init   = initials(&emp.name);
                                                let clr    = score_color(emp.avg);
                                                let pct    = emp.avg;
                                                let name   = emp.name.clone();
                                                let count  = emp.count;
                                                rsx! {
                                                    div { class: "list-row", style: "padding:12px 16px;",
                                                        span { class: "rank-medal", "{medal}" }
                                                        div { class: "av av-sm {av_cls}", "{init}" }
                                                        div { style: "flex:1; min-width:0;",
                                                            div { class: "rank-name", "{name}" }
                                                            div { class: "caption-text", "{count} замеров" }
                                                            div { class: "prog-track", style: "margin-top:5px;",
                                                                div { class: "prog-fill", style: "width:{pct:.0}%; background:{clr};" }
                                                            }
                                                        }
                                                        span { style: "font-size:15px; font-weight:600; color:{clr}; flex-shrink:0;",
                                                            "{pct:.1}%"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if !d.is_personal_view && tab == "criteria" {
                                if crit.is_empty() {
                                    div { class: "empty-state",
                                        div { class: "empty-icon", "📋" }
                                        p { class: "empty-text", "Нет данных по критериям" }
                                    }
                                } else {
                                    div { class: "card", style: "padding:4px 0;",
                                        for stat in crit.iter() {
                                            {
                                                let pct = stat.pass_rate;
                                                let clr = score_color(pct);
                                                let name = stat.criterion_name.clone();
                                                rsx! {
                                                    div { class: "list-row", style: "padding:12px 16px;",
                                                        div { style: "flex:1; min-width:0;",
                                                            div { class: "rank-name", "{name}" }
                                                            div { class: "prog-track", style: "margin-top:5px;",
                                                                div { class: "prog-fill", style: "width:{pct:.0}%; background:{clr};" }
                                                            }
                                                        }
                                                        span { style: "font-size:14px; font-weight:600; color:{clr}; flex-shrink:0; margin-left:12px;",
                                                            "{pct:.0}%"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
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
