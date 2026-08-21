//! Unified manager entry point for employee KЛН, operational walkthroughs and product measurements.

use dioxus::prelude::*;
use uuid::Uuid;

use super::ProductMeasurementPanel;
use crate::{
    account_api::AccountAccessToken,
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_attempt_api::AttemptDocument,
    assessment_management_api::{
        AssessmentManagementApiClient, AssessmentManagementApiError, AssignableTemplate,
        ManagementEmployee, StartManagerMeasurementRequest,
    },
    operational_walkthrough_api::{
        OperationalWalkthroughApiClient, OperationalWalkthroughTemplate,
    },
    workforce_api::{WorkforceApiClient, WorkforceVenue},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeasurementKind {
    Employee,
    Walkthrough,
    Product,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartFailureAction {
    Retry,
    RefreshSession,
    RefreshChoices,
    None,
}

fn start_failure(error: &AssessmentManagementApiError) -> (&'static str, StartFailureAction) {
    match error {
        AssessmentManagementApiError::AuthenticationRequired => (
            "Сессия истекла. Обновите сессию и повторите действие.",
            StartFailureAction::RefreshSession,
        ),
        AssessmentManagementApiError::PermissionDenied => (
            "Недостаточно прав для запуска этого замера.",
            StartFailureAction::None,
        ),
        AssessmentManagementApiError::NotFound => (
            "Сотрудник, методика или ресторан больше недоступны. Обновите варианты и выберите совместимую комбинацию.",
            StartFailureAction::RefreshChoices,
        ),
        AssessmentManagementApiError::DuplicateActiveAssignment => (
            "У сотрудника уже есть активный замер по этой методике. Обновите список, чтобы открыть его.",
            StartFailureAction::RefreshChoices,
        ),
        AssessmentManagementApiError::AlreadyCompleted
        | AssessmentManagementApiError::StateConflict => (
            "Состояние замера изменилось. Обновите доступные варианты.",
            StartFailureAction::RefreshChoices,
        ),
        AssessmentManagementApiError::InvalidRequest => (
            "Проверьте выбранного сотрудника, методику и ресторан.",
            StartFailureAction::RefreshChoices,
        ),
        AssessmentManagementApiError::NetworkUnavailable => (
            "Нет связи с сервером. Проверьте подключение и повторите.",
            StartFailureAction::Retry,
        ),
        AssessmentManagementApiError::InternalError => (
            "Сервис временно недоступен. Повторите действие вручную.",
            StartFailureAction::Retry,
        ),
    }
}

fn employee_allows_venue(
    employees: &[ManagementEmployee],
    employee_id: Uuid,
    venue_id: Uuid,
) -> bool {
    employees
        .iter()
        .find(|employee| employee.employee_profile_id == employee_id)
        .is_some_and(|employee| employee.venue_ids.contains(&venue_id))
}

#[cfg(target_arch = "wasm32")]
fn begin_manager_measurement(
    api: AssessmentManagementApiClient,
    token: AccountAccessToken,
    company_id: Uuid,
    request: StartManagerMeasurementRequest,
    lifecycle_epoch: Signal<u64>,
    mut busy: Signal<bool>,
    mut start_error: Signal<Option<AssessmentManagementApiError>>,
    on_attempt: EventHandler<AttemptDocument>,
) {
    let epoch = lifecycle_epoch();
    busy.set(true);
    start_error.set(None);
    spawn(async move {
        let result = api.start_measurement(&token, company_id, &request).await;
        if lifecycle_epoch() != epoch {
            return;
        }
        busy.set(false);
        match result {
            Ok(value) => on_attempt.call(value),
            Err(problem) => start_error.set(Some(problem)),
        }
    });
}

#[derive(Clone, Debug, PartialEq)]
struct LauncherData {
    company_id: Uuid,
    employees: Vec<ManagementEmployee>,
    employee_templates: Vec<AssignableTemplate>,
    walkthroughs: Vec<OperationalWalkthroughTemplate>,
    venues: Vec<WorkforceVenue>,
}

#[component]
pub fn MeasurementLauncher(
    on_cancel: EventHandler<()>,
    on_attempt: EventHandler<AttemptDocument>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let management = use_context::<AssessmentManagementApiClient>();
    let walkthrough = use_context::<OperationalWalkthroughApiClient>();
    let workforce = use_context::<WorkforceApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut kind = use_signal(|| None::<MeasurementKind>);
    let mut employee_id = use_signal(|| None::<Uuid>);
    let mut template_id = use_signal(|| None::<Uuid>);
    let mut venue_id = use_signal(|| None::<Uuid>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut start_error = use_signal(|| None::<AssessmentManagementApiError>);
    let mut reload = use_signal(|| 0_u64);
    let start_management = management.clone();
    let start_walkthrough = walkthrough.clone();

    let load_session = session.clone();
    let data = use_resource(move || {
        let _reload = reload();
        let state = load_session.state();
        let management = management.clone();
        let walkthrough = walkthrough.clone();
        let workforce = workforce.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Err("Сессия недоступна. Войдите снова.".to_string());
            };
            let Some(company_id) = account.selected_company.map(|value| value.0) else {
                return Err("Выберите компанию.".to_string());
            };
            let employees = management
                .employees(&account.access_token, company_id, None, 100, None)
                .await
                .map_err(|_| "Не удалось загрузить сотрудников.".to_string())?;
            let employee_templates = management
                .templates(&account.access_token, company_id)
                .await
                .map_err(|_| "Не удалось загрузить шаблоны КЛН.".to_string())?;
            let walkthroughs = walkthrough
                .templates(&account.access_token, company_id)
                .await
                .map_err(|_| "Не удалось загрузить шаблоны обходов.".to_string())?;
            let venues = workforce
                .venues(&account.access_token, company_id)
                .await
                .map_err(|_| "Не удалось загрузить рестораны.".to_string())?;
            Ok(LauncherData {
                company_id,
                employees,
                employee_templates,
                walkthroughs,
                venues,
            })
        }
    });

    rsx! {
        section { class: "measurement-launcher", aria_labelledby: "measurement-launcher-title",
            header { class: "measurement-launcher-head",
                div {
                    p { class: "management-eyebrow", "НОВЫЙ ЗАМЕР" }
                    h1 { id: "measurement-launcher-title", "Что будем измерять?" }
                    p { "Выберите методику — RestOS откроет соответствующий рабочий бланк." }
                }
                button { class: "btn-ghost", r#type: "button", disabled: busy(), onclick: move |_| on_cancel.call(()), "Назад" }
            }
            div { class: "measurement-kind-grid",
                button { class: if kind() == Some(MeasurementKind::Employee) { "measurement-kind active" } else { "measurement-kind" }, r#type: "button", onclick: move |_| { kind.set(Some(MeasurementKind::Employee)); error.set(None); start_error.set(None); }, strong { "КЛН сотрудника" } span { "Выберите сотрудника и форму оценки" } }
                button { class: if kind() == Some(MeasurementKind::Walkthrough) { "measurement-kind active" } else { "measurement-kind" }, r#type: "button", onclick: move |_| { kind.set(Some(MeasurementKind::Walkthrough)); error.set(None); start_error.set(None); }, strong { "Обход ресторана" } span { "Сервис или производство" } }
                button { class: if kind() == Some(MeasurementKind::Product) { "measurement-kind active" } else { "measurement-kind" }, r#type: "button", onclick: move |_| { kind.set(Some(MeasurementKind::Product)); error.set(None); start_error.set(None); }, strong { "Вкус и скорость" } span { "Повторяемый замер блюд и напитков" } }
            }
            div { class: "management-live", role: "status", aria_live: "polite", if let Some(message) = error() { "{message}" } }
            match data() {
                None => rsx! { p { class: "management-state", "Загружаем доступные методики..." } },
                Some(Err(problem)) => rsx! { div { class: "management-state management-error", p { "{problem}" } button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" } } },
                Some(Ok(data)) => match kind() {
                    None => rsx! { p { class: "measurement-launcher-hint", "Выберите один из трёх типов замера." } },
                    Some(MeasurementKind::Product) => rsx! { ProductMeasurementPanel {} },
                    Some(MeasurementKind::Employee) => {
                        let employees_for_change = data.employees.clone();
                        let compatible_venues = employee_id()
                            .and_then(|selected| data.employees.iter().find(|employee| employee.employee_profile_id == selected))
                            .map(|employee| employee.venue_ids.clone())
                            .unwrap_or_default();
                        let venue_required = employee_id()
                            .and_then(|selected| data.employees.iter().find(|employee| employee.employee_profile_id == selected))
                            .is_some_and(|employee| employee.venue_required);
                        rsx! {
                        section { class: "measurement-launcher-form", aria_label: "Настройка КЛН сотрудника",
                            h2 { "КЛН сотрудника" }
                            label { span { "Сотрудник" } select { value: employee_id().map(|value| value.to_string()).unwrap_or_default(), onchange: move |event| {
                                let selected = Uuid::parse_str(&event.value()).ok();
                                if let (Some(employee), Some(venue)) = (selected, venue_id()) {
                                    if !employee_allows_venue(&employees_for_change, employee, venue) {
                                        venue_id.set(None);
                                    }
                                }
                                employee_id.set(selected);
                                start_error.set(None);
                            }, option { value: "", "Выберите сотрудника" } for employee in data.employees.iter() { option { value: "{employee.employee_profile_id}", "{employee.display_name}" } } } }
                            label { span { "Методика" } select { value: template_id().map(|value| value.to_string()).unwrap_or_default(), onchange: move |event| template_id.set(Uuid::parse_str(&event.value()).ok()), option { value: "", "Выберите КЛН" } for template in data.employee_templates.iter() { option { value: "{template.template_version_id}", "{template.name}" } } } }
                            label { span { if venue_required { "Ресторан" } else { "Ресторан (необязательно)" } } select { value: venue_id().map(|value| value.to_string()).unwrap_or_default(), onchange: move |event| { venue_id.set(Uuid::parse_str(&event.value()).ok()); start_error.set(None); }, option { value: "", if venue_required { "Выберите ресторан" } else { "Без привязки" } } for venue in data.venues.iter().filter(|venue| compatible_venues.contains(&venue.venue_id)) { option { value: "{venue.venue_id}", "{venue.name}" } } } }
                            if employee_id().is_some() && compatible_venues.is_empty() && !venue_required {
                                p { class: "management-state", "Сотрудник не привязан к ресторану. Замер можно открыть без привязки." }
                            }
                            button { class: "btn-primary", r#type: "button", disabled: busy() || employee_id().is_none() || template_id().is_none() || (venue_required && venue_id().is_none()), onclick: {
                                let session = session.clone();
                                let api = start_management.clone();
                                let company_id = data.company_id;
                                move |_| {
                                    let AccountSessionState::Authenticated(account) = session.state() else { return; };
                                    let (Some(employee), Some(template)) = (employee_id(), template_id()) else { return; };
                                    let request = StartManagerMeasurementRequest { employee_profile_id: employee, template_version_id: template, venue_id: venue_id() };
                                    let api = api.clone();
                                    begin_manager_measurement(api, account.access_token, company_id, request, lifecycle_epoch, busy, start_error, on_attempt);
                                }
                            }, if busy() { "Открываем форму..." } else { "Перейти к замеру" } }
                            if let Some(problem) = start_error() {
                                {
                                    let (message, action) = start_failure(&problem);
                                    rsx! {
                                        div { class: "management-state management-error", role: "alert",
                                            p { "{message}" }
                                            match action {
                                                StartFailureAction::Retry => rsx! { button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: {
                                                    let session = session.clone();
                                                    let api = start_management.clone();
                                                    let company_id = data.company_id;
                                                    move |_| {
                                                        let AccountSessionState::Authenticated(account) = session.state() else { start_error.set(Some(AssessmentManagementApiError::AuthenticationRequired)); return; };
                                                        let (Some(employee), Some(template)) = (employee_id(), template_id()) else { return; };
                                                        let request = StartManagerMeasurementRequest { employee_profile_id: employee, template_version_id: template, venue_id: venue_id() };
                                                        begin_manager_measurement(api.clone(), account.access_token, company_id, request, lifecycle_epoch, busy, start_error, on_attempt);
                                                    }
                                                }, "Повторить" } },
                                                StartFailureAction::RefreshSession => rsx! { button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: {
                                                    let refresh_session = session.clone();
                                                    move |_| {
                                                        let refresh_session = refresh_session.clone();
                                                        let epoch = lifecycle_epoch();
                                                        busy.set(true);
                                                        spawn(async move {
                                                            let refreshed = refresh_session.refresh().await;
                                                            if lifecycle_epoch() != epoch { return; }
                                                            busy.set(false);
                                                            match refreshed {
                                                                Ok(_) => { start_error.set(None); reload += 1; }
                                                                Err(_) => start_error.set(Some(AssessmentManagementApiError::AuthenticationRequired)),
                                                            }
                                                        });
                                                    }
                                                }, "Обновить сессию" } },
                                                StartFailureAction::RefreshChoices => rsx! { button { class: "btn-secondary", r#type: "button", onclick: move |_| { employee_id.set(None); template_id.set(None); venue_id.set(None); start_error.set(None); reload += 1; }, "Обновить варианты" } },
                                                StartFailureAction::None => rsx! {},
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    },
                    Some(MeasurementKind::Walkthrough) => rsx! {
                        section { class: "measurement-launcher-form", aria_label: "Настройка обхода",
                            h2 { "Обход ресторана" }
                            label { span { "Ресторан" } select { value: venue_id().map(|value| value.to_string()).unwrap_or_default(), onchange: move |event| venue_id.set(Uuid::parse_str(&event.value()).ok()), option { value: "", "Выберите ресторан" } for venue in data.venues.iter() { option { value: "{venue.venue_id}", "{venue.name}" } } } }
                            label { span { "Методика" } select { value: template_id().map(|value| value.to_string()).unwrap_or_default(), onchange: move |event| template_id.set(Uuid::parse_str(&event.value()).ok()), option { value: "", "Выберите обход" } for template in data.walkthroughs.iter() { option { value: "{template.template_version_id}", "{template.name}" } } } }
                            button { class: "btn-primary", r#type: "button", disabled: busy() || venue_id().is_none() || template_id().is_none(), onclick: {
                                let session = session.clone();
                                let api = start_walkthrough.clone();
                                let company_id = data.company_id;
                                move |_| {
                                    let AccountSessionState::Authenticated(account) = session.state() else { return; };
                                    let (Some(venue), Some(template)) = (venue_id(), template_id()) else { return; };
                                    let api = api.clone();
                                    let epoch = lifecycle_epoch();
                                    busy.set(true); error.set(None);
                                    spawn(async move {
                                        let result = api.start(&account.access_token, company_id, venue, template).await;
                                        if lifecycle_epoch() != epoch { return; }
                                        busy.set(false);
                                        match result { Ok(value) => on_attempt.call(value), Err(_) => error.set(Some("Не удалось открыть форму обхода. Повторите вручную.".into())) }
                                    });
                                }
                            }, if busy() { "Открываем форму..." } else { "Перейти к обходу" } }
                        }
                    },
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    fn employee(venue_ids: Vec<Uuid>) -> ManagementEmployee {
        ManagementEmployee {
            employee_profile_id: Uuid::from_u128(1),
            display_name: "Synthetic employee".into(),
            position_title: None,
            status: "active".into(),
            venue_ids,
            venue_required: false,
        }
    }

    #[wasm_bindgen_test]
    fn employee_venue_compatibility_is_exact_and_unassigned_is_not_fabricated() {
        let allowed = Uuid::from_u128(2);
        let other = Uuid::from_u128(3);
        let employees = vec![employee(vec![allowed])];
        assert!(employee_allows_venue(
            &employees,
            Uuid::from_u128(1),
            allowed
        ));
        assert!(!employee_allows_venue(
            &employees,
            Uuid::from_u128(1),
            other
        ));
        assert!(!employee_allows_venue(
            &[employee(vec![])],
            Uuid::from_u128(1),
            allowed
        ));
    }

    #[wasm_bindgen_test]
    fn start_failure_actions_are_safe_and_retry_is_bounded() {
        assert_eq!(
            start_failure(&AssessmentManagementApiError::NetworkUnavailable).1,
            StartFailureAction::Retry
        );
        assert_eq!(
            start_failure(&AssessmentManagementApiError::AuthenticationRequired).1,
            StartFailureAction::RefreshSession
        );
        assert_eq!(
            start_failure(&AssessmentManagementApiError::NotFound).1,
            StartFailureAction::RefreshChoices
        );
        assert_eq!(
            start_failure(&AssessmentManagementApiError::PermissionDenied).1,
            StartFailureAction::None
        );
    }
}
