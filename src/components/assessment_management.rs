//! Minimal Account-scoped assessment management UI.

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::JsValue;

use crate::{
    account_api::{BootstrapCompany, SelectedCompanyId},
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_management_api::{
        AssessmentManagementApiClient, AssessmentManagementApiError, AssignableTemplate,
        AssignmentStatus, CreateAssignmentRequest, ManagementEmployee, ManagerAssignment,
    },
    workforce_api::{WorkforceApiClient, WorkforceVenue},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagerCapability {
    Checking,
    Authorized,
    Hidden,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
struct ManagementData {
    company_id: Uuid,
    lifecycle_epoch: u64,
    company_generation: u64,
    employees: Vec<ManagementEmployee>,
    templates: Vec<AssignableTemplate>,
    assignments: Vec<ManagerAssignment>,
    venues: Vec<WorkforceVenue>,
}

pub fn bootstrap_owner_capability(
    companies: &[BootstrapCompany],
    selected_company: Option<Uuid>,
) -> ManagerCapability {
    let Some(company_id) = selected_company else {
        return ManagerCapability::Hidden;
    };
    if companies
        .iter()
        .any(|company| company.company_id == company_id && company.relationship == "owner")
    {
        ManagerCapability::Authorized
    } else {
        ManagerCapability::Checking
    }
}

pub fn capability_from_probe(
    result: &Result<Vec<ManagementEmployee>, AssessmentManagementApiError>,
) -> ManagerCapability {
    match result {
        Ok(_) => ManagerCapability::Authorized,
        Err(AssessmentManagementApiError::PermissionDenied) => ManagerCapability::Hidden,
        Err(_) => ManagerCapability::Error,
    }
}

pub fn scoped_result_is_current(
    current_epoch: u64,
    operation_epoch: u64,
    current_company_generation: u64,
    operation_company_generation: u64,
    current_company: Option<Uuid>,
    operation_company: Uuid,
    current_operation_generation: u64,
    operation_generation: u64,
) -> bool {
    current_epoch == operation_epoch
        && current_company_generation == operation_company_generation
        && current_company == Some(operation_company)
        && current_operation_generation == operation_generation
}

pub fn create_is_admitted(
    employee: Option<Uuid>,
    template: Option<Uuid>,
    confirming: bool,
    in_flight: bool,
    due_valid: bool,
) -> bool {
    employee.is_some() && template.is_some() && confirming && !in_flight && due_valid
}

pub fn revoke_is_admitted(status: AssignmentStatus, confirming: bool, in_flight: bool) -> bool {
    matches!(
        status,
        AssignmentStatus::Assigned | AssignmentStatus::InProgress
    ) && confirming
        && !in_flight
}

pub fn due_millis_is_valid(now_millis: f64, due_millis: Option<f64>) -> bool {
    now_millis.is_finite()
        && due_millis
            .map(|value| value.is_finite() && value > now_millis)
            .unwrap_or(true)
}

pub fn status_label(status: AssignmentStatus) -> &'static str {
    match status {
        AssignmentStatus::Assigned => "Назначена",
        AssignmentStatus::InProgress => "В работе",
        AssignmentStatus::Completed => "Завершена",
        AssignmentStatus::Revoked => "Отозвана",
    }
}

pub fn activity_label(value: &str) -> &'static str {
    match value {
        "evaluation" => "Оценка",
        "measurement" => "Измерение",
        "walkthrough" => "Обход",
        "checklist" => "Чек-лист",
        "test" => "Тест",
        "survey" => "Опрос",
        "attestation" => "Аттестация",
        _ => "Шаблон",
    }
}

