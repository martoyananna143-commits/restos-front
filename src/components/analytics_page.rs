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
    if s >= 80.0 { "var(--green)" } else if s >= 50.0 { "var(--amber)" } else { "var(--red)" }
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

            rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",

                        div { class: "page-header",
                            div {
                                div { class: "page-title", "Аналитика" }
                                div { class: "page-subtitle", "Статистика организации" }
                            }
                        }

                        // Summary cards
                        div { class: "stats-row",
                            div { class: "stat-card",
                                span { class: "label-text", "Оценок" }
                                div { class: "stat-num", "{d.total_evaluations}" }
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

                        div { class: "pad", style: "margin-top:16px;",
                            if tab == "employees" {
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
                                                            div { class: "caption-text", "{count} оценок" }
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

                            if tab == "criteria" {
                                if crit.is_empty() {
                                    div { class: "empty-state",
                                        div { class: "empty-icon", "📋" }
                                        p { class: "empty-text", "Нет данных по критериям" }
                                    }
                                } else {
                                    div { class: "card", style: "padding:4px 0;",
                                        for stat in crit.iter() {
                                            {
                                                let pct = stat.pass_rate * 100.0;
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
