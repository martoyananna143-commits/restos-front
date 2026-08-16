//! Owner-only organization settings for Venues and explicit employee access profiles.

use std::collections::BTreeSet;

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_api::AccountAccessToken,
    account_session::{AccountSessionAdapter, AccountSessionState},
    organization_access_api::{
        CreateOrganizationVenueRequest, OrganizationAccessApiClient, OrganizationAccessApiError,
        OrganizationAccessProfile, OrganizationEmployeeAccess, OrganizationVenue,
        ReplaceOrganizationAccessRequest, ReplaceOrganizationPositionRequest,
    },
    organization_workflow_api::{
        AccessPreset, CreateGroupInvitationRequest, CreatePositionRequest, GroupInvitation,
        GroupRegistration, OrganizationPosition, OrganizationWorkflowApiClient,
        OrganizationWorkflowApiError, GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED,
    },
};

use super::WorkforceOnboardingPage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrganizationSettingsSection {
    Employees,
    Venues,
    Positions,
    Invitations,
}

fn current_scope(session: &AccountSessionAdapter) -> Option<(AccountAccessToken, Uuid)> {
    let AccountSessionState::Authenticated(account) = session.state() else {
        return None;
    };
    Some((account.access_token, account.selected_company?.0))
}

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    if crypto.is_null() || crypto.is_undefined() {
        return None;
    }
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    Uuid::parse_str(&function.call0(&crypto).ok()?.as_string()?).ok()
}

fn normalized_venue_name(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty() && normalized.chars().count() <= 255).then_some(normalized)
}

fn safe_error(error: &OrganizationAccessApiError) -> &'static str {
    match error {
        OrganizationAccessApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        OrganizationAccessApiError::PermissionDenied | OrganizationAccessApiError::NotFound => {
            "Настройки организации недоступны для вашей роли."
        }
        OrganizationAccessApiError::Conflict => {
            "Доступ уже изменился. Обновите список и повторите действие."
        }
        OrganizationAccessApiError::InvalidRequest => "Проверьте выбранный профиль и рестораны.",
        OrganizationAccessApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OrganizationAccessApiError::InternalError => "Не удалось загрузить настройки организации.",
    }
}

fn safe_workflow_error(error: &OrganizationWorkflowApiError) -> &'static str {
    match error {
        OrganizationWorkflowApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        OrganizationWorkflowApiError::NotFound => "Раздел недоступен для вашей роли.",
        OrganizationWorkflowApiError::Conflict => "Данные уже изменились. Обновите список.",
        OrganizationWorkflowApiError::InvalidRequest => "Проверьте заполненные поля.",
        OrganizationWorkflowApiError::Unavailable => "Действие больше недоступно.",
        OrganizationWorkflowApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OrganizationWorkflowApiError::InternalError => "Не удалось выполнить действие.",
    }
}

fn invitation_expiry_iso() -> String {
    let value = js_sys::Date::new_0();
    value.set_time(value.get_time() + 7.0 * 24.0 * 60.0 * 60.0 * 1000.0);
    value.to_iso_string().as_string().unwrap_or_default()
}

fn profile_from_wire(value: &str) -> Option<OrganizationAccessProfile> {
    match value {
        "employee_unassigned" => Some(OrganizationAccessProfile::EmployeeUnassigned),
        "employee_venue" => Some(OrganizationAccessProfile::EmployeeVenue),
        "venue_manager" => Some(OrganizationAccessProfile::VenueManager),
        "organization_manager" => Some(OrganizationAccessProfile::OrganizationManager),
        _ => None,
    }
}

fn access_selection_valid(
    profile: OrganizationAccessProfile,
    venue_count: usize,
    editable: bool,
) -> bool {
    editable
        && match profile {
            OrganizationAccessProfile::EmployeeUnassigned
            | OrganizationAccessProfile::OrganizationManager => venue_count == 0,
            OrganizationAccessProfile::EmployeeVenue => venue_count == 1,
            OrganizationAccessProfile::VenueManager => venue_count >= 1,
            OrganizationAccessProfile::Owner | OrganizationAccessProfile::Unsupported => false,
        }
}

