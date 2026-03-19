//! Employee list page

use dioxus::prelude::*;
use crate::api;
use crate::types::{Employee, EmployeeTypeOption};
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

#[component]
pub fn EmployeesPage(token: String) -> Element {
    let mut search    = use_signal(String::new);
    let mut filter    = use_signal(|| "all".to_string());
    let mut editing_id: Signal<Option<i64>> = use_signal(|| None);
    let mut employees: Signal<Vec<Employee>> = use_signal(Vec::new);
    let mut roles: Signal<Vec<EmployeeTypeOption>> = use_signal(Vec::new);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);

    let t1 = token.clone();
    let initial = use_resource(move || {
        let tok = t1.clone();
        async move {
            let emps = api::fetch_employees(&tok).await;
            let rls  = api::fetch_employee_types(&tok).await;
            (emps, rls)
        }
    });

    use_effect(move || {
        if let Some((emps_result, roles_result)) = initial() {
            match emps_result {
                Ok(emps) => employees.set(emps),
                Err(e) => load_error.set(Some(e)),
            }
            if let Ok(rls) = roles_result {
                roles.set(rls);
            }
        }
    });

    if let Some(e) = load_error() {
        return rsx! { ErrorView { message: e } };
    }
    if initial().is_none() {
        return rsx! { LoadingView { message: "Загрузка команды...".to_string() } };
    }

    let emps = employees.read();
    let search_val = search.read().to_lowercase();
    let filter_val = filter.read().clone();
    let role_list  = roles.read();

    let filtered: Vec<&Employee> = emps.iter().filter(|e| {
        let name_match = search_val.is_empty() || e.full_name.to_lowercase().contains(&search_val);
        let role_match = match filter_val.as_str() {
            "admin"    => e.is_admin,
            "employee" => !e.is_admin,
            _          => true,
        };
        name_match && role_match
    }).collect();

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",

                div { class: "page-header",
                    div {
                        div { class: "page-title", "Команда" }
                        div { class: "page-subtitle", "{emps.len()} сотрудников" }
                    }
                }

                // Search bar
                div { class: "search-bar",
                    span { class: "search-icon", "🔍" }
                    input {
                        class: "search-input",
                        r#type: "search",
                        placeholder: "Поиск по имени...",
                        value: "{search}",
                        oninput: move |e| search.set(e.value()),
                    }
                }

                // Filter chips
                div { class: "filter-row",
                    for (val, lbl) in [("all","Все"), ("admin","Администраторы"), ("employee","Сотрудники")] {
                        {
                            let v = val.to_string();
                            let active = filter_val == val;
                            rsx! {
                                button {
                                    class: if active { "chip chip-active" } else { "chip" },
                                    onclick: move |_| filter.set(v.clone()),
                                    "{lbl}"
                                }
                            }
                        }
                    }
                }

                // Employee list
                div { class: "list-section",
                    if filtered.is_empty() {
                        div { class: "empty-state",
                            div { class: "empty-icon", "👥" }
                            p { class: "empty-text", "Никого не найдено" }
                        }
                    } else {
                        for emp in filtered.iter() {
                            {
                                let emp_id = emp.id;
                                let is_editing = editing_id() == Some(emp_id);
                                let t = token.clone();
                                rsx! {
                                    EmployeeCard {
                                        key: "{emp.id}",
                                        employee: (*emp).clone(),
                                        roles: role_list.clone(),
                                        is_editing,
                                        token: t,
                                        on_edit_toggle: move |_| {
                                            if editing_id() == Some(emp_id) {
                                                editing_id.set(None);
                                            } else {
                                                editing_id.set(Some(emp_id));
                                            }
                                        },
                                        on_role_changed: move |new_type_id: i64| {
                                            employees.write().iter_mut().for_each(|e| {
                                                if e.id == emp_id {
                                                    e.employee_type_id = new_type_id;
                                                }
                                            });
                                            editing_id.set(None);
                                        },
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

#[component]
fn EmployeeCard(
    employee: Employee,
    roles: Vec<EmployeeTypeOption>,
    is_editing: bool,
    token: String,
    on_edit_toggle: EventHandler<()>,
    on_role_changed: EventHandler<i64>,
) -> Element {
    let emp = employee.clone();
    let init  = initials(&emp.full_name);
    let av_cls = av_color(&emp.full_name);
    let current_role = roles.iter().find(|r| r.id == emp.employee_type_id);
    let role_name = current_role.map(|r| r.name.as_str()).unwrap_or("Сотрудник");
    let role_badge = if emp.is_admin { "badge-amber" } else { "badge-muted" };

    rsx! {
        div { class: "employee-card",
            div { class: "employee-main",
                div { class: "av {av_cls}", "{init}" }
                div { class: "employee-info",
                    div { class: "employee-name", "{emp.full_name}" }
                    if let Some(pos) = &emp.position {
                        if !pos.is_empty() {
                            div { class: "employee-pos", "{pos}" }
                        }
                    }
                }
                div { class: "employee-right",
                    span { class: "badge {role_badge}", "{role_name}" }
                    button {
                        class: "btn-ghost btn-icon-sm",
                        style: "margin-top:4px;",
                        onclick: move |_| on_edit_toggle.call(()),
                        if is_editing { "✕" } else { "✎" }
                    }
                }
            }

            if is_editing {
                div { class: "role-editor",
                    label { class: "field-label", "Изменить роль:" }
                    div { class: "role-chips",
                        for role in roles.iter() {
                            {
                                let rid = role.id;
                                let selected = emp.employee_type_id == rid;
                                let t = token.clone();
                                let emp_id = emp.id;
                                rsx! {
                                    button {
                                        class: if selected { "chip chip-active" } else { "chip" },
                                        onclick: move |_| {
                                            let tok = t.clone();
                                            let new_id = rid;
                                            spawn(async move {
                                                let _ = api::update_employee_role(&tok, emp_id, new_id).await;
                                            });
                                            on_role_changed.call(rid);
                                        },
                                        "{role.name}"
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
