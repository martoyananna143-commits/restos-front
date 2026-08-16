//! Team operations with production task views and explicit task dispatch.

use std::collections::BTreeSet;
use std::rc::Rc;

use dioxus::prelude::*;
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_api::AccountAccessToken,
    account_session::{AccountSessionAdapter, AccountSessionState},
    organization_workflow_api::{
        CreateTaskRequest, DispatchTaskRequest, OrganizationTask, OrganizationWorkflowApiClient,
        OrganizationWorkflowApiError, TaskMediaCapability, TaskPhoto, TransitionTaskRequest,
        UpdateTaskRequest,
    },
    workforce_api::{WorkforceApiClient, WorkforceVenue},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeamArea {
    Shifts,
    Tasks,
    Calendar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TaskView {
    Mine,
    Created,
    Review,
}

impl TaskView {
    const fn wire(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Created => "created",
            Self::Review => "review",
        }
    }
}

fn current_scope(session: &AccountSessionAdapter) -> Option<(AccountAccessToken, Uuid)> {
    let AccountSessionState::Authenticated(account) = session.state() else {
        return None;
    };
    Some((account.access_token, account.selected_company?.0))
}

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    Uuid::parse_str(&function.call0(&crypto).ok()?.as_string()?).ok()
}

fn safe_error(error: &OrganizationWorkflowApiError) -> &'static str {
    match error {
        OrganizationWorkflowApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        OrganizationWorkflowApiError::NotFound => {
            "Задачи недоступны для текущей роли или ресторана."
        }
        OrganizationWorkflowApiError::Conflict => "Задача уже изменилась. Обновите список.",
        OrganizationWorkflowApiError::InvalidRequest => "Проверьте задачу и исполнителей.",
        OrganizationWorkflowApiError::Unavailable => "Действие больше недоступно.",
        OrganizationWorkflowApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OrganizationWorkflowApiError::InternalError => "Не удалось загрузить задачи.",
    }
}

fn task_status_label(status: &str) -> &'static str {
    match status {
        "draft" => "Черновик",
        "assigned" => "Назначена",
        "submitted_for_review" => "Ожидает проверки",
        "changes_requested" => "Нужна доработка",
        "accepted" => "Принята",
        "completed" => "Завершена",
        "cancelled" => "Отменена",
        _ => "Статус недоступен",
    }
}

fn task_event_label(event: &str) -> &'static str {
    match event {
        "task_created" => "Задача создана",
        "task_updated" => "Черновик изменён",
        "task_cancelled" => "Задача отменена",
        "task_dispatched" => "Задача назначена",
        "assignment_submitted_for_review" => "Результат отправлен на проверку",
        "assignment_changes_requested" => "Запрошена доработка",
        "assignment_accepted" => "Результат принят",
        "task_completed" => "Задача завершена",
        _ => "Событие задачи",
    }
}

struct RevokeObjectUrl(String);

impl Drop for RevokeObjectUrl {
    fn drop(&mut self) {
        let _ = web_sys::Url::revoke_object_url(&self.0);
    }
}

#[derive(Clone)]
struct PreviewObjectUrl(Rc<RevokeObjectUrl>);

impl PreviewObjectUrl {
    fn from_file(file: &web_sys::File) -> Result<Self, ()> {
        web_sys::Url::create_object_url_with_blob(file.as_ref())
            .map(|value| Self(Rc::new(RevokeObjectUrl(value))))
            .map_err(|_| ())
    }

    fn from_bytes(bytes: &[u8], mime_type: &str) -> Result<Self, ()> {
        let parts = js_sys::Array::new();
        let array = js_sys::Uint8Array::from(bytes);
        parts.push(&array.buffer());
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime_type);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(|_| ())?;
        web_sys::Url::create_object_url_with_blob(&blob)
            .map(|value| Self(Rc::new(RevokeObjectUrl(value))))
            .map_err(|_| ())
    }

    fn value(&self) -> &str {
        &self.0 .0
    }
}

fn selected_browser_file(input_id: &str) -> Option<web_sys::File> {
    web_sys::window()?
        .document()?
        .get_element_by_id(input_id)?
        .dyn_into::<web_sys::HtmlInputElement>()
        .ok()?
        .files()?
        .get(0)
}