#[component]
pub fn OrganizationSettingsPage(
    section: OrganizationSettingsSection,
    on_section_change: EventHandler<OrganizationSettingsSection>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationAccessApiClient>();
    let workflow_api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut reload = use_signal(|| 0_u64);

    let venue_session = session.clone();
    let venue_api = api.clone();
    let venues = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = venue_session.clone();
        let api = venue_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationAccessApiError::AuthenticationRequired);
            };
            let result = api.venues(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationAccessApiError::AuthenticationRequired);
            }
            result
        }
    });

    let employee_session = session.clone();
    let employee_api = api.clone();
    let employees = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = employee_session.clone();
        let api = employee_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationAccessApiError::AuthenticationRequired);
            };
            let result = api.employees(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationAccessApiError::AuthenticationRequired);
            }
            result
        }
    });

    let workflow_session = session.clone();
    let position_api = workflow_api.clone();
    let positions = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = workflow_session.clone();
        let api = position_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            let result = api.positions(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            }
            result
        }
    });

    let invitation_session = session.clone();
    let invitation_api = workflow_api.clone();
    let invitations = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = invitation_session.clone();
        let api = invitation_api.clone();
        async move {
            if !GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED {
                return Ok(Vec::new());
            }
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            let result = api.group_invitations(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            }
            result
        }
    });

    let pending_session = session.clone();
    let pending_api = workflow_api.clone();
    let pending = use_resource(move || {
        let _reload = reload();
        let epoch = lifecycle_epoch();
        let session = pending_session.clone();
        let api = pending_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            let result = api.pending_registrations(&token, company_id).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            }
            result
        }
    });
    let preset_api = workflow_api.clone();
    let preset_session = session.clone();
    let presets = use_resource(move || {
        let _reload = reload();
        let api = preset_api.clone();
        let session = preset_session.clone();
        async move {
            let Some((token, _)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            api.access_presets(&token).await
        }
    });

    rsx! {
        section { class: "journey-page organization-settings", aria_labelledby: "organization-settings-title",
            header { class: "journey-hero",
                p { class: "management-eyebrow", "ОРГАНИЗАЦИЯ" }
                h1 { id: "organization-settings-title", "Настройка организации" }
                p { "Владелец управляет ресторанами и явным доступом сотрудников. Кадровая подчинённость не создаётся." }
            }
            nav { class: "journey-segments", aria_label: "Настройки организации",
                button { class: if section == OrganizationSettingsSection::Employees { "active" } else { "" }, r#type: "button", aria_current: if section == OrganizationSettingsSection::Employees { "page" } else { "false" }, onclick: move |_| on_section_change.call(OrganizationSettingsSection::Employees), "Сотрудники" }
                button { class: if section == OrganizationSettingsSection::Venues { "active" } else { "" }, r#type: "button", aria_current: if section == OrganizationSettingsSection::Venues { "page" } else { "false" }, onclick: move |_| on_section_change.call(OrganizationSettingsSection::Venues), "Рестораны" }
                button { class: if section == OrganizationSettingsSection::Positions { "active" } else { "" }, r#type: "button", aria_current: if section == OrganizationSettingsSection::Positions { "page" } else { "false" }, onclick: move |_| on_section_change.call(OrganizationSettingsSection::Positions), "Должности и доступ" }
                button { class: if section == OrganizationSettingsSection::Invitations { "active" } else { "" }, r#type: "button", aria_current: if section == OrganizationSettingsSection::Invitations { "page" } else { "false" }, onclick: move |_| on_section_change.call(OrganizationSettingsSection::Invitations), "Приглашения" }
            }

            match section {
                OrganizationSettingsSection::Employees => rsx! {
                    WorkforceOnboardingPage {}
                    article { class: "journey-panel organization-access-panel",
                        h2 { "Профили и доступ" }
                        p { class: "journey-muted", "Выберите один production-профиль. Изменение применяется только после явного подтверждения." }
                        EmployeeAccessList {
                            employees: employees(),
                            venues: venues(),
                            positions: positions(),
                            on_reload: move |_| reload += 1,
                        }
                    }
                    article { class: "journey-panel",
                        h2 { "Активные сотрудники" }
                        p { class: "journey-muted", "Организация → профиль доступа → рестораны. Кадровая подчинённость не создаётся." }
                        AccessStructure { employees: employees(), venues: venues() }
                    }
                    PendingRegistrationList { pending: pending(), on_reload: move |_| reload += 1 }
                },
                OrganizationSettingsSection::Venues => rsx! {
                    article { class: "journey-panel",
                        h2 { "Рестораны" }
                        p { class: "journey-muted", "Новый ресторан становится доступен менеджеру организации автоматически. Venue-менеджеру его назначает владелец явно." }
                        CreateVenueForm { on_created: move |_| reload += 1 }
                        VenueList { venues: venues() }
                    }
                },
                OrganizationSettingsSection::Positions => rsx! {
                    article { class: "journey-panel",
                        h2 { "Должности и уровни доступа" }
                        p { class: "journey-muted", "Выберите готовый production-профиль. Системная должность владельца доступна только для просмотра." }
                        ol { class: "organization-setup-steps",
                            li { "Создайте рестораны" }
                            li { "Добавьте должности в организационном порядке" }
                            li { "Выберите безопасный уровень доступа" }
                            li { "Проверьте описание прав" }
                            li { "Групповые приглашения станут доступны после публикации правовых документов" }
                        }
                        CreatePositionForm { presets: presets(), on_created: move |_| reload += 1 }
                        PositionList { positions: positions() }
                    }
                },
                OrganizationSettingsSection::Invitations => rsx! {
                    article { class: "journey-panel",
                        h2 { "Групповые приглашения" }
                        if GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED {
                            p { class: "journey-muted", "Одна ссылка регистрирует несколько сотрудников. Каждый профиль активируется владельцем отдельно." }
                            CreateGroupInvitationForm { venues: venues(), positions: positions(), on_created: move |_| reload += 1 }
                            GroupInvitationList { invitations: invitations(), on_reload: move |_| reload += 1 }
                        } else {
                            div { class: "journey-empty", role: "status",
                                strong { "Временно недоступно" }
                                p { "Групповые приглашения будут включены после публикации обновлённых правовых документов." }
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn CreateVenueForm(on_created: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationAccessApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut name = use_signal(String::new);
    let mut timezone = use_signal(|| "Europe/Moscow".to_string());
    let mut request_id = use_signal(|| None::<Uuid>);
    let mut creating = use_signal(|| false);
    let mut generation = use_signal(|| 0_u64);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| None::<String>);
    let create_session = session.clone();
    let create_api = api.clone();

    rsx! {
        form { class: "organization-venue-form", onsubmit: move |event| {
            event.prevent_default();
            if creating() { return; }
            let Some(venue_name) = normalized_venue_name(&name()) else {
                error.set(Some("Введите название ресторана.".into()));
                return;
            };
            let Some((token, company_id)) = current_scope(&create_session) else {
                error.set(Some("Сессия недоступна. Войдите снова.".into()));
                return;
            };
            let operation_request_id = match request_id() {
                Some(value) => value,
                None => {
                    let Some(value) = browser_request_id() else {
                        error.set(Some("Не удалось подготовить безопасный запрос.".into()));
                        return;
                    };
                    request_id.set(Some(value));
                    value
                }
            };
            creating.set(true);
            error.set(None);
            success.set(None);
            generation += 1;
            let operation_generation = generation();
            let epoch = lifecycle_epoch();
            let api = create_api.clone();
            let body = CreateOrganizationVenueRequest {
                request_id: operation_request_id,
                name: venue_name,
                timezone: Some(timezone()),
            };
            spawn(async move {
                let result = api.create_venue(&token, company_id, &body).await;
                if lifecycle_epoch() != epoch || generation() != operation_generation {
                    return;
                }
                creating.set(false);
                match result {
                    Ok(value) => {
                        success.set(Some(if value.created { "Ресторан создан." } else { "Ресторан уже был создан этим запросом." }.into()));
                        name.set(String::new());
                        request_id.set(None);
                        on_created.call(());
                    }
                    Err(problem) => error.set(Some(safe_error(&problem).into())),
                }
            });
        },
            div { class: "form-field",
                label { class: "field-label", r#for: "organization-venue-name", "Название ресторана" }
                input { id: "organization-venue-name", class: "field-input", maxlength: "255", autocomplete: "organization", value: "{name}", disabled: creating(), oninput: move |event| { name.set(event.value()); request_id.set(None); error.set(None); } }
            }
            div { class: "form-field",
                label { class: "field-label", r#for: "organization-venue-timezone", "Часовой пояс" }
                select { id: "organization-venue-timezone", class: "field-input", value: "{timezone}", disabled: creating(), onchange: move |event| { timezone.set(event.value()); request_id.set(None); },
                    option { value: "Europe/Moscow", "Europe/Moscow" }
                }
            }
            if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
            if let Some(message) = success() { p { class: "journey-success", role: "status", "{message}" } }
            button { class: "btn-primary", r#type: "submit", disabled: normalized_venue_name(&name()).is_none() || creating(), if creating() { "Создание…" } else { "Создать ресторан" } }
        }
    }
}

#[component]
fn EmployeeAccessList(
    employees: Option<Result<Vec<OrganizationEmployeeAccess>, OrganizationAccessApiError>>,
    venues: Option<Result<Vec<OrganizationVenue>, OrganizationAccessApiError>>,
    positions: Option<Result<Vec<OrganizationPosition>, OrganizationWorkflowApiError>>,
    on_reload: EventHandler<()>,
) -> Element {
    match (employees, venues, positions) {
        (None, _, _) | (_, None, _) | (_, _, None) => {
            rsx! { p { role: "status", "Загрузка сотрудников и ресторанов…" } }
        }
        (Some(Err(problem)), _, _) | (_, Some(Err(problem)), _) => rsx! {
            div { class: "journey-error", role: "alert",
                p { "{safe_error(&problem)}" }
                button { class: "btn-secondary", r#type: "button", onclick: move |_| on_reload.call(()), "Повторить" }
            }
        },
        (_, _, Some(Err(problem))) => rsx! {
            div { class: "journey-error", role: "alert",
                p { "{safe_workflow_error(&problem)}" }
                button { class: "btn-secondary", r#type: "button", onclick: move |_| on_reload.call(()), "Повторить" }
            }
        },
        (Some(Ok(items)), Some(Ok(_)), Some(Ok(_))) if items.is_empty() => {
            rsx! { div { class: "journey-empty", "Сотрудников пока нет." } }
        }
        (Some(Ok(items)), Some(Ok(venues)), Some(Ok(positions))) => {
            rsx! { div { class: "organization-access-list",
                for employee in items {
                    AccessEditor {
                        key: "access-{employee.employee_profile_id}-{employee.revision}",
                        employee,
                        venues: venues.clone(),
                        positions: positions.clone(),
                        on_saved: move |_| on_reload.call(()),
                    }
                }
            } }
        }
    }
}

#[component]
fn AccessEditor(
    employee: OrganizationEmployeeAccess,
    venues: Vec<OrganizationVenue>,
    positions: Vec<OrganizationPosition>,
    on_saved: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationAccessApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let initial_profile = employee.profile;
    let initial_venues = employee.venue_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut profile = use_signal(|| initial_profile);
    let mut selected = use_signal(|| initial_venues.clone());
    let initial_position = employee.position_id;
    let mut selected_position = use_signal(|| initial_position);
    let mut confirming = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut position_saving = use_signal(|| false);
    let mut generation = use_signal(|| 0_u64);
    let mut error = use_signal(|| None::<String>);
    let save_session = session.clone();
    let save_api = api.clone();
    let position_session = session.clone();
    let position_api = api.clone();
    let access_revision = employee.revision.clone();
    let position_revision = employee.revision.clone();
    let employee_id = employee.employee_profile_id;
    let changed = profile() != initial_profile || selected() != initial_venues;
    let position_changed = selected_position() != initial_position;
    let valid = access_selection_valid(profile(), selected().len(), employee.editable);

    rsx! {
        article { class: "organization-access-card",
          details { class: "employee-action-sheet",
            summary {
                div { strong { "{employee.display_name}" } small { "{employee.profile.label()}" } }
                span { class: "access-status-pill", "{employee.employment_status}" }
            }
            p { class: "journey-muted", "Настроить сотрудника, должность и доступные рестораны" }
            if employee.editable {
                div { class: "form-field",
                    label { class: "field-label", r#for: "position-{employee_id}", "Должность" }
                    select { id: "position-{employee_id}", class: "field-input", value: "{selected_position}", disabled: position_saving(), onchange: move |event| {
                        if let Ok(value) = Uuid::parse_str(&event.value()) {
                            selected_position.set(value);
                            error.set(None);
                        }
                    },
                        for position in positions.iter().filter(|item| item.access_preset != "owner") {
                            option { key: "employee-position-{position.position_id}", value: "{position.position_id}", "{position.name}" }
                        }
                    }
                    button { class: "btn-secondary", r#type: "button", disabled: !position_changed || position_saving() || saving(), onclick: move |_| {
                        let Some((token, company_id)) = current_scope(&position_session) else {
                            error.set(Some("Сессия недоступна. Войдите снова.".into()));
                            return;
                        };
                        position_saving.set(true);
                        error.set(None);
                        let epoch = lifecycle_epoch();
                        let api = position_api.clone();
                        let body = ReplaceOrganizationPositionRequest {
                            position_id: selected_position(),
                            expected_revision: position_revision.clone(),
                        };
                        spawn(async move {
                            let result = api.replace_position(&token, company_id, employee_id, &body).await;
                            if lifecycle_epoch() != epoch { return; }
                            position_saving.set(false);
                            match result {
                                Ok(_) => on_saved.call(()),
                                Err(problem) => error.set(Some(safe_error(&problem).into())),
                            }
                        });
                    }, if position_saving() { "Сохранение…" } else { "Изменить должность" } }
                }
                div { class: "form-field",
                    label { class: "field-label", r#for: "profile-{employee.employee_profile_id}", "Профиль доступа" }
                    select { id: "profile-{employee.employee_profile_id}", class: "field-input", value: "{profile().wire()}", disabled: saving(), onchange: move |event| {
                        let Some(value) = profile_from_wire(&event.value()) else { return; };
                        profile.set(value);
                        if matches!(value, OrganizationAccessProfile::EmployeeUnassigned | OrganizationAccessProfile::OrganizationManager) {
                            selected.set(BTreeSet::new());
                        }
                        confirming.set(false);
                        error.set(None);
                    },
                        option { value: "employee_unassigned", "Сотрудник без ресторана" }
                        option { value: "employee_venue", "Сотрудник ресторана" }
                        option { value: "venue_manager", "Менеджер ресторанов" }
                        option { value: "organization_manager", "Менеджер организации" }
                    }
                }
                if matches!(profile(), OrganizationAccessProfile::EmployeeVenue | OrganizationAccessProfile::VenueManager) {
                    fieldset { class: "organization-venue-choice", disabled: saving(),
                        legend { if profile() == OrganizationAccessProfile::EmployeeVenue { "Выберите один ресторан" } else { "Выберите доступные рестораны" } }
                        for venue in venues.iter() {
                            label { key: "employee-{employee.employee_profile_id}-venue-{venue.venue_id}",
                                input { r#type: if profile() == OrganizationAccessProfile::EmployeeVenue { "radio" } else { "checkbox" }, name: "venues-{employee.employee_profile_id}", value: "{venue.venue_id}", checked: selected().contains(&venue.venue_id), onchange: {
                                    let venue_id = venue.venue_id;
                                    move |event: Event<FormData>| {
                                        let mut next = selected();
                                        if profile() == OrganizationAccessProfile::EmployeeVenue {
                                            next.clear();
                                            if event.checked() { next.insert(venue_id); }
                                        } else if event.checked() {
                                            next.insert(venue_id);
                                        } else {
                                            next.remove(&venue_id);
                                        }
                                        selected.set(next);
                                        confirming.set(false);
                                        error.set(None);
                                    }
                                } }
                                span { "{venue.name}" }
                            }
                        }
                    }
                }
                if !valid && changed { p { class: "journey-warning", role: "status", "Для выбранного профиля укажите корректное количество ресторанов." } }
                if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
                if confirming() {
                    div { class: "organization-access-confirm", role: "group", aria_label: "Подтверждение изменения доступа",
                        p { "Подтвердите изменение профиля и доступных ресторанов для этого сотрудника." }
                        button { class: "btn-primary", r#type: "button", disabled: saving(), onclick: move |_| {
                            let Some((token, company_id)) = current_scope(&save_session) else {
                                error.set(Some("Сессия недоступна. Войдите снова.".into()));
                                return;
                            };
                            if !access_selection_valid(profile(), selected().len(), employee.editable) { return; }
                            saving.set(true);
                            error.set(None);
                            generation += 1;
                            let operation_generation = generation();
                            let epoch = lifecycle_epoch();
                            let api = save_api.clone();
                            let body = ReplaceOrganizationAccessRequest {
                                profile: profile(),
                                venue_ids: selected().into_iter().collect(),
                                expected_revision: access_revision.clone(),
                            };
                            spawn(async move {
                                let result = api.replace_access(&token, company_id, employee_id, &body).await;
                                if lifecycle_epoch() != epoch || generation() != operation_generation { return; }
                                saving.set(false);
                                confirming.set(false);
                                match result {
                                    Ok(_) => on_saved.call(()),
                                    Err(problem) => error.set(Some(safe_error(&problem).into())),
                                }
                            });
                        }, if saving() { "Сохранение…" } else { "Подтвердить изменение" } }
                        button { class: "btn-secondary", r#type: "button", disabled: saving(), onclick: move |_| confirming.set(false), "Отмена" }
                    }
                } else {
                    button { class: "btn-secondary", r#type: "button", disabled: !changed || !valid || saving(), onclick: move |_| confirming.set(true), "Изменить доступ" }
                }
            } else {
                p { class: "journey-muted", "Профиль владельца защищён и не редактируется." }
            }
          }
        }
    }
}

#[component]
fn AccessStructure(
    employees: Option<Result<Vec<OrganizationEmployeeAccess>, OrganizationAccessApiError>>,
    venues: Option<Result<Vec<OrganizationVenue>, OrganizationAccessApiError>>,
) -> Element {
    match (employees, venues) {
        (Some(Ok(employees)), Some(Ok(venues))) => rsx! {
            div { class: "access-tree",
                strong { "Организация" }
                ul {
                    for employee in employees {
                        li { key: "tree-{employee.employee_profile_id}",
                            strong { "{employee.display_name}" }
                            span { " · {employee.profile.label()}" }
                            if !employee.venue_ids.is_empty() {
                                ul {
                                    for venue_id in employee.venue_ids {
                                        if let Some(venue) = venues.iter().find(|item| item.venue_id == venue_id) {
                                            li { key: "tree-{employee.employee_profile_id}-{venue_id}", "{venue.name}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        (Some(Err(problem)), _) | (_, Some(Err(problem))) => {
            rsx! { p { class: "journey-error", role: "alert", "{safe_error(&problem)}" } }
        }
        _ => rsx! { p { role: "status", "Загрузка структуры доступа…" } },
    }
}

#[component]
fn VenueList(
    venues: Option<Result<Vec<OrganizationVenue>, OrganizationAccessApiError>>,
) -> Element {
    match venues {
        None => rsx! { p { role: "status", "Загрузка ресторанов…" } },
        Some(Err(problem)) => {
            rsx! { p { class: "journey-error", role: "alert", "{safe_error(&problem)}" } }
        }
        Some(Ok(items)) if items.is_empty() => {
            rsx! { div { class: "journey-empty", "Ресторанов пока нет." } }
        }
        Some(Ok(items)) => rsx! { ul { class: "venue-settings-list",
            for venue in items {
                li { key: "settings-venue-{venue.venue_id}",
                    span { class: "brand-venue-mark", aria_hidden: "true", "R" }
                    div {
                        strong { "{venue.name}" }
                        small {
                            if let Some(value) = venue.timezone { "{value}" } else { "Часовой пояс организации" }
                        }
                    }
                }
            }
        } },
    }
}

#[component]
fn CreatePositionForm(
    presets: Option<Result<Vec<AccessPreset>, OrganizationWorkflowApiError>>,
    on_created: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut preset = use_signal(|| "employee_venue".to_string());
    let mut sort_order = use_signal(|| "100".to_string());
    let mut request_id = use_signal(|| None::<Uuid>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    rsx! { form { class: "organization-workflow-form", onsubmit: move |event| {
        event.prevent_default();
        let Ok(order) = sort_order().parse::<i32>() else { error.set(Some("Укажите порядок от 0 до 1000.".into())); return; };
        if busy() || name().trim().is_empty() || !(0..=1000).contains(&order) { return; }
        let Some((token, company_id)) = current_scope(&session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
        let Some(operation_id) = request_id().or_else(browser_request_id) else { error.set(Some("Не удалось подготовить безопасный запрос.".into())); return; };
        request_id.set(Some(operation_id));
        busy.set(true);
        error.set(None);
        let api = api.clone();
        let body = CreatePositionRequest {
            request_id: operation_id,
            name: name().trim().to_string(),
            description: (!description().trim().is_empty()).then(|| description().trim().to_string()),
            access_preset: preset(),
            sort_order: order,
        };
        spawn(async move {
            match api.create_position(&token, company_id, &body).await {
                Ok(_) => { name.set(String::new()); description.set(String::new()); request_id.set(None); on_created.call(()); }
                Err(problem) => error.set(Some(safe_workflow_error(&problem).into())),
            }
            busy.set(false);
        });
    },
        div { class: "form-field", label { class: "field-label", r#for: "position-name", "Название должности" }
            input { id: "position-name", class: "field-input", maxlength: "255", value: "{name}", disabled: busy(), oninput: move |event| { name.set(event.value()); request_id.set(None); error.set(None); } }
        }
        div { class: "form-field", label { class: "field-label", r#for: "position-description", "Описание — необязательно" }
            textarea { id: "position-description", class: "field-input", maxlength: "2000", value: "{description}", disabled: busy(), oninput: move |event| { description.set(event.value()); request_id.set(None); } }
        }
        div { class: "form-field", label { class: "field-label", r#for: "position-preset", "Уровень доступа" }
            select { id: "position-preset", class: "field-input", value: "{preset}", disabled: busy(), onchange: move |event| { preset.set(event.value()); request_id.set(None); },
                option { value: "employee_venue", "Обычный сотрудник" }
                option { value: "employee_unassigned", "Сотрудник без привязки" }
                option { value: "venue_manager", "Управление выбранными ресторанами" }
                option { value: "organization_manager", "Управление всей организацией" }
            }
        }
        if let Some(Ok(items)) = presets.as_ref() {
            if let Some(selected) = items.iter().find(|item| item.code == preset()) {
                div { class: "journey-info", role: "status",
                    strong { "{selected.title}" }
                    p { "{selected.description}" }
                    small { "Scope: {selected.scope}" }
                }
            }
        }
        div { class: "form-field", label { class: "field-label", r#for: "position-sort-order", "Порядок в структуре" }
            input { id: "position-sort-order", class: "field-input", r#type: "number", min: "0", max: "1000", value: "{sort_order}", disabled: busy(), oninput: move |event| { sort_order.set(event.value()); request_id.set(None); } }
        }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
        button { class: "btn-primary", r#type: "submit", disabled: busy() || name().trim().is_empty() || sort_order().parse::<i32>().ok().is_none_or(|value| !(0..=1000).contains(&value)), if busy() { "Создание…" } else { "Сохранить должность" } }
    } }
}

#[component]
fn PositionList(
    positions: Option<Result<Vec<OrganizationPosition>, OrganizationWorkflowApiError>>,
) -> Element {
    match positions {
        None => rsx! { p { role: "status", "Загрузка должностей…" } },
        Some(Err(problem)) => {
            rsx! { p { class: "journey-error", role: "alert", "{safe_workflow_error(&problem)}" } }
        }
        Some(Ok(items)) if items.is_empty() => {
            rsx! { div { class: "journey-empty", "Должностей пока нет." } }
        }
        Some(Ok(items)) => rsx! { ul { class: "organization-workflow-list",
            for position in items {
                li { key: "position-{position.position_id}",
                    div { strong { "{position.name}" }
                        if let Some(description) = position.description { small { "{description}" } }
                    }
                    span { class: "status-chip", "{position.access_preset}" }
                }
            }
        } },
    }
}

#[component]
fn PendingRegistrationList(
    pending: Option<Result<Vec<GroupRegistration>, OrganizationWorkflowApiError>>,
    on_reload: EventHandler<()>,
) -> Element {
    rsx! { article { class: "journey-panel",
        h2 { "Ожидают активации" }
        p { class: "journey-muted", "Заявки не смешиваются с активными сотрудниками. Доступ появляется только после явной активации." }
        match pending {
            None => rsx! { p { role: "status", "Загрузка заявок…" } },
            Some(Err(problem)) => rsx! { p { class: "journey-error", role: "alert", "{safe_workflow_error(&problem)}" } },
            Some(Ok(items)) if items.is_empty() => rsx! { div { class: "journey-empty", "Новых заявок нет." } },
            Some(Ok(items)) => rsx! { ul { class: "organization-workflow-list",
                for registration in items {
                    PendingRegistrationRow { key: "pending-{registration.registration_id}", registration, on_reload }
                }
            } },
        }
    } }
}

#[component]
fn PendingRegistrationRow(registration: GroupRegistration, on_reload: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! { li {
        div { strong { "{registration.display_name}" } small { "Ожидает активации" } }
        button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: move |_| {
            let Some((token, company_id)) = current_scope(&session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
            busy.set(true); error.set(None); let api = api.clone();
            spawn(async move { match api.activate_registration(&token, company_id, registration.registration_id).await { Ok(_) => on_reload.call(()), Err(problem) => error.set(Some(safe_workflow_error(&problem).into())) } busy.set(false); });
        }, if busy() { "Активация…" } else { "Активировать" } }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
    } }
}

#[component]
fn CreateGroupInvitationForm(
    venues: Option<Result<Vec<OrganizationVenue>, OrganizationAccessApiError>>,
    positions: Option<Result<Vec<OrganizationPosition>, OrganizationWorkflowApiError>>,
    on_created: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut venue_id = use_signal(String::new);
    let mut position_id = use_signal(String::new);
    let mut label = use_signal(|| "Набор сотрудников".to_string());
    let mut capacity = use_signal(|| "25".to_string());
    let mut request_id = use_signal(|| None::<Uuid>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut created_link = use_signal(|| None::<String>);
    let available_venues = venues.and_then(Result::ok).unwrap_or_default();
    let available_positions = positions.and_then(Result::ok).unwrap_or_default();

    rsx! { form { class: "organization-workflow-form", onsubmit: move |event| {
        event.prevent_default();
        let (Ok(selected_venue), Ok(selected_position), Ok(max_registrations)) = (Uuid::parse_str(&venue_id()), Uuid::parse_str(&position_id()), capacity().parse::<i32>()) else { error.set(Some("Выберите ресторан, должность и вместимость.".into())); return; };
        if busy() || !(1..=200).contains(&max_registrations) { return; }
        let Some((token, company_id)) = current_scope(&session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
        let Some(operation_id) = request_id().or_else(browser_request_id) else { error.set(Some("Не удалось подготовить безопасный запрос.".into())); return; };
        request_id.set(Some(operation_id)); busy.set(true); error.set(None); created_link.set(None);
        let api = api.clone();
        let body = CreateGroupInvitationRequest { request_id: operation_id, venue_id: selected_venue, position_id: selected_position, label: label().trim().to_string(), expires_at: invitation_expiry_iso(), max_registrations };
        spawn(async move {
            match api.create_group_invitation(&token, company_id, &body).await {
                Ok(value) => { created_link.set(value.join_path); request_id.set(None); on_created.call(()); }
                Err(problem) => error.set(Some(safe_workflow_error(&problem).into())),
            }
            busy.set(false);
        });
    },
        div { class: "form-field", label { class: "field-label", r#for: "group-label", "Название набора" }
            input { id: "group-label", class: "field-input", maxlength: "160", value: "{label}", disabled: busy(), oninput: move |event| { label.set(event.value()); request_id.set(None); } }
        }
        div { class: "form-field", label { class: "field-label", r#for: "group-venue", "Ресторан" }
            select { id: "group-venue", class: "field-input", value: "{venue_id}", disabled: busy(), onchange: move |event| { venue_id.set(event.value()); request_id.set(None); },
                option { value: "", "Выберите ресторан" }
                for venue in available_venues { option { value: "{venue.venue_id}", "{venue.name}" } }
            }
        }
        div { class: "form-field", label { class: "field-label", r#for: "group-position", "Должность" }
            select { id: "group-position", class: "field-input", value: "{position_id}", disabled: busy(), onchange: move |event| { position_id.set(event.value()); request_id.set(None); },
                option { value: "", "Выберите должность" }
                for position in available_positions { if position.access_preset != "owner" && position.access_preset != "employee_unassigned" { option { value: "{position.position_id}", "{position.name}" } } }
            }
        }
        div { class: "form-field", label { class: "field-label", r#for: "group-capacity", "Количество регистраций" }
            input { id: "group-capacity", class: "field-input", r#type: "number", min: "1", max: "200", value: "{capacity}", disabled: busy(), oninput: move |event| { capacity.set(event.value()); request_id.set(None); } }
        }
        p { class: "journey-muted", "Ссылка действует 7 дней. В базе хранится только криптографический digest токена." }
        if let Some(link) = created_link() { div { class: "journey-success", role: "status", strong { "Ссылка создана" } code { "{link}" } } }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
        button { class: "btn-primary", r#type: "submit", disabled: busy() || venue_id().is_empty() || position_id().is_empty() || label().trim().is_empty(), if busy() { "Создание…" } else { "Создать групповую ссылку" } }
    } }
}

#[component]
fn GroupInvitationList(
    invitations: Option<Result<Vec<GroupInvitation>, OrganizationWorkflowApiError>>,
    on_reload: EventHandler<()>,
) -> Element {
    match invitations {
        None => rsx! { p { role: "status", "Загрузка приглашений…" } },
        Some(Err(problem)) => {
            rsx! { p { class: "journey-error", role: "alert", "{safe_workflow_error(&problem)}" } }
        }
        Some(Ok(items)) if items.is_empty() => {
            rsx! { div { class: "journey-empty", "Групповых приглашений пока нет." } }
        }
        Some(Ok(items)) => rsx! { div {
            ul { class: "organization-workflow-list",
                for invitation in items {
                    GroupInvitationRow { key: "group-{invitation.invitation_id}", invitation, on_reload }
                }
            }
        } },
    }
}

#[component]
fn GroupInvitationRow(invitation: GroupInvitation, on_reload: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! { li {
        div { strong { "{invitation.label}" } small { "{invitation.registration_count} из {invitation.max_registrations} · {invitation.status}" } }
        if invitation.status == "active" { button { class: "btn-critical", r#type: "button", disabled: busy(), onclick: move |_| {
            let Some((token, company_id)) = current_scope(&session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
            busy.set(true); error.set(None); let api = api.clone();
            spawn(async move { match api.revoke_group_invitation(&token, company_id, invitation.invitation_id).await { Ok(_) => on_reload.call(()), Err(problem) => error.set(Some(safe_workflow_error(&problem).into())) } busy.set(false); });
        }, if busy() { "Отзыв…" } else { "Отозвать" } } }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn access_profiles_require_exact_venue_cardinality() {
        assert!(access_selection_valid(
            OrganizationAccessProfile::EmployeeUnassigned,
            0,
            true
        ));
        assert!(access_selection_valid(
            OrganizationAccessProfile::EmployeeVenue,
            1,
            true
        ));
        assert!(!access_selection_valid(
            OrganizationAccessProfile::EmployeeVenue,
            2,
            true
        ));
        assert!(access_selection_valid(
            OrganizationAccessProfile::VenueManager,
            2,
            true
        ));
        assert!(access_selection_valid(
            OrganizationAccessProfile::OrganizationManager,
            0,
            true
        ));
        assert!(!access_selection_valid(
            OrganizationAccessProfile::Owner,
            0,
            false
        ));
    }

    #[wasm_bindgen_test]
    fn organization_settings_contains_confirmation_and_no_reporting_or_destructive_actions() {
        let source = include_str!("organization_settings.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("Подтвердить изменение"));
        assert!(production.contains("Кадровая подчинённость не создаётся"));
        assert!(!production.contains("hard_delete"));
        assert!(!production.contains("руководитель → подчинённый"));
        assert!(!GROUP_ONBOARDING_DOB_LEGAL_PUBLISHED);
        assert!(production.contains("Временно недоступно"));
    }
}