pub fn safe_management_error(error: &AssessmentManagementApiError) -> &'static str {
    match error {
        AssessmentManagementApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        AssessmentManagementApiError::PermissionDenied => {
            "Доступ к назначениям этой компании отозван."
        }
        AssessmentManagementApiError::NotFound => "Назначение больше недоступно.",
        AssessmentManagementApiError::DuplicateActiveAssignment => {
            "Эта версия оценки уже назначена сотруднику."
        }
        AssessmentManagementApiError::AlreadyCompleted => "Завершённую оценку нельзя отозвать.",
        AssessmentManagementApiError::StateConflict => "Состояние изменилось. Обновите список.",
        AssessmentManagementApiError::InvalidRequest => {
            "Проверьте сотрудника, шаблон и срок выполнения."
        }
        AssessmentManagementApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите действие вручную."
        }
        AssessmentManagementApiError::InternalError => {
            "Не удалось выполнить действие. Попробуйте позже."
        }
    }
}

fn selected_company(state: &AccountSessionState) -> Option<Uuid> {
    match state {
        AccountSessionState::Authenticated(value) => value.selected_company.map(|id| id.0),
        _ => None,
    }
}

fn local_due_to_utc(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(String::new());
    }
    let date = js_sys::Date::new(&JsValue::from_str(trimmed));
    let millis = date.get_time();
    if !due_millis_is_valid(js_sys::Date::now(), Some(millis)) {
        return None;
    }
    date.to_iso_string().as_string()
}

fn current_token_and_company(
    session: &AccountSessionAdapter,
) -> Option<(crate::account_api::AccountAccessToken, Uuid)> {
    let AccountSessionState::Authenticated(value) = session.state() else {
        return None;
    };
    Some((value.access_token, value.selected_company?.0))
}