#[component]
pub fn TeamManagementPage(
    can_manage: bool,
    area: TeamArea,
    on_area_change: EventHandler<TeamArea>,
) -> Element {
    rsx! {
        section { class: "journey-page team-management", aria_labelledby: "team-management-title",
            header { class: "journey-hero",
                p { class: "management-eyebrow", "КОМАНДА" }
                h1 { id: "team-management-title", "Управление командой" }
                p { "Смены, задачи и календарь в контексте выбранной организации." }
            }
            nav { class: "journey-segments", aria_label: "Управление командой",
                button { class: if area == TeamArea::Shifts { "active" } else { "" }, r#type: "button", aria_current: if area == TeamArea::Shifts { "page" } else { "false" }, onclick: move |_| on_area_change.call(TeamArea::Shifts), "Смены" }
                button { class: if area == TeamArea::Tasks { "active" } else { "" }, r#type: "button", aria_current: if area == TeamArea::Tasks { "page" } else { "false" }, onclick: move |_| on_area_change.call(TeamArea::Tasks), "Задачи" }
                button { class: if area == TeamArea::Calendar { "active" } else { "" }, r#type: "button", aria_current: if area == TeamArea::Calendar { "page" } else { "false" }, onclick: move |_| on_area_change.call(TeamArea::Calendar), "Календарь" }
            }
            match area {
                TeamArea::Tasks => rsx! { TaskWorkspace { can_manage } },
                TeamArea::Shifts => rsx! { DeferredTeamArea { title: "Смены", description: "Планирование смен появится после утверждения domain contract." } },
                TeamArea::Calendar => rsx! { DeferredTeamArea { title: "Календарь", description: "События появятся после утверждения календарного workflow." } },
            }
        }
    }
}

#[component]
fn DeferredTeamArea(title: &'static str, description: &'static str) -> Element {
    rsx! { article { class: "journey-panel journey-empty", role: "status",
        span { class: "journey-empty-icon", aria_hidden: "true", "○" }
        h2 { "{title}" } p { "{description}" } strong { "Раздел готовится" }
    } }
}

#[component]
fn TaskWorkspace(can_manage: bool) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let workforce_api = use_context::<WorkforceApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut view = use_signal(|| TaskView::Mine);
    let mut reload = use_signal(|| 0_u64);
    let mut venue_filter = use_signal(String::new);
    let mut status_filter = use_signal(String::new);

    let capability_session = session.clone();
    let capability_api = api.clone();
    let media_capability = use_resource(move || {
        let epoch = lifecycle_epoch();
        let session = capability_session.clone();
        let api = capability_api.clone();
        async move {
            let Some((token, _)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            let result = api.task_media_capability(&token).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            }
            result
        }
    });
    let media_enabled = matches!(
        media_capability(),
        Some(Ok(TaskMediaCapability { enabled: true }))
    );

    let task_session = session.clone();
    let task_api = api.clone();
    let tasks = use_resource(move || {
        let _reload = reload();
        let selected_view = if can_manage { view() } else { TaskView::Mine };
        let epoch = lifecycle_epoch();
        let session = task_session.clone();
        let api = task_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            let result = api.tasks(&token, company_id, selected_view.wire()).await;
            if lifecycle_epoch() != epoch {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            }
            result
        }
    });
    let venue_session = session.clone();
    let venue_api = workforce_api.clone();
    let venues = use_resource(move || {
        let _reload = reload();
        let session = venue_session.clone();
        let api = venue_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return None;
            };
            api.venues(&token, company_id).await.ok()
        }
    });
    let visible_tasks = tasks().map(|result| {
        result.map(|items| {
            items
                .into_iter()
                .filter(|task| {
                    (venue_filter().is_empty() || task.venue_id.to_string() == venue_filter())
                        && (status_filter().is_empty()
                            || task.assignment_status.as_deref().unwrap_or(&task.status)
                                == status_filter())
                })
                .collect::<Vec<_>>()
        })
    });

    rsx! {
        article { class: "journey-panel task-workspace",
            div { class: "journey-panel-heading", div { h2 { "Рабочие задачи" }
                if media_enabled {
                    p { class: "journey-muted", "Фото результата обязательно перед отправкой на проверку." }
                } else {
                    p { class: "journey-muted", "Задачи с фото временно недоступны до подтверждения защищённого хранилища." }
                }
            }
            }
            match media_capability() {
                None => rsx! { p { role: "status", "Проверяем доступность защищённого хранилища…" } },
                Some(Err(problem)) => rsx! { p { class: "journey-error", role: "alert", "{safe_error(&problem)}" } },
                Some(Ok(TaskMediaCapability { enabled: false })) => rsx! {
                    div { class: "journey-empty", role: "status",
                        strong { "Фото и задачи временно недоступны" }
                        p { "Шаблоны, замеры, аналитика и настройки организации продолжают работать." }
                    }
                },
                Some(Ok(TaskMediaCapability { enabled: true })) => rsx! {
                    nav { class: "journey-segments", aria_label: "Представление задач",
                        button { class: if view() == TaskView::Mine { "active" } else { "" }, r#type: "button", onclick: move |_| view.set(TaskView::Mine), "Мои" }
                        if can_manage {
                            button { class: if view() == TaskView::Created { "active" } else { "" }, r#type: "button", onclick: move |_| view.set(TaskView::Created), "Созданные" }
                            button { class: if view() == TaskView::Review { "active" } else { "" }, r#type: "button", onclick: move |_| view.set(TaskView::Review), "На проверке" }
                        }
                    }
                    if can_manage && view() == TaskView::Created {
                        CreateTaskForm { venues: venues(), on_created: move |_| reload += 1 }
                    }
                    div { class: "task-filters", aria_label: "Фильтры задач",
                        label { "Ресторан"
                            select { value: "{venue_filter}", onchange: move |event| venue_filter.set(event.value()),
                                option { value: "", "Все доступные" }
                                for venue in venues().flatten().unwrap_or_default() { option { value: "{venue.venue_id}", "{venue.name}" } }
                            }
                        }
                        label { "Статус"
                            select { value: "{status_filter}", onchange: move |event| status_filter.set(event.value()),
                                option { value: "", "Все статусы" }
                                option { value: "assigned", "Назначена" }
                                option { value: "submitted_for_review", "Ожидает проверки" }
                                option { value: "changes_requested", "Нужна доработка" }
                                option { value: "accepted", "Принята" }
                                option { value: "completed", "Завершена" }
                            }
                        }
                    }
                    TaskList { tasks: visible_tasks, can_manage, on_changed: move |_| reload += 1 }
                },
            }
        }
    }
}

#[component]
fn CreateTaskForm(
    venues: Option<Option<Vec<WorkforceVenue>>>,
    on_created: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut title = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut venue_id = use_signal(String::new);
    let mut selected = use_signal(BTreeSet::<Uuid>::new);
    let mut request_id = use_signal(|| None::<Uuid>);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let venue_items = venues.flatten().unwrap_or_default();
    let assignee_session = session.clone();
    let assignee_api = api.clone();
    let assignees = use_resource(move || {
        let selected_venue = Uuid::parse_str(&venue_id()).ok();
        let session = assignee_session.clone();
        let api = assignee_api.clone();
        async move {
            let Some(venue) = selected_venue else {
                return None;
            };
            let Some((token, company_id)) = current_scope(&session) else {
                return None;
            };
            Some(api.task_assignees(&token, company_id, venue).await)
        }
    });
    let employee_items = assignees()
        .and_then(|value| value)
        .and_then(Result::ok)
        .unwrap_or_default();
    rsx! { form { class: "task-create-form", onsubmit: move |event| {
        event.prevent_default();
        let (Ok(venue), Some(operation_id)) = (Uuid::parse_str(&venue_id()), request_id().or_else(browser_request_id)) else { error.set(Some("Выберите ресторан и исполнителей.".into())); return; };
        if busy() || title().trim().is_empty() || selected().is_empty() { return; }
        let Some((token, company_id)) = current_scope(&session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
        request_id.set(Some(operation_id)); busy.set(true); error.set(None); let api = api.clone();
        let create = CreateTaskRequest { request_id: operation_id, venue_id: venue, assessment_attempt_id: None, title: title().trim().to_string(), description: (!description().trim().is_empty()).then(|| description().trim().to_string()) };
        let assignees = selected().into_iter().collect::<Vec<_>>();
        spawn(async move {
            let result = async {
                let task = api.create_task(&token, company_id, &create).await?;
                api.dispatch_task(&token, company_id, task.task_id, &DispatchTaskRequest { employee_profile_ids: assignees, expected_version: task.version }).await
            }.await;
            match result { Ok(_) => { title.set(String::new()); description.set(String::new()); selected.set(BTreeSet::new()); request_id.set(None); on_created.call(()); }, Err(problem) => error.set(Some(safe_error(&problem).into())) }
            busy.set(false);
        });
    },
        h3 { "Новая задача" }
        div { class: "form-field", label { class: "field-label", r#for: "task-title", "Название" } input { id: "task-title", class: "field-input", maxlength: "255", value: "{title}", disabled: busy(), oninput: move |event| { title.set(event.value()); request_id.set(None); } } }
        div { class: "form-field", label { class: "field-label", r#for: "task-description", "Описание" } textarea { id: "task-description", class: "field-input", maxlength: "4000", value: "{description}", disabled: busy(), oninput: move |event| { description.set(event.value()); request_id.set(None); } } }
        div { class: "form-field", label { class: "field-label", r#for: "task-venue", "Ресторан" } select { id: "task-venue", class: "field-input", value: "{venue_id}", disabled: busy(), onchange: move |event| { venue_id.set(event.value()); selected.set(BTreeSet::new()); request_id.set(None); }, option { value: "", "Выберите ресторан" } for venue in venue_items { option { value: "{venue.venue_id}", "{venue.name}" } } } }
        fieldset { class: "task-assignees", legend { "Исполнители" }
            for employee in employee_items {
                label { key: "task-employee-{employee.employee_profile_id}", input { r#type: "checkbox", checked: selected().contains(&employee.employee_profile_id), disabled: busy(), onchange: move |_| { let mut next = selected(); if !next.insert(employee.employee_profile_id) { next.remove(&employee.employee_profile_id); } selected.set(next); request_id.set(None); } } span { "{employee.display_name}" } small { "{employee.position_name}" } }
            }
        }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
        button { class: "btn-primary", r#type: "submit", disabled: busy() || title().trim().is_empty() || venue_id().is_empty() || selected().is_empty(), if busy() { "Назначение…" } else { "Создать и назначить" } }
    } }
}

#[component]
fn TaskList(
    tasks: Option<Result<Vec<OrganizationTask>, OrganizationWorkflowApiError>>,
    can_manage: bool,
    on_changed: EventHandler<()>,
) -> Element {
    match tasks {
        None => rsx! { p { role: "status", "Загрузка задач…" } },
        Some(Err(problem)) => {
            rsx! { p { class: "journey-error", role: "alert", "{safe_error(&problem)}" } }
        }
        Some(Ok(items)) if items.is_empty() => {
            rsx! { div { class: "journey-empty", "В этом представлении задач пока нет." } }
        }
        Some(Ok(items)) => rsx! { ul { class: "task-list",
            for task in items { li { key: "task-{task.task_id}-{task.task_assignment_id:?}",
                details { class: "task-detail",
                    summary { strong { "{task.title}" } span { class: "status-chip", "{task_status_label(task.assignment_status.as_deref().unwrap_or(&task.status))}" } }
                    if let Some(description) = task.description.clone() { p { "{description}" } }
                    div { class: "task-list-meta",
                        small { "Исполнителей: {task.assignment_count}" }
                        small { "Фото: {task.photo_count}" }
                        if task.assessment_attempt_id.is_some() { small { "Источник: по результатам замера" } }
                    }
                    TaskActions { task: task.clone(), can_manage, on_changed }
                    TaskHistory { task_id: task.task_id }
                }
            } }
        } },
    }
}

#[component]
fn TaskActions(task: OrganizationTask, can_manage: bool, on_changed: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let task_for_action = task.clone();
    let perform = EventHandler::new(move |action: String| {
        if busy() {
            return;
        }
        let Some((token, company_id)) = current_scope(&session) else {
            error.set(Some("Сессия недоступна. Войдите снова.".into()));
            return;
        };
        let (Some(assignment_id), Some(expected_version)) = (
            task_for_action.task_assignment_id,
            task_for_action.assignment_version,
        ) else {
            error.set(Some("Назначение задачи недоступно.".into()));
            return;
        };
        busy.set(true);
        error.set(None);
        let api = api.clone();
        spawn(async move {
            let result = api
                .transition_task(
                    &token,
                    company_id,
                    assignment_id,
                    &TransitionTaskRequest {
                        action,
                        expected_version,
                    },
                )
                .await;
            busy.set(false);
            match result {
                Ok(_) => on_changed.call(()),
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });
    let status = task.assignment_status.as_deref();
    rsx! {
        if task.status == "draft" && can_manage {
            DraftTaskActions { task: task.clone(), on_changed }
        }
        if task.status != "draft" && task.status != "cancelled" {
            TaskPhotoPanel {
                task: task.clone(),
                can_upload: matches!(status, Some("assigned" | "changes_requested")),
                can_delete: matches!(status, Some("assigned" | "changes_requested")),
                on_changed,
            }
        }
        if matches!(status, Some("assigned" | "changes_requested")) {
            if task.photo_count == 0 {
                p { class: "journey-warning", role: "status", "Добавьте минимум одно фото результата перед отправкой на проверку." }
            }
            button { class: "btn-primary", r#type: "button", disabled: busy() || task.photo_count == 0, onclick: move |_| perform.call("submit".into()), if busy() { "Отправка…" } else { "Отправить на проверку" } }
        }
        if can_render_review_actions(can_manage, status) {
            div { class: "task-review-actions",
                button { class: "btn-primary", r#type: "button", disabled: busy(), onclick: { let perform = perform.clone(); move |_| perform.call("accept".into()) }, "Принять" }
                button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: move |_| perform.call("request_changes".into()), "Вернуть на доработку" }
            }
        }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
    }
}

fn can_render_review_actions(can_manage: bool, status: Option<&str>) -> bool {
    can_manage && status == Some("submitted_for_review")
}

#[component]
fn TaskPhotoPanel(
    task: OrganizationTask,
    can_upload: bool,
    can_delete: bool,
    on_changed: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut reload = use_signal(|| 0_u64);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut preview = use_signal(|| None::<PreviewObjectUrl>);
    let task_id = task.task_id;
    let assignment_id = task.task_assignment_id;
    let list_session = session.clone();
    let list_api = api.clone();
    let photos = use_resource(move || {
        let _reload = reload();
        let session = list_session.clone();
        let api = list_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            api.task_photos(&token, company_id, task_id, assignment_id)
                .await
        }
    });
    let photo_items = photos().and_then(Result::ok).unwrap_or_default();
    let gallery_id = format!("task-photo-gallery-{task_id}");
    let camera_id = format!("task-photo-camera-{task_id}");
    let upload_session = session.clone();
    let upload_api = api.clone();
    let upload = EventHandler::new(move |file: web_sys::File| {
        if busy() || file.size() <= 0.0 || file.size() > 10.0 * 1024.0 * 1024.0 {
            error.set(Some(
                "Выберите JPEG или PNG размером не более 10 МиБ.".into(),
            ));
            return;
        }
        let Some((token, company_id)) = current_scope(&upload_session) else {
            error.set(Some("Сессия недоступна. Войдите снова.".into()));
            return;
        };
        let local_preview = PreviewObjectUrl::from_file(&file).ok();
        let api = upload_api.clone();
        let epoch = lifecycle_epoch();
        busy.set(true);
        error.set(None);
        spawn(async move {
            let result = api
                .upload_task_photo(&token, company_id, task_id, assignment_id, file)
                .await;
            if lifecycle_epoch() != epoch {
                return;
            }
            busy.set(false);
            match result {
                Ok(_) => {
                    preview.set(local_preview);
                    reload += 1;
                    on_changed.call(());
                }
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });

    rsx! { section { class: "task-photo-panel", aria_label: "Фото результата",
        div { class: "task-photo-heading",
            strong { "Фото результата" }
            small { "{photo_items.len()} из 5" }
        }
        if can_upload && photo_items.len() < 5 {
            input {
                id: "{gallery_id}",
                class: "sr-only",
                r#type: "file",
                accept: "image/jpeg,image/png",
                disabled: busy(),
                onchange: {
                    let gallery_id = gallery_id.clone();
                    move |_| {
                        if let Some(file) = selected_browser_file(&gallery_id) {
                            upload.call(file);
                        }
                    }
                },
            }
            input {
                id: "{camera_id}",
                class: "sr-only",
                r#type: "file",
                accept: "image/jpeg,image/png",
                capture: "environment",
                disabled: busy(),
                onchange: {
                    let camera_id = camera_id.clone();
                    move |_| {
                        if let Some(file) = selected_browser_file(&camera_id) {
                            upload.call(file);
                        }
                    }
                },
            }
            div { class: "task-photo-controls",
                button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: {
                    let gallery_id = gallery_id.clone();
                    move |_| {
                        if let Some(input) = web_sys::window().and_then(|value| value.document()).and_then(|value| value.get_element_by_id(&gallery_id)).and_then(|value| value.dyn_into::<web_sys::HtmlInputElement>().ok()) { input.click(); }
                    }
                }, "Выбрать из галереи" }
                button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: {
                    let camera_id = camera_id.clone();
                    move |_| {
                        if let Some(input) = web_sys::window().and_then(|value| value.document()).and_then(|value| value.get_element_by_id(&camera_id)).and_then(|value| value.dyn_into::<web_sys::HtmlInputElement>().ok()) { input.click(); }
                    }
                }, "Сделать фото" }
            }
        }
        if busy() { p { role: "status", "Обработка фото…" } }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
        if photo_items.is_empty() {
            p { class: "journey-muted", "Добавьте хотя бы одно фото перед отправкой на проверку." }
        } else {
            ul { class: "task-photo-list",
                for (index, photo) in photo_items.iter().cloned().enumerate() {
                    TaskPhotoRow { key: "task-photo-{photo.photo_id}", task_id, photo, index, can_delete, busy, error, preview, reload, on_changed }
                }
            }
        }
        if let Some(object_url) = preview() {
            div { class: "task-photo-preview", role: "dialog", aria_label: "Предпросмотр фото результата",
                img { src: "{object_url.value()}", alt: "Фото результата" }
                button { class: "btn-secondary", r#type: "button", onclick: move |_| preview.set(None), "Закрыть предпросмотр" }
            }
        }
    } }
}

#[component]
fn TaskPhotoRow(
    task_id: Uuid,
    photo: TaskPhoto,
    index: usize,
    can_delete: bool,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut preview: Signal<Option<PreviewObjectUrl>>,
    mut reload: Signal<u64>,
    on_changed: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let open_session = session.clone();
    let open_api = api.clone();
    let delete_session = session.clone();
    let delete_api = api.clone();
    let photo_id = photo.photo_id;
    let mime_type = photo.mime_type.clone();
    rsx! { li {
        span { "Фото {index + 1}" }
        button { class: "btn-ghost", r#type: "button", disabled: busy(), onclick: move |_| {
            let Some((token, company_id)) = current_scope(&open_session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
            let api = open_api.clone(); let epoch = lifecycle_epoch(); let mime_type = mime_type.clone(); busy.set(true); error.set(None);
            spawn(async move { let result = api.download_task_photo(&token, company_id, task_id, photo_id).await; if lifecycle_epoch() != epoch { return; } busy.set(false); match result {
                Ok(bytes) => match PreviewObjectUrl::from_bytes(&bytes, &mime_type) { Ok(value) => preview.set(Some(value)), Err(_) => error.set(Some("Не удалось открыть фото.".into())) },
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            } });
        }, "Открыть" }
        if can_delete {
            button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: move |_| {
                let Some((token, company_id)) = current_scope(&delete_session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                let api = delete_api.clone(); let epoch = lifecycle_epoch(); busy.set(true); error.set(None);
                spawn(async move { let result = api.delete_task_photo(&token, company_id, task_id, photo_id).await; if lifecycle_epoch() != epoch { return; } busy.set(false); match result { Ok(_) => { reload += 1; on_changed.call(()); }, Err(problem) => error.set(Some(safe_error(&problem).into())) } });
            }, "Удалить" }
        }
    } }
}

#[component]
pub(crate) fn DraftTaskActions(task: OrganizationTask, on_changed: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut title = use_signal(|| task.title.clone());
    let mut description = use_signal(|| task.description.clone().unwrap_or_default());
    let mut selected = use_signal(BTreeSet::<Uuid>::new);
    let mut busy = use_signal(|| false);
    let mut confirming_delete = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let editable = task.status == "draft";
    let changed = title().trim() != task.title
        || (!description().trim().is_empty()).then(|| description().trim().to_string())
            != task.description;
    let assignee_session = session.clone();
    let assignee_api = api.clone();
    let assignee_venue = task.venue_id;
    let assignees = use_resource(move || {
        let session = assignee_session.clone();
        let api = assignee_api.clone();
        async move {
            let Some((token, company_id)) = current_scope(&session) else {
                return Err(OrganizationWorkflowApiError::AuthenticationRequired);
            };
            api.task_assignees(&token, company_id, assignee_venue).await
        }
    });
    let assignees = assignees().and_then(Result::ok).unwrap_or_default();
    let edit_session = session.clone();
    let edit_api = api.clone();
    let delete_session = session.clone();
    let delete_api = api.clone();
    let dispatch_session = session.clone();
    let dispatch_api = api.clone();
    let edit_task = task.clone();
    let delete_task = task.clone();
    let dispatch_task = task.clone();

    rsx! { div { class: "task-draft-actions",
        div { class: "form-field",
            label { class: "field-label", r#for: "task-edit-title-{task.task_id}", "Название задачи" }
            input { id: "task-edit-title-{task.task_id}", class: "field-input", maxlength: "255", value: "{title}", disabled: busy() || !editable, oninput: move |event| { title.set(event.value()); error.set(None); } }
        }
        div { class: "form-field",
            label { class: "field-label", r#for: "task-edit-description-{task.task_id}", "Описание" }
            textarea { id: "task-edit-description-{task.task_id}", class: "field-input", maxlength: "4000", value: "{description}", disabled: busy() || !editable, oninput: move |event| { description.set(event.value()); error.set(None); } }
        }
        fieldset { class: "task-assignees", disabled: busy() || !editable,
            legend { "Исполнители" }
            for employee in assignees {
                label { key: "draft-assignee-{task.task_id}-{employee.employee_profile_id}",
                    input { r#type: "checkbox", checked: selected().contains(&employee.employee_profile_id), onchange: move |_| {
                        let mut next = selected();
                        if !next.insert(employee.employee_profile_id) { next.remove(&employee.employee_profile_id); }
                        selected.set(next);
                        error.set(None);
                    } }
                    "{employee.display_name}"
                }
            }
        }
        if let Some(message) = error() { p { class: "journey-error", role: "alert", "{message}" } }
        div { class: "task-review-actions",
            button { class: "btn-secondary", r#type: "button", disabled: busy() || !editable || !changed || title().trim().is_empty(), onclick: move |_| {
                let Some((token, company_id)) = current_scope(&edit_session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                busy.set(true); error.set(None); let api = edit_api.clone(); let epoch = lifecycle_epoch();
                let body = UpdateTaskRequest { expected_version: edit_task.version, title: title().trim().to_string(), description: (!description().trim().is_empty()).then(|| description().trim().to_string()) };
                spawn(async move { let result = api.update_task(&token, company_id, edit_task.task_id, &body).await; if lifecycle_epoch() != epoch { return; } busy.set(false); match result { Ok(_) => on_changed.call(()), Err(problem) => error.set(Some(safe_error(&problem).into())) } });
            }, "Сохранить изменения" }
            button { class: "btn-primary", r#type: "button", disabled: busy() || !editable || selected().is_empty(), onclick: move |_| {
                let Some((token, company_id)) = current_scope(&dispatch_session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                busy.set(true); error.set(None); let api = dispatch_api.clone(); let epoch = lifecycle_epoch();
                let body = DispatchTaskRequest { employee_profile_ids: selected().into_iter().collect(), expected_version: dispatch_task.version };
                spawn(async move { let result = api.dispatch_task(&token, company_id, dispatch_task.task_id, &body).await; if lifecycle_epoch() != epoch { return; } busy.set(false); match result { Ok(_) => on_changed.call(()), Err(problem) => error.set(Some(safe_error(&problem).into())) } });
            }, "Назначить выбранным" }
            if confirming_delete() {
                button { class: "btn-danger", r#type: "button", disabled: busy(), onclick: move |_| {
                    let Some((token, company_id)) = current_scope(&delete_session) else { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                    busy.set(true); error.set(None); let api = delete_api.clone(); let epoch = lifecycle_epoch();
                    spawn(async move { let result = api.cancel_task(&token, company_id, delete_task.task_id, delete_task.version).await; if lifecycle_epoch() != epoch { return; } busy.set(false); match result { Ok(_) => on_changed.call(()), Err(problem) => error.set(Some(safe_error(&problem).into())) } });
                }, "Подтвердить удаление" }
                button { class: "btn-secondary", r#type: "button", disabled: busy(), onclick: move |_| confirming_delete.set(false), "Отмена" }
            } else {
                button { class: "btn-secondary", r#type: "button", disabled: busy() || !editable, onclick: move |_| confirming_delete.set(true), "Удалить черновик" }
            }
        }
    } }
}

#[component]
fn TaskHistory(task_id: Uuid) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OrganizationWorkflowApiClient>();
    let mut requested = use_signal(|| false);
    let history_session = session.clone();
    let history_api = api.clone();
    let history = use_resource(move || {
        let should_load = requested();
        let session = history_session.clone();
        let api = history_api.clone();
        async move {
            if !should_load {
                return None;
            }
            let Some((token, company_id)) = current_scope(&session) else {
                return Some(Err(OrganizationWorkflowApiError::AuthenticationRequired));
            };
            Some(api.task_history(&token, company_id, task_id).await)
        }
    });
    rsx! { div { class: "task-history",
        button { class: "btn-ghost", r#type: "button", onclick: move |_| requested.set(true), disabled: requested(), "Показать историю" }
        match history() {
            Some(Some(Ok(events))) => rsx! { ol { for event in events { li { key: "task-event-{task_id}-{event.occurred_at}", span { "{task_event_label(&event.event_type)}" } time { datetime: "{event.occurred_at}", "{event.occurred_at}" } } } } },
            Some(Some(Err(problem))) => rsx! { p { class: "journey-error", role: "alert", "{safe_error(&problem)}" } },
            _ => rsx! {},
        }
    } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn task_views_are_exact_and_no_browser_storage_is_used() {
        assert_eq!(TaskView::Mine.wire(), "mine");
        assert_eq!(TaskView::Created.wire(), "created");
        assert_eq!(TaskView::Review.wire(), "review");
        let production = include_str!("team_management.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("localStorage"));
        assert!(!production.contains("base64"));
        assert!(production.contains("create_task"));
        assert!(production.contains("dispatch_task"));
    }

    #[wasm_bindgen_test]
    fn private_task_media_ui_is_bounded_ephemeral_and_explicit() {
        let production = include_str!("team_management.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!production.contains("localStorage"));
        assert!(!production.contains("sessionStorage"));
        assert!(!production.contains("base64"));
        assert!(!production.contains("signed_url"));
        assert!(production.contains("upload_task_photo"));
        assert!(production.contains("download_task_photo"));
        assert!(production.contains("delete_task_photo"));
        assert!(production.contains("accept: \"image/jpeg,image/png\""));
        assert!(production.contains("capture: \"environment\""));
        assert!(production.contains("file.size() > 10.0 * 1024.0 * 1024.0"));
        assert!(production.contains("Url::revoke_object_url"));
        assert!(production.contains("photo_items.len() < 5"));
        assert!(production.contains("task_media_capability"));
        assert!(production.contains("if media_enabled"));
        assert!(production.contains("Фото и задачи временно недоступны"));
    }

    #[wasm_bindgen_test]
    fn employee_task_workspace_is_read_only_scoped_to_own_assignments() {
        let production = include_str!("team_management.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(production.contains("pub fn TeamManagementPage("));
        assert!(production.contains("area: TeamArea"));
        assert!(production.contains("on_area_change: EventHandler<TeamArea>"));
        assert!(production.contains("if can_manage { view() } else { TaskView::Mine }"));
        assert!(production.contains("if can_manage && view() == TaskView::Created"));
        assert!(production.contains("TaskPhotoPanel"));
    }

    #[wasm_bindgen_test]
    fn employee_cannot_render_manager_review_actions() {
        assert!(!can_render_review_actions(
            false,
            Some("submitted_for_review")
        ));
        assert!(can_render_review_actions(
            true,
            Some("submitted_for_review")
        ));
        assert!(!can_render_review_actions(true, Some("assigned")));
    }
}
