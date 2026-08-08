//! Employee-facing assigned assessment, draft, conflict, and completion flow.

use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_attempt_api::{
        AssessmentAttemptApiClient, AssessmentAttemptApiError, AssessmentItem, AssignmentSummary,
        AttemptAnswer, AttemptDocument, CompletionResult, ReplaceDraftRequest, RevisionConflict,
    },
};

const AUTOSAVE_DEBOUNCE_MS: u32 = 750;
const MAX_ANSWERS: usize = 5_000;
const DEFAULT_TEXT_LIMIT: usize = 10_000;
const MAX_MULTI_OPTIONS: usize = 100;
const MAX_DOCUMENT_BYTES: usize = 1_000_000;

fn remaining_debounce_ms(last_change_ms: f64, now_ms: f64) -> u32 {
    if !last_change_ms.is_finite() || !now_ms.is_finite() || last_change_ms < 0.0 || now_ms < 0.0 {
        return u32::MAX;
    }
    let remaining = last_change_ms + f64::from(AUTOSAVE_DEBOUNCE_MS) - now_ms;
    if remaining <= 0.0 {
        0
    } else {
        remaining.ceil().min(f64::from(u32::MAX)) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveStatus {
    Saved,
    Dirty,
    Saving,
    Failed,
    ValidationError,
    SubmitFailed,
    Conflict,
}

#[derive(Clone, Debug, PartialEq)]
struct DraftState {
    server_revision: i32,
    answers: BTreeMap<Uuid, AttemptAnswer>,
    dirty_generation: u64,
    in_flight: bool,
    status: SaveStatus,
    conflict: Option<RevisionConflict>,
    pending_after_flight: bool,
    last_change_ms: f64,
}

impl DraftState {
    fn from_attempt(attempt: &AttemptDocument) -> Self {
        Self {
            server_revision: attempt.revision,
            answers: attempt
                .answers
                .iter()
                .cloned()
                .map(|answer| (answer.item_id, answer))
                .collect(),
            dirty_generation: 0,
            in_flight: false,
            status: SaveStatus::Saved,
            conflict: None,
            pending_after_flight: false,
            last_change_ms: 0.0,
        }
    }

    fn change(&mut self, answer: Option<AttemptAnswer>, item_id: Uuid, now_ms: f64) -> bool {
        match answer {
            Some(answer) => {
                if self.answers.get(&item_id) == Some(&answer) {
                    return false;
                }
                self.answers.insert(item_id, answer);
            }
            None => {
                if self.answers.remove(&item_id).is_none() {
                    return false;
                }
            }
        }
        self.dirty_generation = self.dirty_generation.wrapping_add(1);
        self.status = SaveStatus::Dirty;
        self.conflict = None;
        self.last_change_ms = now_ms;
        true
    }

    fn payload(&self, attempt: &AttemptDocument) -> Result<ReplaceDraftRequest, ()> {
        if self.answers.len() > MAX_ANSWERS {
            return Err(());
        }
        let items: BTreeMap<Uuid, &AssessmentItem> = attempt
            .document
            .sections
            .iter()
            .flat_map(|section| section.items.iter())
            .map(|item| (item.id, item))
            .collect();
        let mut answers = Vec::with_capacity(self.answers.len());
        for answer in self.answers.values() {
            let item = items.get(&answer.item_id).ok_or(())?;
            answers.push(answer_for(item, answer.value.clone()).ok_or(())?);
        }
        let request = ReplaceDraftRequest {
            expected_revision: self.server_revision,
            answers,
        };
        let encoded = serde_json::to_vec(&request).map_err(|_| ())?;
        (encoded.len() <= MAX_DOCUMENT_BYTES)
            .then_some(request)
            .ok_or(())
    }

    fn accept_save(&mut self, snapshot_generation: u64, attempt: &AttemptDocument) {
        self.in_flight = false;
        self.pending_after_flight = false;
        self.server_revision = attempt.revision;
        self.status = if self.dirty_generation == snapshot_generation {
            SaveStatus::Saved
        } else {
            SaveStatus::Dirty
        };
    }

    fn accept_conflict(&mut self, conflict: RevisionConflict) {
        self.in_flight = false;
        self.pending_after_flight = false;
        self.status = SaveStatus::Conflict;
        self.conflict = Some(conflict);
    }

    fn load_server(&mut self) {
        if let Some(conflict) = self.conflict.take() {
            self.server_revision = conflict.current_revision;
            self.answers = conflict
                .answers
                .into_iter()
                .map(|answer| (answer.item_id, answer))
                .collect();
            self.dirty_generation = self.dirty_generation.wrapping_add(1);
            self.status = SaveStatus::Saved;
            self.pending_after_flight = false;
            self.last_change_ms = 0.0;
        }
    }

    fn prepare_overwrite(&mut self) {
        if let Some(conflict) = self.conflict.take() {
            self.server_revision = conflict.current_revision;
            self.dirty_generation = self.dirty_generation.wrapping_add(1);
            self.status = SaveStatus::Dirty;
        }
    }
}

fn prepare_explicit_retry(
    state: &DraftState,
    read_only: bool,
    save_generation: u64,
) -> Option<(DraftState, u64)> {
    if read_only || state.in_flight || state.status != SaveStatus::Failed {
        return None;
    }
    let mut prepared = state.clone();
    prepared.status = SaveStatus::Dirty;
    Some((prepared, save_generation.wrapping_add(1)))
}

fn operation_is_current(
    current_epoch: u64,
    operation_epoch: u64,
    current_company: Option<Uuid>,
    operation_company: Option<Uuid>,
) -> bool {
    current_epoch == operation_epoch
        && current_company.is_some()
        && current_company == operation_company
}

fn list_result_is_current(
    current_list_generation: u64,
    operation_list_generation: u64,
    current_company_generation: u64,
    operation_company_generation: u64,
    current_epoch: u64,
    operation_epoch: u64,
    current_company: Option<Uuid>,
    operation_company: Option<Uuid>,
) -> bool {
    current_list_generation == operation_list_generation
        && current_company_generation == operation_company_generation
        && operation_is_current(
            current_epoch,
            operation_epoch,
            current_company,
            operation_company,
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubmitPreparation {
    SubmitNow,
    SaveFirst,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AssignmentAction {
    Start,
    Continue,
}

impl AssignmentAction {
    const fn label(self) -> &'static str {
        match self {
            Self::Start => "Начать",
            Self::Continue => "Продолжить",
        }
    }
}

fn assignment_action(status: &str, read_only: bool) -> Option<AssignmentAction> {
    if read_only {
        return None;
    }
    match status {
        "assigned" => Some(AssignmentAction::Start),
        "in_progress" => Some(AssignmentAction::Continue),
        _ => None,
    }
}

fn attempt_action_is_admitted(action: Option<AssignmentAction>, opening: bool) -> bool {
    action.is_some() && !opening
}

fn submit_preparation(status: SaveStatus, read_only: bool, submitting: bool) -> SubmitPreparation {
    if read_only
        || submitting
        || matches!(status, SaveStatus::Conflict | SaveStatus::ValidationError)
    {
        SubmitPreparation::Blocked
    } else if matches!(status, SaveStatus::Saved | SaveStatus::SubmitFailed) {
        SubmitPreparation::SubmitNow
    } else {
        SubmitPreparation::SaveFirst
    }
}

fn submit_after_save(status: SaveStatus, submit_requested: bool) -> bool {
    status == SaveStatus::Saved && submit_requested
}

fn needs_followup_save(state: &DraftState) -> bool {
    state.status == SaveStatus::Dirty || state.pending_after_flight
}

fn answer_for(item: &AssessmentItem, value: Value) -> Option<AttemptAnswer> {
    let option_ids = item.options.iter().map(|option| option.id).collect();
    answer_for_parts(
        item.id,
        &item.answer_type,
        item.config.max_length,
        &option_ids,
        value,
    )
}

fn answer_for_parts(
    item_id: Uuid,
    answer_type: &str,
    max_length: Option<usize>,
    option_ids: &BTreeSet<Uuid>,
    value: Value,
) -> Option<AttemptAnswer> {
    let value = if matches!(answer_type, "score" | "decimal") {
        match value {
            Value::String(text) => text
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map_or(Value::Null, |number| json!(number)),
            other => other,
        }
    } else {
        value
    };
    let valid = match answer_type {
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some(),
        "score" | "decimal" => value.as_f64().is_some_and(f64::is_finite),
        "text" => value.as_str().is_some_and(|text| {
            !text.trim().is_empty()
                && text.chars().count() <= max_length.unwrap_or(DEFAULT_TEXT_LIMIT)
        }),
        "single_choice" => value
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .is_some_and(|id| option_ids.contains(&id)),
        "multi_choice" => value.as_array().is_some_and(|values| {
            values.len() <= MAX_MULTI_OPTIONS
                && values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == values.len()
                && values.iter().all(|value| {
                    value
                        .as_str()
                        .and_then(|id| Uuid::parse_str(id).ok())
                        .is_some_and(|id| option_ids.contains(&id))
                })
        }),
        "date" => value.as_str().is_some_and(valid_date),
        "time" => value.as_str().is_some_and(valid_time),
        _ => false,
    };
    valid.then(|| AttemptAnswer {
        item_id,
        answer_type: answer_type.to_string(),
        value,
    })
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}
fn valid_time(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || byte.is_ascii_digit())
    {
        return false;
    }
    value[0..2].parse::<u32>().is_ok_and(|hour| hour < 24)
        && value[3..5].parse::<u32>().is_ok_and(|minute| minute < 60)
}

fn safe_error(error: &AssessmentAttemptApiError) -> &'static str {
    match error {
        AssessmentAttemptApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        AssessmentAttemptApiError::NotFound => "Оценка недоступна.",
        AssessmentAttemptApiError::InvalidRequest => "Проверьте заполненные ответы.",
        AssessmentAttemptApiError::NetworkUnavailable => "Нет связи с сервером. Повторите вручную.",
        _ => "Не удалось выполнить запрос. Повторите вручную.",
    }
}

fn save_label(status: SaveStatus) -> &'static str {
    match status {
        SaveStatus::Saved => "Сохранено",
        SaveStatus::Dirty => "Есть несохранённые изменения",
        SaveStatus::Saving => "Сохранение…",
        SaveStatus::Failed => "Не удалось сохранить",
        SaveStatus::ValidationError => "Проверьте заполненные ответы",
        SaveStatus::SubmitFailed => "Не удалось отправить оценку",
        SaveStatus::Conflict => "Конфликт версии",
    }
}

#[component]
pub fn AssessmentAttemptsPage() -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentAttemptApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut list_generation = use_signal(|| 0_u64);
    let mut company_generation = use_signal(|| 0_u64);
    let mut scoped_company = use_signal(|| None::<Uuid>);
    let mut attempt_generation = use_signal(|| 0_u64);
    let save_generation = use_signal(|| 0_u64);
    let submit_generation = use_signal(|| 0_u64);
    let mut assignments = use_signal(Vec::<AssignmentSummary>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut opening = use_signal(|| None::<Uuid>);
    let mut attempt = use_signal(|| None::<AttemptDocument>);
    let mut draft = use_signal(|| None::<DraftState>);
    let mut current_section = use_signal(|| 0_usize);
    let mut completion = use_signal(|| None::<CompletionResult>);
    let confirming_submit = use_signal(|| false);
    let submitting = use_signal(|| false);
    let submit_requested = use_signal(|| false);

    let load_session = session.clone();
    let load_api = api.clone();
    use_effect(move || {
        let _epoch = lifecycle_epoch();
        let state = load_session.state();
        let company = match &state {
            AccountSessionState::Authenticated(value) => value.selected_company.map(|id| id.0),
            _ => None,
        };
        if *scoped_company.peek() != company {
            scoped_company.set(company);
            company_generation += 1;
        }
        let operation_company_generation = *company_generation.peek();
        let generation = list_generation();
        assignments.set(Vec::new());
        attempt.set(None);
        draft.set(None);
        completion.set(None);
        error.set(None);
        loading.set(true);
        let Some((token, company_id)) = (match state {
            AccountSessionState::Authenticated(value) => company.map(|id| (value.access_token, id)),
            _ => None,
        }) else {
            loading.set(false);
            error.set(Some("Выберите компанию, чтобы увидеть назначения.".into()));
            return;
        };
        let api = load_api.clone();
        let session = load_session.clone();
        let operation_epoch = lifecycle_epoch();
        spawn(async move {
            let result = api.list_assignments(&token).await;
            let current_company = match session.state() {
                AccountSessionState::Authenticated(value) => value.selected_company.map(|id| id.0),
                _ => None,
            };
            if !list_result_is_current(
                list_generation(),
                generation,
                *company_generation.peek(),
                operation_company_generation,
                lifecycle_epoch(),
                operation_epoch,
                current_company,
                Some(company_id),
            ) {
                return;
            }
            loading.set(false);
            match result {
                Ok(items) => assignments.set(
                    items
                        .into_iter()
                        .filter(|item| item.company_id == company_id)
                        .collect(),
                ),
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });

    if let Some(active) = attempt() {
        return rsx! { AttemptEditor { active, attempt, draft, completion, current_section, confirming_submit, submitting, submit_requested, attempt_generation, save_generation, submit_generation } };
    }

    rsx! {
        section { class: "attempt-page", aria_labelledby: "assigned-assessments-title",
            header { class: "attempt-hero", h1 { id: "assigned-assessments-title", "Мои оценки" } p { "Назначенные вам оценки и сохранённые черновики." } }
            div { class: "account-live", role: "status", aria_live: "polite", if let Some(message) = error() { "{message}" } }
            if loading() { p { class: "account-muted", "Загрузка назначений..." } }
            else if error().is_some() { button { class: "btn-secondary", r#type: "button", onclick: move |_| list_generation += 1, "Повторить" } }
            else if assignments().is_empty() { div { class: "account-empty", p { "Назначенных оценок пока нет." } } }
            else { div { class: "attempt-assignment-list",
                for item in assignments() {
                    { let id = item.id; let action = assignment_action(&item.status, item.read_only); let session = session.clone(); let api = api.clone();
                    rsx! { article { key: "{id}", class: "attempt-assignment-card",
                        div { h2 { "{item.template_name}" } p { "{assignment_status(&item.status, item.read_only)}" } small { "Назначено: {item.assigned_at}" } if let Some(due) = item.due_at.as_ref() { small { "Срок: {due}" } } }
                        if let Some(action) = action {
                            button { class: "btn-primary", r#type: "button", disabled: opening().is_some(), onclick: move |_| {
                                if !attempt_action_is_admitted(Some(action), opening().is_some()) { return; }
                                let token = match session.state() { AccountSessionState::Authenticated(value) => value.access_token, _ => { error.set(Some("Сессия недоступна. Войдите снова.".into())); return; } };
                                opening.set(Some(id)); attempt_generation += 1; let generation = attempt_generation(); let operation_epoch = lifecycle_epoch(); let operation_company_generation = company_generation(); let api = api.clone();
                                spawn(async move { let result = api.create_or_resume(&token, id).await; if attempt_generation() != generation || lifecycle_epoch() != operation_epoch || company_generation() != operation_company_generation { return; } opening.set(None); match result { Ok(value) => { draft.set(Some(DraftState::from_attempt(&value))); attempt.set(Some(value)); current_section.set(0); }, Err(problem) => error.set(Some(safe_error(&problem).into())) } });
                            }, if opening() == Some(id) { "Открытие..." } else { "{action.label()}" } }
                        }
                    } } }
                }
            } }
        }
    }
}

fn assignment_status(status: &str, read_only: bool) -> &'static str {
    if read_only && status == "revoked" {
        "Отозвана · только просмотр"
    } else {
        match status {
            "assigned" => "Назначена",
            "in_progress" => "В процессе",
            "completed" => "Завершена",
            "revoked" => "Отозвана",
            _ => "Недоступна",
        }
    }
}

#[component]
fn AttemptEditor(
    active: AttemptDocument,
    mut attempt: Signal<Option<AttemptDocument>>,
    mut draft: Signal<Option<DraftState>>,
    mut completion: Signal<Option<CompletionResult>>,
    mut current_section: Signal<usize>,
    mut confirming_submit: Signal<bool>,
    mut submitting: Signal<bool>,
    mut submit_requested: Signal<bool>,
    mut attempt_generation: Signal<u64>,
    mut save_generation: Signal<u64>,
    mut submit_generation: Signal<u64>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentAttemptApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let read_only = active.read_only || active.status == "submitted" || completion().is_some();
    let sections = &active.document.sections;
    let section_index = current_section().min(sections.len().saturating_sub(1));
    let total = sections
        .iter()
        .map(|section| section.items.len())
        .sum::<usize>();
    let answered = draft().map(|state| state.answers.len()).unwrap_or(0);
    let status = draft()
        .map(|state| state.status)
        .unwrap_or(SaveStatus::Saved);
    let retry_active = active.clone();
    let retry_api = api.clone();
    let retry_session = session.clone();
    let overwrite_active = active.clone();
    let overwrite_api = api.clone();
    let overwrite_session = session.clone();
    let submit_active = active.clone();
    let submit_api = api.clone();
    let submit_session = session.clone();

    rsx! { section { class: "attempt-editor", aria_labelledby: "attempt-title",
        header { class: "attempt-editor-header", button { class: "btn-ghost", r#type: "button", onclick: move |_| { attempt_generation += 1; attempt.set(None); draft.set(None); completion.set(None); }, "← К назначениям" } h1 { id: "attempt-title", "Прохождение оценки" } p { "Заполнено {answered} из {total}" } }
        div { class: "attempt-save-status", role: "status", aria_live: "polite", "{save_label(status)}" }
        if let Some(reason) = active.read_only_reason.as_ref() { p { class: "account-safe-error", "{read_only_message(reason)}" } }
        nav { class: "attempt-section-nav", aria_label: "Разделы оценки", for (index, section) in sections.iter().enumerate() { button { key: "{section.id}", class: if index == section_index { "active" } else { "" }, r#type: "button", onclick: move |_| current_section.set(index), "{section.title}" } } }
        if let Some(section) = sections.get(section_index) { section { class: "attempt-section", h2 { "{section.title}" } if let Some(description) = section.description.as_ref() { p { "{description}" } }
            for item in section.items.iter() { AnswerControl { key: "{item.id}", item: item.clone(), read_only, draft, active: active.clone(), attempt_generation, save_generation, submit_generation, submit_requested, submitting, confirming_submit, completion } }
        } }
        if matches!(status, SaveStatus::Failed | SaveStatus::ValidationError) { button { class: "btn-secondary", r#type: "button", disabled: status == SaveStatus::ValidationError, onclick: move |_| {
            let Some(current) = draft() else { return; };
            let Some((prepared, next_generation)) = prepare_explicit_retry(&current, read_only, save_generation()) else { return; };
            save_generation.set(next_generation);
            draft.set(Some(prepared));
            schedule_autosave(retry_active.clone(), draft, save_generation, attempt_generation, lifecycle_epoch, retry_api.clone(), retry_session.clone(), true, submit_generation, submit_requested, submitting, confirming_submit, completion);
        }, "Повторить сохранение" } }
        if status == SaveStatus::Conflict { div { class: "attempt-conflict", role: "alert", h2 { "На сервере есть более новая версия" } p { "Выберите, какую версию продолжить." }
            button { class: "btn-secondary", r#type: "button", onclick: move |_| { save_generation += 1; if let Some(state) = draft.write().as_mut() { state.load_server(); } }, "Загрузить версию сервера" }
            button { class: "btn-ghost", r#type: "button", onclick: move |_| { save_generation += 1; if let Some(state) = draft.write().as_mut() { state.prepare_overwrite(); } schedule_autosave(overwrite_active.clone(), draft, save_generation, attempt_generation, lifecycle_epoch, overwrite_api.clone(), overwrite_session.clone(), true, submit_generation, submit_requested, submitting, confirming_submit, completion); }, "Оставить мои ответы и сохранить поверх" }
        } }
        if let Some(result) = completion() { div { class: "attempt-result", h2 { "Оценка завершена" } p { "Отправлено: {result.submitted_at}" } p { "Ответов: {result.answered_count} из {result.total_count}; обязательных: {result.required_count}" } p { "Результат зафиксирован по полноте заполнения." } } }
        else if !read_only { button { class: "btn-primary", r#type: "button", disabled: submit_preparation(status, read_only, submitting()) == SubmitPreparation::Blocked, onclick: move |_| confirming_submit.set(true), "Завершить оценку" } }
        if confirming_submit() { div { class: "attempt-confirm", role: "dialog", aria_modal: "true", aria_labelledby: "submit-confirm-title", h2 { id: "submit-confirm-title", "Отправить оценку?" } p { "После отправки ответы нельзя будет изменить." }
            button { class: "btn-primary", r#type: "button", disabled: submitting(), onclick: move |_| { submitting.set(true); submit_requested.set(true); submit_generation += 1; match submit_preparation(status, read_only, false) { SubmitPreparation::SubmitNow => start_submit(submit_active.id, submit_api.clone(), submit_session.clone(), attempt_generation, lifecycle_epoch, submit_generation, submit_requested, submitting, confirming_submit, completion, draft), SubmitPreparation::SaveFirst => schedule_autosave(submit_active.clone(), draft, save_generation, attempt_generation, lifecycle_epoch, submit_api.clone(), submit_session.clone(), true, submit_generation, submit_requested, submitting, confirming_submit, completion), SubmitPreparation::Blocked => { submit_requested.set(false); submitting.set(false); } } }, if submitting() { "Подготовка..." } else { "Подтвердить" } }
            button { class: "btn-ghost", r#type: "button", disabled: submitting(), onclick: move |_| confirming_submit.set(false), "Отмена" }
        } }
    } }
}

fn read_only_message(reason: &str) -> &'static str {
    match reason {
        "submitted" => "Оценка уже отправлена и доступна только для просмотра.",
        "revoked" => "Назначение отозвано.",
        "expired" => "Срок выполнения истёк.",
        _ => "Редактирование недоступно.",
    }
}

#[component]
fn AnswerControl(
    item: AssessmentItem,
    read_only: bool,
    mut draft: Signal<Option<DraftState>>,
    active: AttemptDocument,
    attempt_generation: Signal<u64>,
    mut save_generation: Signal<u64>,
    mut submit_generation: Signal<u64>,
    mut submit_requested: Signal<bool>,
    mut submitting: Signal<bool>,
    confirming_submit: Signal<bool>,
    completion: Signal<Option<CompletionResult>>,
) -> Element {
    let api = use_context::<AssessmentAttemptApiClient>();
    let session = use_context::<AccountSessionAdapter>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let current = draft().and_then(|state| state.answers.get(&item.id).cloned());
    let label = if item.required {
        format!("{} · обязательно", item.prompt)
    } else {
        item.prompt.clone()
    };
    let item_id = item.id;
    let answer_type = item.answer_type.clone();
    let change_answer_type = answer_type.clone();
    let max_length = item.config.max_length;
    let option_ids = item
        .options
        .iter()
        .map(|option| option.id)
        .collect::<BTreeSet<_>>();
    let change = EventHandler::new(move |value: Value| {
        if read_only {
            return;
        }
        if submit_requested() {
            submit_requested.set(false);
            submitting.set(false);
            submit_generation += 1;
        }
        let answer = answer_for_parts(item_id, &change_answer_type, max_length, &option_ids, value);
        let changed = draft
            .write()
            .as_mut()
            .is_some_and(|state| state.change(answer, item_id, js_sys::Date::now()));
        if !changed {
            return;
        }
        save_generation += 1;
        schedule_autosave(
            active.clone(),
            draft,
            save_generation,
            attempt_generation,
            lifecycle_epoch,
            api.clone(),
            session.clone(),
            false,
            submit_generation,
            submit_requested,
            submitting,
            confirming_submit,
            completion,
        );
    });
    let boolean_change = change;
    let integer_change = change;
    let decimal_change = change;
    let date_change = change;
    let time_change = change;
    let single_change = change;
    let multi_change = change;
    let text_change = change;
    let current_multi: BTreeSet<String> = current
        .as_ref()
        .and_then(|answer| answer.value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect();
    rsx! { fieldset { class: "attempt-question", disabled: read_only, legend { "{label}" } if let Some(guidance) = item.guidance.as_ref() { p { class: "attempt-guidance", "{guidance}" } }
        match answer_type.as_str() {
            "boolean" => rsx! { select { value: current.as_ref().and_then(|a| a.value.as_bool()).map(|v| v.to_string()).unwrap_or_default(), onchange: move |event| boolean_change.call(json!(event.value() == "true")), option { value: "", "Выберите" } option { value: "true", "Да" } option { value: "false", "Нет" } } },
            "score" | "integer" => rsx! { input { r#type: "number", value: current.as_ref().and_then(|a| a.value.as_i64()).map(|v| v.to_string()).unwrap_or_default(), oninput: move |event| if let Ok(value) = event.value().parse::<i64>() { integer_change.call(json!(value)); } } },
            "decimal" => rsx! { input { r#type: "number", step: "any", value: current.as_ref().and_then(|a| a.value.as_str()).unwrap_or_default(), oninput: move |event| decimal_change.call(json!(event.value())) } },
            "date" => rsx! { input { r#type: "date", value: current.as_ref().and_then(|a| a.value.as_str()).unwrap_or_default(), oninput: move |event| date_change.call(json!(event.value())) } },
            "time" => rsx! { input { r#type: "time", value: current.as_ref().and_then(|a| a.value.as_str()).unwrap_or_default(), oninput: move |event| time_change.call(json!(event.value())) } },
            "single_choice" => rsx! { div { for option in item.options.iter() { { let option_id = option.id; let option_change = single_change; rsx! { label { key: "{option_id}", input { r#type: "radio", name: "answer-{item_id}", value: "{option_id}", checked: current.as_ref().and_then(|a| a.value.as_str()) == Some(option_id.to_string().as_str()), onchange: move |_| option_change.call(json!(option_id.to_string())) } "{option.label}" } } } } } },
            "multi_choice" => rsx! {
                div {
                    for option in item.options.iter() {
                        {
                            let id = option.id;
                            let selected = current_multi.contains(&id.to_string());
                            let mut values = current_multi.clone();
                            let option_change = multi_change;
                            rsx! {
                                label {
                                    key: "{id}",
                                    input {
                                        r#type: "checkbox",
                                        checked: selected,
                                        onchange: move |event| {
                                            if event.checked() {
                                                values.insert(id.to_string());
                                            } else {
                                                values.remove(&id.to_string());
                                            }
                                            option_change.call(json!(values));
                                        }
                                    }
                                    "{option.label}"
                                }
                            }
                        }
                    }
                }
            },
            _ => rsx! { textarea { maxlength: "{item.config.max_length.unwrap_or(DEFAULT_TEXT_LIMIT)}", placeholder: item.config.placeholder.as_deref().unwrap_or(""), value: current.as_ref().and_then(|a| a.value.as_str()).unwrap_or_default(), oninput: move |event| text_change.call(json!(event.value())) } },
        }
    } }
}

fn schedule_autosave(
    active: AttemptDocument,
    mut draft: Signal<Option<DraftState>>,
    mut save_generation: Signal<u64>,
    attempt_generation: Signal<u64>,
    lifecycle_epoch: Signal<u64>,
    api: AssessmentAttemptApiClient,
    session: AccountSessionAdapter,
    immediate: bool,
    submit_generation: Signal<u64>,
    mut submit_requested: Signal<bool>,
    mut submitting: Signal<bool>,
    confirming_submit: Signal<bool>,
    completion: Signal<Option<CompletionResult>>,
) {
    let scheduled_generation = save_generation();
    let scheduled_attempt_generation = attempt_generation();
    let scheduled_lifecycle_epoch = lifecycle_epoch();
    spawn(async move {
        if !immediate {
            loop {
                let Some(state) = draft() else {
                    return;
                };
                let remaining = remaining_debounce_ms(state.last_change_ms, js_sys::Date::now());
                if remaining == 0 {
                    break;
                }
                TimeoutFuture::new(remaining).await;
                if save_generation() != scheduled_generation
                    || attempt_generation() != scheduled_attempt_generation
                    || lifecycle_epoch() != scheduled_lifecycle_epoch
                {
                    return;
                }
            }
        }
        if save_generation() != scheduled_generation
            || attempt_generation() != scheduled_attempt_generation
            || lifecycle_epoch() != scheduled_lifecycle_epoch
        {
            return;
        }
        if draft().is_some_and(|state| state.in_flight) {
            if let Some(state) = draft.write().as_mut() {
                state.pending_after_flight = true;
            }
            return;
        }
        let Some(state_snapshot) = draft() else {
            return;
        };
        if state_snapshot.status != SaveStatus::Dirty {
            return;
        }
        let payload = match state_snapshot.payload(&active) {
            Ok(payload) => payload,
            Err(()) => {
                if let Some(state) = draft.write().as_mut() {
                    state.status = SaveStatus::ValidationError;
                }
                return;
            }
        };
        let snapshot_generation = state_snapshot.dirty_generation;
        let token = match session.state() {
            AccountSessionState::Authenticated(value) => value.access_token,
            _ => return,
        };
        if let Some(state) = draft.write().as_mut() {
            state.in_flight = true;
            state.status = SaveStatus::Saving;
        }
        let result = api.replace_draft(&token, active.id, &payload).await;
        if attempt_generation() != scheduled_attempt_generation
            || lifecycle_epoch() != scheduled_lifecycle_epoch
        {
            return;
        }
        match result {
            Ok(saved) => {
                if let Some(state) = draft.write().as_mut() {
                    state.accept_save(snapshot_generation, &saved);
                }
            }
            Err(AssessmentAttemptApiError::Conflict(conflict)) => {
                if let Some(state) = draft.write().as_mut() {
                    state.accept_conflict(conflict);
                }
                submit_requested.set(false);
                submitting.set(false);
            }
            Err(_) => {
                if let Some(state) = draft.write().as_mut() {
                    state.in_flight = false;
                    state.status = SaveStatus::Failed;
                }
                submit_requested.set(false);
                submitting.set(false);
            }
        }
        if draft().is_some_and(|state| submit_after_save(state.status, submit_requested())) {
            start_submit(
                active.id,
                api.clone(),
                session.clone(),
                attempt_generation,
                lifecycle_epoch,
                submit_generation,
                submit_requested,
                submitting,
                confirming_submit,
                completion,
                draft,
            );
            return;
        }
        let schedule_next = draft().is_some_and(|state| needs_followup_save(&state));
        if schedule_next {
            save_generation += 1;
            schedule_autosave(
                active,
                draft,
                save_generation,
                attempt_generation,
                lifecycle_epoch,
                api,
                session,
                false,
                submit_generation,
                submit_requested,
                submitting,
                confirming_submit,
                completion,
            );
        }
    });
}

fn start_submit(
    attempt_id: Uuid,
    api: AssessmentAttemptApiClient,
    session: AccountSessionAdapter,
    attempt_generation: Signal<u64>,
    lifecycle_epoch: Signal<u64>,
    submit_generation: Signal<u64>,
    mut submit_requested: Signal<bool>,
    mut submitting: Signal<bool>,
    mut confirming_submit: Signal<bool>,
    mut completion: Signal<Option<CompletionResult>>,
    mut draft: Signal<Option<DraftState>>,
) {
    let operation_attempt_generation = attempt_generation();
    let operation_lifecycle_epoch = lifecycle_epoch();
    let operation_submit_generation = submit_generation();
    spawn(async move {
        let token = match session.state() {
            AccountSessionState::Authenticated(value) => value.access_token,
            _ => return,
        };
        let result = api.submit(&token, attempt_id).await;
        if attempt_generation() != operation_attempt_generation
            || lifecycle_epoch() != operation_lifecycle_epoch
            || submit_generation() != operation_submit_generation
            || !submit_requested()
        {
            return;
        }
        submit_requested.set(false);
        submitting.set(false);
        confirming_submit.set(false);
        match result {
            Ok(value) => {
                if let Some(state) = draft.write().as_mut() {
                    state.in_flight = false;
                    state.pending_after_flight = false;
                    state.last_change_ms = 0.0;
                    state.status = SaveStatus::Saved;
                }
                completion.set(Some(value));
            }
            Err(_) => {
                if let Some(state) = draft.write().as_mut() {
                    state.status = SaveStatus::SubmitFailed;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    const ITEM_ID: Uuid = Uuid::from_u128(1);
    const OPTION_ID: Uuid = Uuid::from_u128(2);

    fn item(kind: &str) -> AssessmentItem {
        AssessmentItem {
            id: ITEM_ID,
            prompt: "Вопрос".into(),
            guidance: None,
            answer_type: kind.into(),
            required: true,
            sort_order: 1,
            config: crate::assessment_attempt_api::InputConfig {
                placeholder: None,
                max_length: Some(20),
            },
            options: vec![crate::assessment_attempt_api::AssessmentOption {
                id: OPTION_ID,
                label: "A".into(),
                sort_order: 1,
            }],
        }
    }

    #[wasm_bindgen_test]
    fn stage23c_all_nine_answer_types_normalize() {
        for (kind, value) in [
            ("boolean", json!(true)),
            ("score", json!(5)),
            ("integer", json!(2)),
            ("decimal", json!("2.5")),
            ("text", json!("ответ")),
            ("single_choice", json!(Uuid::from_u128(2).to_string())),
            ("multi_choice", json!([Uuid::from_u128(2)])),
            ("date", json!("2026-08-07")),
            ("time", json!("12:30")),
        ] {
            assert!(answer_for(&item(kind), value).is_some(), "{kind}");
        }
    }

    #[wasm_bindgen_test]
    fn stage23c_invalid_answers_are_rejected() {
        assert!(answer_for(&item("integer"), json!(true)).is_none());
        assert!(answer_for(&item("decimal"), json!("NaN")).is_none());
        assert!(answer_for(
            &item("single_choice"),
            json!(Uuid::from_u128(9).to_string())
        )
        .is_none());
    }

    #[wasm_bindgen_test]
    fn stage23c_conflict_preserves_local_until_explicit_choice() {
        let attempt = test_attempt();
        let mut state = DraftState::from_attempt(&attempt);
        state.change(answer_for(&item("text"), json!("local")), ITEM_ID, 100.0);
        state.accept_conflict(RevisionConflict {
            code: "assessment_revision_conflict".into(),
            current_revision: 4,
            answers: vec![answer_for(&item("text"), json!("server")).unwrap()],
        });
        assert_eq!(state.answers[&ITEM_ID].value, json!("local"));
        state.load_server();
        assert_eq!(state.answers[&ITEM_ID].value, json!("server"));
        assert_eq!(state.server_revision, 4);
        assert_eq!(state.status, SaveStatus::Saved);
        assert_eq!(state.last_change_ms, 0.0);
    }

    #[wasm_bindgen_test]
    fn stage23c_change_during_save_remains_dirty() {
        let attempt = test_attempt();
        let mut state = DraftState::from_attempt(&attempt);
        state.change(answer_for(&item("text"), json!("one")), ITEM_ID, 100.0);
        let snapshot = state.dirty_generation;
        state.in_flight = true;
        state.change(answer_for(&item("text"), json!("two")), ITEM_ID, 200.0);
        state.accept_save(
            snapshot,
            &AttemptDocument {
                revision: 2,
                ..attempt
            },
        );
        assert_eq!(state.status, SaveStatus::Dirty);
    }

    #[wasm_bindgen_test]
    fn stage23c_debounce_is_exactly_750_ms() {
        assert_eq!(AUTOSAVE_DEBOUNCE_MS, 750);
        assert_eq!(remaining_debounce_ms(1_000.0, 1_749.0), 1);
        assert_eq!(remaining_debounce_ms(1_000.0, 1_750.0), 0);
        assert_eq!(remaining_debounce_ms(1_000.0, 2_000.0), 0);
        assert_eq!(remaining_debounce_ms(1_000.25, 1_749.5), 1);
        assert_eq!(remaining_debounce_ms(f64::NAN, 1_000.0), u32::MAX);
        assert_eq!(remaining_debounce_ms(1_000.0, f64::INFINITY), u32::MAX);
        assert_eq!(remaining_debounce_ms(1_000.0, 900.0), 850);
    }

    #[wasm_bindgen_test]
    fn stage23c_answer_boundaries_use_production_validators() {
        let mut text = item("text");
        text.config.max_length = Some(DEFAULT_TEXT_LIMIT);
        assert!(answer_for(&text, json!("a".repeat(DEFAULT_TEXT_LIMIT))).is_some());
        assert!(answer_for(&text, json!("a".repeat(DEFAULT_TEXT_LIMIT + 1))).is_none());
        assert!(answer_for(&item("date"), json!("2024-02-29")).is_some());
        assert!(answer_for(&item("date"), json!("2100-02-29")).is_none());
        assert!(answer_for(&item("date"), json!("2026-02-30")).is_none());
        assert!(answer_for(&item("time"), json!("23:59")).is_some());
        assert!(answer_for(&item("time"), json!("24:00")).is_none());
        assert!(answer_for(&item("time"), json!("9:30")).is_none());
    }

    #[wasm_bindgen_test]
    fn stage23c_multi_choice_is_owned_deduplicated_and_bounded() {
        let mut multi = item("multi_choice");
        multi.options = (1..=MAX_MULTI_OPTIONS + 1)
            .map(|number| crate::assessment_attempt_api::AssessmentOption {
                id: Uuid::from_u128(number as u128),
                label: format!("Option {number}"),
                sort_order: number as i32,
            })
            .collect();
        let valid = (1..=MAX_MULTI_OPTIONS)
            .rev()
            .map(|number| Uuid::from_u128(number as u128).to_string())
            .collect::<Vec<_>>();
        assert!(answer_for(&multi, json!(valid)).is_some());
        assert!(answer_for(
            &multi,
            json!([
                Uuid::from_u128(1).to_string(),
                Uuid::from_u128(1).to_string()
            ])
        )
        .is_none());
        let overflow = (1..=MAX_MULTI_OPTIONS + 1)
            .map(|number| Uuid::from_u128(number as u128).to_string())
            .collect::<Vec<_>>();
        assert!(answer_for(&multi, json!(overflow)).is_none());
        assert!(answer_for(&multi, json!([Uuid::from_u128(999).to_string()])).is_none());
    }

    #[wasm_bindgen_test]
    fn stage23c_change_clear_and_identical_value_preserve_generations() {
        let attempt = test_attempt();
        let mut state = DraftState::from_attempt(&attempt);
        let answer = answer_for(&item("text"), json!("one"));
        assert!(state.change(answer.clone(), ITEM_ID, 100.0));
        assert_eq!(state.dirty_generation, 1);
        assert_eq!(state.last_change_ms, 100.0);
        assert!(!state.change(answer, ITEM_ID, 200.0));
        assert_eq!(state.dirty_generation, 1);
        assert_eq!(state.last_change_ms, 100.0);
        assert!(state.change(None, ITEM_ID, 300.0));
        assert!(state.answers.is_empty());
        assert_eq!(state.dirty_generation, 2);
    }

    #[wasm_bindgen_test]
    fn stage23c_generation_admission_rejects_stale_company_or_logout() {
        let company = Some(Uuid::from_u128(10));
        assert!(operation_is_current(4, 4, company, company));
        assert!(!operation_is_current(5, 4, company, company));
        assert!(!operation_is_current(
            4,
            4,
            Some(Uuid::from_u128(11)),
            company
        ));
        assert!(!operation_is_current(4, 4, None, company));
    }

    #[wasm_bindgen_test]
    fn stage23e_assigned_read_write_exposes_start_action() {
        assert_eq!(
            assignment_action("assigned", false),
            Some(AssignmentAction::Start)
        );
        assert_eq!(AssignmentAction::Start.label(), "Начать");
    }

    #[wasm_bindgen_test]
    fn stage23e_in_progress_read_write_exposes_continue_action() {
        assert_eq!(
            assignment_action("in_progress", false),
            Some(AssignmentAction::Continue)
        );
        assert_eq!(AssignmentAction::Continue.label(), "Продолжить");
    }

    #[wasm_bindgen_test]
    fn stage23e_completed_summary_fails_closed_without_attempt_identifier() {
        assert_eq!(assignment_action("completed", true), None);
        assert_eq!(assignment_action("completed", false), None);
    }

    #[wasm_bindgen_test]
    fn stage23e_revoked_read_only_has_no_attempt_action() {
        assert_eq!(assignment_action("revoked", true), None);
    }

    #[wasm_bindgen_test]
    fn stage23e_expired_read_only_has_no_attempt_action() {
        assert_eq!(assignment_action("assigned", true), None);
        assert_eq!(assignment_action("in_progress", true), None);
    }

    #[wasm_bindgen_test]
    fn stage23e_unknown_status_fails_closed() {
        assert_eq!(assignment_action("unknown", false), None);
        assert_eq!(assignment_action("", false), None);
    }

    #[wasm_bindgen_test]
    fn stage23e_read_only_and_missing_action_block_attempt_admission() {
        assert!(!attempt_action_is_admitted(
            assignment_action("revoked", true),
            false
        ));
        assert!(!attempt_action_is_admitted(None, false));
        assert!(!attempt_action_is_admitted(
            Some(AssignmentAction::Start),
            true
        ));
        assert!(attempt_action_is_admitted(
            Some(AssignmentAction::Start),
            false
        ));
    }

    #[wasm_bindgen_test]
    fn stage23c_list_company_switch_and_retry_generations_reject_stale_results() {
        let company = Some(Uuid::from_u128(10));
        assert!(list_result_is_current(2, 2, 3, 3, 4, 4, company, company));
        assert!(!list_result_is_current(3, 2, 3, 3, 4, 4, company, company));
        assert!(!list_result_is_current(2, 2, 4, 3, 4, 4, company, company));
        assert!(!list_result_is_current(
            2,
            2,
            3,
            3,
            4,
            4,
            Some(Uuid::from_u128(11)),
            company,
        ));
    }

    #[wasm_bindgen_test]
    fn stage23c_autosave_single_flight_and_followup_decisions_are_bounded() {
        let attempt = test_attempt();
        let mut state = DraftState::from_attempt(&attempt);
        assert!(!needs_followup_save(&state));
        state.change(answer_for(&item("text"), json!("one")), ITEM_ID, 100.0);
        assert!(needs_followup_save(&state));
        let snapshot = state.dirty_generation;
        state.in_flight = true;
        state.pending_after_flight = true;
        assert!(needs_followup_save(&state));
        state.change(answer_for(&item("text"), json!("two")), ITEM_ID, 200.0);
        state.accept_save(
            snapshot,
            &AttemptDocument {
                revision: 2,
                ..attempt
            },
        );
        assert_eq!(state.status, SaveStatus::Dirty);
        assert!(needs_followup_save(&state));
        assert_eq!(remaining_debounce_ms(state.last_change_ms, 500.0), 450);
        assert_eq!(remaining_debounce_ms(state.last_change_ms, 950.0), 0);
    }

    #[wasm_bindgen_test]
    fn stage23c_submit_requires_confirmation_save_and_current_completion() {
        assert_eq!(
            submit_preparation(SaveStatus::Saved, false, false),
            SubmitPreparation::SubmitNow
        );
        assert_eq!(
            submit_preparation(SaveStatus::Dirty, false, false),
            SubmitPreparation::SaveFirst
        );
        assert_eq!(
            submit_preparation(SaveStatus::Saving, false, false),
            SubmitPreparation::SaveFirst
        );
        assert_eq!(
            submit_preparation(SaveStatus::Failed, false, false),
            SubmitPreparation::SaveFirst
        );
        assert_eq!(
            submit_preparation(SaveStatus::Conflict, false, false),
            SubmitPreparation::Blocked
        );
        assert_eq!(
            submit_preparation(SaveStatus::ValidationError, false, false),
            SubmitPreparation::Blocked
        );
        assert_eq!(
            submit_preparation(SaveStatus::Saved, true, false),
            SubmitPreparation::Blocked
        );
        assert_eq!(
            submit_preparation(SaveStatus::Saved, false, true),
            SubmitPreparation::Blocked
        );
        assert!(submit_after_save(SaveStatus::Saved, true));
        assert!(!submit_after_save(SaveStatus::Dirty, true));
        assert!(!submit_after_save(SaveStatus::Saved, false));
    }

    #[wasm_bindgen_test]
    fn stage23c_safe_list_labels_never_echo_internal_values() {
        assert_eq!(assignment_status("assigned", false), "Назначена");
        assert_eq!(assignment_status("in_progress", false), "В процессе");
        assert_eq!(assignment_status("completed", true), "Завершена");
        assert_eq!(
            assignment_status("revoked", true),
            "Отозвана · только просмотр"
        );
        assert_eq!(
            assignment_status("00000000-0000-0000-0000-000000000001", false),
            "Недоступна"
        );
        assert_eq!(
            safe_error(&AssessmentAttemptApiError::NetworkUnavailable),
            "Нет связи с сервером. Повторите вручную."
        );
    }

    #[wasm_bindgen_test]
    fn stage23c_conflict_overwrite_uses_server_revision_and_local_document() {
        let attempt = test_attempt_with_item(item("text"));
        let mut state = DraftState::from_attempt(&attempt);
        state.change(answer_for(&item("text"), json!("local")), ITEM_ID, 100.0);
        state.accept_conflict(RevisionConflict {
            code: "assessment_revision_conflict".into(),
            current_revision: 9,
            answers: vec![answer_for(&item("text"), json!("server")).unwrap()],
        });
        state.prepare_overwrite();
        let payload = state.payload(&attempt).unwrap();
        assert_eq!(payload.expected_revision, 9);
        assert_eq!(payload.answers[0].value, json!("local"));
        assert_eq!(state.status, SaveStatus::Dirty);
        assert!(state.conflict.is_none());
    }

    #[wasm_bindgen_test]
    fn stage23c_payload_rejects_unknown_items_and_invalid_values() {
        let attempt = test_attempt_with_item(item("text"));
        let mut state = DraftState::from_attempt(&attempt);
        state.answers.insert(
            Uuid::from_u128(99),
            AttemptAnswer {
                item_id: Uuid::from_u128(99),
                answer_type: "text".into(),
                value: json!("hidden"),
            },
        );
        assert!(state.payload(&attempt).is_err());
        state.answers.clear();
        state.answers.insert(
            ITEM_ID,
            AttemptAnswer {
                item_id: ITEM_ID,
                answer_type: "text".into(),
                value: json!("a".repeat(DEFAULT_TEXT_LIMIT + 1)),
            },
        );
        assert!(state.payload(&attempt).is_err());
    }

    #[wasm_bindgen_test]
    fn stage23c_read_only_reasons_are_safe_and_bounded() {
        assert_eq!(
            read_only_message("submitted"),
            "Оценка уже отправлена и доступна только для просмотра."
        );
        assert_eq!(read_only_message("revoked"), "Назначение отозвано.");
        assert_eq!(read_only_message("expired"), "Срок выполнения истёк.");
        assert_eq!(
            read_only_message("private-answer"),
            "Редактирование недоступно."
        );
    }

    #[wasm_bindgen_test]
    fn stage23c_explicit_retry_transitions_failed_without_changing_document() {
        let attempt = test_attempt_with_item(item("text"));
        let mut failed = DraftState::from_attempt(&attempt);
        failed.change(answer_for(&item("text"), json!("local")), ITEM_ID, 125.0);
        failed.status = SaveStatus::Failed;
        let original = failed.clone();

        let (prepared, next_generation) = prepare_explicit_retry(&failed, false, 8).unwrap();

        assert_eq!(prepared.status, SaveStatus::Dirty);
        assert_eq!(next_generation, 9);
        assert_eq!(prepared.answers, original.answers);
        assert_eq!(prepared.server_revision, original.server_revision);
        assert_eq!(prepared.dirty_generation, original.dirty_generation);
        assert_eq!(prepared.last_change_ms, original.last_change_ms);
        assert!(!prepared.in_flight);
        assert!(prepared.payload(&attempt).is_ok());
    }

    #[wasm_bindgen_test]
    fn stage23c_explicit_retry_rejects_non_network_error_states() {
        let attempt = test_attempt();
        for status in [
            SaveStatus::ValidationError,
            SaveStatus::Conflict,
            SaveStatus::Saving,
            SaveStatus::Saved,
            SaveStatus::Dirty,
            SaveStatus::SubmitFailed,
        ] {
            let mut state = DraftState::from_attempt(&attempt);
            state.status = status;
            assert!(
                prepare_explicit_retry(&state, false, 4).is_none(),
                "{status:?}"
            );
        }
    }

    #[wasm_bindgen_test]
    fn stage23c_explicit_retry_rejects_in_flight_and_read_only() {
        let attempt = test_attempt();
        let mut failed = DraftState::from_attempt(&attempt);
        failed.status = SaveStatus::Failed;
        failed.in_flight = true;
        assert!(prepare_explicit_retry(&failed, false, 4).is_none());
        failed.in_flight = false;
        assert!(prepare_explicit_retry(&failed, true, 4).is_none());
    }

    #[wasm_bindgen_test]
    fn stage23c_explicit_retry_network_failure_stays_failed_without_automatic_retry() {
        let attempt = test_attempt();
        let mut state = DraftState::from_attempt(&attempt);
        state.change(answer_for(&item("text"), json!("local")), ITEM_ID, 300.0);
        state.in_flight = false;
        state.status = SaveStatus::Failed;
        state.pending_after_flight = false;

        assert_eq!(state.status, SaveStatus::Failed);
        assert!(!needs_followup_save(&state));
    }

    #[wasm_bindgen_test]
    fn stage23c_explicit_retry_generation_is_single_immediate_and_stale_safe() {
        let attempt = test_attempt_with_item(item("text"));
        let mut failed = DraftState::from_attempt(&attempt);
        failed.change(answer_for(&item("text"), json!("local")), ITEM_ID, 1_000.0);
        failed.status = SaveStatus::Failed;
        let old_generation = 11;

        let (prepared, next_generation) =
            prepare_explicit_retry(&failed, false, old_generation).unwrap();

        assert_eq!(next_generation, old_generation + 1);
        assert_ne!(old_generation, next_generation);
        assert_eq!(prepared.status, SaveStatus::Dirty);
        assert_eq!(prepared.last_change_ms, 1_000.0);
        assert_eq!(remaining_debounce_ms(prepared.last_change_ms, 1_000.0), 750);
        assert!(prepared.payload(&attempt).is_ok());
    }

    fn test_attempt() -> AttemptDocument {
        AttemptDocument {
            id: Uuid::from_u128(3),
            assignment_id: Uuid::from_u128(4),
            status: "draft".into(),
            revision: 1,
            started_at: "2026-08-07T00:00:00Z".into(),
            last_saved_at: None,
            submitted_at: None,
            read_only: false,
            read_only_reason: None,
            document: crate::assessment_attempt_api::TemplateDocument {
                version_id: Uuid::from_u128(5),
                sections: vec![],
            },
            answers: vec![],
        }
    }

    fn test_attempt_with_item(item: AssessmentItem) -> AttemptDocument {
        let mut attempt = test_attempt();
        attempt
            .document
            .sections
            .push(crate::assessment_attempt_api::AssessmentSection {
                id: Uuid::from_u128(6),
                parent_section_id: None,
                title: "Section".into(),
                description: None,
                sort_order: 1,
                items: vec![item],
            });
        attempt
    }
}