#[component]
pub fn AssessmentManagementPage(on_company_changed: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentManagementApiClient>();
    let workforce_api = use_context::<WorkforceApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut company_generation = use_signal(|| 0_u64);
    let mut operation_generation = use_signal(|| 0_u64);
    let mut reload = use_signal(|| 0_u64);
    let mut search = use_signal(String::new);
    let mut selected_employee = use_signal(|| None::<Uuid>);
    let mut selected_template = use_signal(|| None::<Uuid>);
    let mut selected_venue = use_signal(|| None::<Uuid>);
    let mut due_at = use_signal(String::new);
    let mut confirming_create = use_signal(|| false);
    let mut creating = use_signal(|| false);
    let mut mutation_error = use_signal(|| None::<String>);
    let mut detail = use_signal(|| None::<ManagerAssignment>);
    let mut detail_loading = use_signal(|| false);
    let mut confirming_revoke = use_signal(|| false);
    let mut revoking = use_signal(|| false);

    let load_session = session.clone();
    let load_api = api.clone();
    let load_workforce_api = workforce_api.clone();
    let data = use_resource(move || {
        let _reload = reload();
        let query = search();
        let operation_epoch = lifecycle_epoch();
        let operation_company_generation = company_generation();
        let session = load_session.clone();
        let api = load_api.clone();
        let workforce_api = load_workforce_api.clone();
        async move {
            let Some((token, company_id)) = current_token_and_company(&session) else {
                return Err(AssessmentManagementApiError::AuthenticationRequired);
            };
            let employees = api
                .employees(&token, company_id, Some(&query), 100, None)
                .await?;
            let templates = api.templates(&token, company_id).await?;
            let assignments = api.assignments(&token, company_id, None, 100, None).await?;
            let venues = workforce_api
                .venues(&token, company_id)
                .await
                .map_err(|_| AssessmentManagementApiError::InternalError)?;
            Ok(ManagementData {
                company_id,
                lifecycle_epoch: operation_epoch,
                company_generation: operation_company_generation,
                employees,
                templates,
                assignments,
                venues,
            })
        }
    });

    let session_state = session.state();
    let companies = match &session_state {
        AccountSessionState::Authenticated(value) => value.bootstrap.companies.clone(),
        _ => Vec::new(),
    };
    let current_company = selected_company(&session_state);
    let current_data = data().and_then(|result| match result {
        Ok(value)
            if value.lifecycle_epoch == lifecycle_epoch()
                && value.company_generation == company_generation()
                && Some(value.company_id) == current_company =>
        {
            Some(Ok(value))
        }
        Ok(_) => None,
        Err(error) => Some(Err(error)),
    });

    let create_session = session.clone();
    let create_api = api.clone();
    let detail_session = session.clone();
    let detail_api = api.clone();
    let revoke_session = session.clone();
    let revoke_api = api.clone();

    rsx! {
        section { class: "management-page", aria_labelledby: "management-title",
            header { class: "management-hero",
                div {
                    p { class: "management-eyebrow", "RESTOS • УПРАВЛЕНИЕ ОЦЕНКАМИ" }
                    h1 { id: "management-title", "Назначения" }
                    p { "Назначайте опубликованные оценки сотрудникам и отслеживайте только безопасный прогресс выполнения." }
                }
                if companies.len() > 1 {
                    label { class: "management-company",
                        span { "Компания" }
                        select {
                            value: current_company.map(|id| id.to_string()).unwrap_or_default(),
                            onchange: move |event| {
                                let Ok(company_id) = Uuid::parse_str(&event.value()) else { return; };
                                if session.select_company(SelectedCompanyId(company_id)).is_err() { return; }
                                company_generation += 1;
                                operation_generation += 1;
                                selected_employee.set(None);
                                selected_template.set(None);
                                selected_venue.set(None);
                                detail.set(None);
                                confirming_create.set(false);
                                confirming_revoke.set(false);
                                mutation_error.set(None);
                                on_company_changed.call(());
                            },
                            option { value: "", disabled: true, "Выберите компанию" }
                            for company in companies.iter() {
                                option { key: "{company.company_id}", value: "{company.company_id}", "{company.company_name}" }
                            }
                        }
                    }
                }
            }

            div { class: "management-live", role: "status", aria_live: "polite",
                if let Some(message) = mutation_error() { "{message}" }
            }

            match current_data {
                None => rsx! { div { class: "management-state", role: "status", "Загружаем сотрудников, шаблоны и назначения..." } },
                Some(Err(error)) => rsx! {
                    div { class: "management-state management-error", role: "alert",
                        p { "{safe_management_error(&error)}" }
                        button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить загрузку" }
                    }
                },
                Some(Ok(data)) => rsx! {
                    div { class: "management-layout",
                        section { class: "management-panel management-create", aria_labelledby: "create-assignment-title",
                            h2 { id: "create-assignment-title", "Новое назначение" }
                            label {
                                span { "Поиск сотрудника" }
                                div { class: "management-search-row",
                                    input { value: "{search}", maxlength: 100, oninput: move |event| search.set(event.value()), placeholder: "Имя сотрудника" }
                                    button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Найти" }
                                }
                            }
                            label {
                                span { "Сотрудник" }
                                select {
                                    value: selected_employee()
                                        .and_then(|id| data.employees.iter().position(|employee| employee.employee_profile_id == id))
                                        .map(|index| index.to_string())
                                        .unwrap_or_default(),
                                    onchange: {
                                        let employees = data.employees.clone();
                                        move |event| {
                                            selected_employee.set(
                                                event.value().parse::<usize>().ok()
                                                    .and_then(|index| employees.get(index))
                                                    .map(|employee| employee.employee_profile_id),
                                            );
                                        }
                                    },
                                    option { value: "", "Выберите сотрудника" }
                                    for (index, employee) in data.employees.iter().enumerate() {
                                        option { key: "employee-{index}", value: "{index}",
                                            if let Some(position) = &employee.position_title { "{employee.display_name} — {position}" } else { "{employee.display_name}" }
                                        }
                                    }
                                }
                            }
                            if data.employees.is_empty() { p { class: "management-muted", "Активные сотрудники не найдены." } }
                            else if data.employees.len() == 100 { p { class: "management-muted", "Показаны первые 100 сотрудников. Уточните поиск." } }
                            label {
                                span { "Опубликованный шаблон" }
                                select {
                                    value: selected_template()
                                        .and_then(|id| data.templates.iter().position(|template| template.template_version_id == id))
                                        .map(|index| index.to_string())
                                        .unwrap_or_default(),
                                    onchange: {
                                        let templates = data.templates.clone();
                                        move |event| {
                                            selected_template.set(
                                                event.value().parse::<usize>().ok()
                                                    .and_then(|index| templates.get(index))
                                                    .map(|template| template.template_version_id),
                                            );
                                        }
                                    },
                                    option { value: "", "Выберите шаблон" }
                                    for (index, template) in data.templates.iter().enumerate() {
                                        option { key: "template-{index}", value: "{index}", "{template.name} • {activity_label(&template.activity_type)} • версия {template.version} • опубликован {template.published_at}" }
                                    }
                                }
                            }
                            if data.templates.is_empty() { p { class: "management-muted", "Нет опубликованных шаблонов для назначения." } }
                            label {
                                span { "Ресторан для аналитики (необязательно)" }
                                select {
                                    value: selected_venue().map(|value| value.to_string()).unwrap_or_default(),
                                    onchange: move |event| {
                                        selected_venue.set(Uuid::parse_str(&event.value()).ok());
                                    },
                                    option { value: "", "Без привязки к ресторану" }
                                    for venue in data.venues.iter() {
                                        option { key: "venue-{venue.venue_id}", value: "{venue.venue_id}", "{venue.name}" }
                                    }
                                }
                            }
                            label {
                                span { "Срок выполнения — ваше местное время (необязательно)" }
                                input { r#type: "datetime-local", value: "{due_at}", oninput: move |event| due_at.set(event.value()) }
                            }
                            if confirming_create() {
                                div { class: "management-confirm", role: "dialog", aria_modal: "true", aria_label: "Подтверждение назначения",
                                    p { "Подтвердите назначение выбранной версии оценки сотруднику." }
                                    button {
                                        class: "btn-primary", r#type: "button", disabled: creating(), aria_busy: creating(),
                                        onclick: move |_| {
                                            let due = local_due_to_utc(&due_at());
                                            if !create_is_admitted(selected_employee(), selected_template(), true, creating(), due.is_some()) {
                                                mutation_error.set(Some("Заполните сотрудника, шаблон и корректный будущий срок.".into()));
                                                return;
                                            }
                                            let Some((token, company_id)) = current_token_and_company(&create_session) else { return; };
                                            creating.set(true);
                                            mutation_error.set(None);
                                            operation_generation += 1;
                                            let generation = operation_generation();
                                            let epoch = lifecycle_epoch();
                                            let company_scope = company_generation();
                                            let request = CreateAssignmentRequest {
                                                employee_profile_id: selected_employee().unwrap_or_default(),
                                                template_version_id: selected_template().unwrap_or_default(),
                                                venue_id: selected_venue(),
                                                due_at: due.filter(|value| !value.is_empty()),
                                            };
                                            let api = create_api.clone();
                                            let session = create_session.clone();
                                            spawn(async move {
                                                let result = api.create_assignment(&token, company_id, &request).await;
                                                if !scoped_result_is_current(lifecycle_epoch(), epoch, company_generation(), company_scope, selected_company(&session.state()), company_id, operation_generation(), generation) { return; }
                                                creating.set(false);
                                                match result {
                                                    Ok(value) => { detail.set(Some(value)); confirming_create.set(false); selected_employee.set(None); selected_template.set(None); selected_venue.set(None); due_at.set(String::new()); reload += 1; mutation_error.set(Some("Оценка назначена.".into())); },
                                                    Err(problem) => mutation_error.set(Some(safe_management_error(&problem).into())),
                                                }
                                            });
                                        },
                                        if creating() { "Назначение..." } else if mutation_error().is_some() { "Повторить создание" } else { "Подтвердить" }
                                    }
                                    button { class: "btn-ghost", r#type: "button", disabled: creating(), onclick: move |_| confirming_create.set(false), "Отмена" }
                                }
                            } else {
                                button {
                                    class: "btn-primary", r#type: "button", disabled: selected_employee().is_none() || selected_template().is_none(),
                                    onclick: move |_| { mutation_error.set(None); confirming_create.set(true); },
                                    "Назначить оценку"
                                }
                            }
                        }

                        section { class: "management-panel management-list", aria_labelledby: "assignment-list-title",
                            h2 { id: "assignment-list-title", "Текущие назначения" }
                            if data.assignments.is_empty() {
                                div { class: "management-empty", "Назначений пока нет." }
                            } else {
                                div { class: "management-cards",
                                    for assignment in data.assignments.iter() {
                                        article { key: "{assignment.id}", class: "management-card",
                                            div { class: "management-card-head",
                                                div { strong { "{assignment.employee.display_name}" } if let Some(position) = &assignment.employee.position_title { small { "{position}" } } }
                                                span { class: "management-status", "{status_label(assignment.status)}" }
                                            }
                                            h3 { "{assignment.template.name}" }
                                            p { "Версия {assignment.template.version} • отвечено {assignment.progress.answered_count} из {assignment.progress.total_count}" }
                                            small { "Назначено: {assignment.assigned_at}" }
                                            if let Some(due) = &assignment.due_at { small { "Срок: {due}" } }
                                            button {
                                                class: "btn-secondary", r#type: "button", disabled: detail_loading(),
                                                onclick: {
                                                    let assignment_id = assignment.id;
                                                    let api = detail_api.clone();
                                                    let session = detail_session.clone();
                                                    move |_| {
                                                        let Some((token, company_id)) = current_token_and_company(&session) else { return; };
                                                        detail_loading.set(true);
                                                        operation_generation += 1;
                                                        let generation = operation_generation();
                                                        let epoch = lifecycle_epoch();
                                                        let company_scope = company_generation();
                                                        let api = api.clone();
                                                        let session = session.clone();
                                                        spawn(async move {
                                                            let result = api.assignment(&token, company_id, assignment_id).await;
                                                            if !scoped_result_is_current(lifecycle_epoch(), epoch, company_generation(), company_scope, selected_company(&session.state()), company_id, operation_generation(), generation) { return; }
                                                            detail_loading.set(false);
                                                            match result { Ok(value) => { detail.set(Some(value)); mutation_error.set(None); }, Err(problem) => mutation_error.set(Some(safe_management_error(&problem).into())) }
                                                        });
                                                    }
                                                },
                                                if detail_loading() { "Загрузка..." } else { "Открыть" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(current) = detail() {
                        section { class: "management-panel management-detail", aria_labelledby: "assignment-detail-title",
                            div { class: "management-detail-head",
                                div { h2 { id: "assignment-detail-title", "{current.template.name}" } p { "{current.employee.display_name} • {status_label(current.status)}" } }
                                button { class: "btn-ghost", r#type: "button", onclick: move |_| { detail.set(None); confirming_revoke.set(false); }, "Закрыть" }
                            }
                            dl { class: "management-progress",
                                div { dt { "Отвечено" } dd { "{current.progress.answered_count}" } }
                                div { dt { "Обязательно" } dd { "{current.progress.required_count}" } }
                                div { dt { "Всего" } dd { "{current.progress.total_count}" } }
                            }
                            div { class: "management-timeline",
                                if let Some(started) = &current.progress.started_at { small { "Начато: {started}" } }
                                if let Some(saved) = &current.progress.last_saved_at { small { "Сохранено: {saved}" } }
                                if let Some(submitted) = &current.progress.submitted_at { small { "Отправлено: {submitted}" } }
                            }
                            if let Some(receipt) = &current.progress.completion {
                                p { class: "management-completion", "Выполнение подтверждено • {receipt.submitted_at}" }
                            }
                            if matches!(current.status, AssignmentStatus::Assigned | AssignmentStatus::InProgress) {
                                if confirming_revoke() {
                                    div { class: "management-confirm", role: "dialog", aria_modal: "true", aria_label: "Подтверждение отзыва",
                                        p { "Отозвать назначение? Сохранённые данные не удаляются, сотрудник увидит режим только для чтения." }
                                        button {
                                            class: "btn-danger", r#type: "button", disabled: revoking(), aria_busy: revoking(),
                                            aria_label: "Подтвердить отзыв назначения. Необратимое действие",
                                            onclick: {
                                                let assignment_id = current.id;
                                                let status = current.status;
                                                let api = revoke_api.clone();
                                                let session = revoke_session.clone();
                                                move |_| {
                                                    if !revoke_is_admitted(status, true, revoking()) { return; }
                                                    let Some((token, company_id)) = current_token_and_company(&session) else { return; };
                                                    revoking.set(true);
                                                    mutation_error.set(None);
                                                    operation_generation += 1;
                                                    let generation = operation_generation();
                                                    let epoch = lifecycle_epoch();
                                                    let company_scope = company_generation();
                                                    let api = api.clone();
                                                    let session = session.clone();
                                                    spawn(async move {
                                                        let result = api.revoke(&token, company_id, assignment_id).await;
                                                        if !scoped_result_is_current(lifecycle_epoch(), epoch, company_generation(), company_scope, selected_company(&session.state()), company_id, operation_generation(), generation) { return; }
                                                        revoking.set(false);
                                                        match result {
                                                            Ok(value) => { detail.set(Some(value)); confirming_revoke.set(false); reload += 1; mutation_error.set(Some("Назначение отозвано.".into())); },
                                                            Err(problem) => mutation_error.set(Some(safe_management_error(&problem).into())),
                                                        }
                                                    });
                                                }
                                            },
                                            span { class: "semantic-button-icon", aria_hidden: "true", "!" }
                                            if revoking() { "Отзыв..." } else if mutation_error().is_some() { "Повторить отзыв" } else { "Подтвердить отзыв" }
                                        }
                                        button { class: "btn-ghost", r#type: "button", disabled: revoking(), onclick: move |_| confirming_revoke.set(false), "Отмена" }
                                    }
                                } else {
                                    button { class: "btn-danger", r#type: "button", aria_label: "Отозвать назначение. Необратимое действие", onclick: move |_| { mutation_error.set(None); confirming_revoke.set(true); },
                                        span { class: "semantic-button-icon", aria_hidden: "true", "!" }
                                        "Отозвать назначение"
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_api::BootstrapCompany;
    use wasm_bindgen_test::wasm_bindgen_test;

    const COMPANY: Uuid = Uuid::from_u128(1);
    const OTHER: Uuid = Uuid::from_u128(2);
    const EMPLOYEE: Uuid = Uuid::from_u128(3);
    const TEMPLATE: Uuid = Uuid::from_u128(4);

    fn company(relationship: &str) -> BootstrapCompany {
        BootstrapCompany {
            company_id: COMPANY,
            company_name: "Компания".into(),
            employee_profile_id: None,
            relationship: relationship.into(),
        }
    }

    #[wasm_bindgen_test]
    fn stage23e_owner_visible_immediately_and_employee_requires_probe() {
        assert_eq!(
            bootstrap_owner_capability(&[company("owner")], Some(COMPANY)),
            ManagerCapability::Authorized
        );
        assert_eq!(
            bootstrap_owner_capability(&[company("employee")], Some(COMPANY)),
            ManagerCapability::Checking
        );
        assert_eq!(
            bootstrap_owner_capability(&[company("owner")], Some(OTHER)),
            ManagerCapability::Checking
        );
    }

    #[wasm_bindgen_test]
    fn stage23e_probe_distinguishes_manager_employee_and_network_error() {
        assert_eq!(
            capability_from_probe(&Ok(Vec::new())),
            ManagerCapability::Authorized
        );
        assert_eq!(
            capability_from_probe(&Err(AssessmentManagementApiError::PermissionDenied)),
            ManagerCapability::Hidden
        );
        assert_eq!(
            capability_from_probe(&Err(AssessmentManagementApiError::NetworkUnavailable)),
            ManagerCapability::Error
        );
    }

    #[wasm_bindgen_test]
    fn stage23e_scope_rejects_company_logout_and_generation_staleness() {
        assert!(scoped_result_is_current(
            1,
            1,
            2,
            2,
            Some(COMPANY),
            COMPANY,
            3,
            3
        ));
        assert!(!scoped_result_is_current(
            2,
            1,
            2,
            2,
            Some(COMPANY),
            COMPANY,
            3,
            3
        ));
        assert!(!scoped_result_is_current(
            1,
            1,
            3,
            2,
            Some(COMPANY),
            COMPANY,
            3,
            3
        ));
        assert!(!scoped_result_is_current(
            1,
            1,
            2,
            2,
            Some(OTHER),
            COMPANY,
            3,
            3
        ));
        assert!(!scoped_result_is_current(1, 1, 2, 2, None, COMPANY, 3, 3));
    }

    #[wasm_bindgen_test]
    fn stage23e_create_requires_confirmation_fields_and_single_flight() {
        assert!(create_is_admitted(
            Some(EMPLOYEE),
            Some(TEMPLATE),
            true,
            false,
            true
        ));
        assert!(!create_is_admitted(None, Some(TEMPLATE), true, false, true));
        assert!(!create_is_admitted(
            Some(EMPLOYEE),
            Some(TEMPLATE),
            false,
            false,
            true
        ));
        assert!(!create_is_admitted(
            Some(EMPLOYEE),
            Some(TEMPLATE),
            true,
            true,
            true
        ));
        assert!(!create_is_admitted(
            Some(EMPLOYEE),
            Some(TEMPLATE),
            true,
            false,
            false
        ));
    }

    #[wasm_bindgen_test]
    fn stage23e_due_boundary_is_finite_strictly_future_and_optional() {
        assert!(due_millis_is_valid(1000.0, None));
        assert!(!due_millis_is_valid(1000.0, Some(1000.0)));
        assert!(due_millis_is_valid(1000.0, Some(1000.1)));
        assert!(!due_millis_is_valid(f64::NAN, Some(2000.0)));
        assert!(!due_millis_is_valid(1000.0, Some(f64::INFINITY)));
    }

    #[wasm_bindgen_test]
    fn stage23e_revoke_is_bounded_to_mutable_confirmed_single_flight() {
        assert!(revoke_is_admitted(AssignmentStatus::Assigned, true, false));
        assert!(revoke_is_admitted(
            AssignmentStatus::InProgress,
            true,
            false
        ));
        assert!(!revoke_is_admitted(
            AssignmentStatus::Completed,
            true,
            false
        ));
        assert!(!revoke_is_admitted(AssignmentStatus::Revoked, true, false));
        assert!(!revoke_is_admitted(
            AssignmentStatus::Assigned,
            false,
            false
        ));
        assert!(!revoke_is_admitted(AssignmentStatus::Assigned, true, true));
    }

    #[wasm_bindgen_test]
    fn stage23e_safe_labels_are_bounded_and_never_echo_input() {
        assert_eq!(status_label(AssignmentStatus::Completed), "Завершена");
        assert_eq!(activity_label("survey"), "Опрос");
        assert_eq!(activity_label("private-value"), "Шаблон");
        assert_eq!(
            safe_management_error(&AssessmentManagementApiError::DuplicateActiveAssignment),
            "Эта версия оценки уже назначена сотруднику."
        );
    }

    #[wasm_bindgen_test]
    fn stage23e_network_errors_require_explicit_user_action() {
        assert_eq!(
            capability_from_probe(&Err(AssessmentManagementApiError::NetworkUnavailable)),
            ManagerCapability::Error
        );
        assert!(!create_is_admitted(
            Some(EMPLOYEE),
            Some(TEMPLATE),
            false,
            false,
            true
        ));
        assert!(!revoke_is_admitted(
            AssignmentStatus::Assigned,
            false,
            false
        ));
    }
}
