//! Employee-facing assigned assessment, draft, conflict, and completion flow.

use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
};

use dioxus::prelude::*;
use futures_util::{
    future::{select, Either},
    pin_mut,
};
use gloo_timers::future::TimeoutFuture;
use serde_json::{json, Value};
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::{
    account_api::{AccountAccessToken, AccountApiError},
    account_session::{AccountSessionAdapter, AccountSessionState},
    assessment_attempt_api::{
        AssessmentAttemptApiClient, AssessmentAttemptApiError, AssessmentHistoryItem,
        AssessmentHistoryPage, AssessmentItem, AssessmentResultDetail, AssessmentSection,
        AssignmentHistoryPeriod, AssignmentSummary, AttemptAnswer, AttemptDocument,
        CompletionResult, ReplaceDraftRequest, RevisionConflict,
    },
    organization_workflow_api::{
        CreateTaskRequest, OrganizationWorkflowApiClient, OrganizationWorkflowApiError,
    },
    presentation_percent::format_percent,
};

use super::team_management::DraftTaskActions;

const AUTOSAVE_DEBOUNCE_MS: u32 = 750;
const ASSESSMENT_REQUEST_TIMEOUT_MS: u32 = 20_000;
const MAX_ANSWERS: usize = 5_000;
const DEFAULT_TEXT_LIMIT: usize = 10_000;
const MAX_MULTI_OPTIONS: usize = 100;
const MAX_DOCUMENT_BYTES: usize = 1_000_000;

fn browser_request_id() -> Option<Uuid> {
    let crypto = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("crypto")).ok()?;
    let random_uuid = js_sys::Reflect::get(&crypto, &JsValue::from_str("randomUUID")).ok()?;
    let function = random_uuid.dyn_into::<js_sys::Function>().ok()?;
    Uuid::parse_str(&function.call0(&crypto).ok()?.as_string()?).ok()
}

fn safe_task_error(error: &OrganizationWorkflowApiError) -> &'static str {
    match error {
        OrganizationWorkflowApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        OrganizationWorkflowApiError::NotFound => {
            "Быстрая задача недоступна для текущего ресторана."
        }
        OrganizationWorkflowApiError::Conflict => "Задача уже изменилась. Откройте раздел задач.",
        OrganizationWorkflowApiError::InvalidRequest => "Проверьте название задачи.",
        OrganizationWorkflowApiError::Unavailable => "Создание задачи временно недоступно.",
        OrganizationWorkflowApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OrganizationWorkflowApiError::InternalError => "Не удалось сохранить задачу.",
    }
}

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

async fn bounded_request<T>(request: impl Future<Output = T>) -> Option<T> {
    let timeout = TimeoutFuture::new(ASSESSMENT_REQUEST_TIMEOUT_MS);
    pin_mut!(request, timeout);
    match select(request, timeout).await {
        Either::Left((value, _)) => Some(value),
        Either::Right(((), _)) => None,
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
    section_order: Vec<Uuid>,
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
            section_order: normalized_section_order(
                &attempt.document.sections,
                &attempt.ui_metadata.section_order,
            ),
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
            let mut normalized = answer_for(item, answer.value.clone()).ok_or(())?;
            normalized.comment = answer.comment.clone();
            answers.push(normalized);
        }
        let request = ReplaceDraftRequest {
            expected_revision: self.server_revision,
            answers,
            section_order: self.section_order.clone(),
        };
        let encoded = serde_json::to_vec(&request).map_err(|_| ())?;
        (encoded.len() <= MAX_DOCUMENT_BYTES)
            .then_some(request)
            .ok_or(())
    }

    fn change_section_order(&mut self, section_order: Vec<Uuid>, now_ms: f64) -> bool {
        if section_order == self.section_order {
            return false;
        }
        if section_order.len() != self.section_order.len()
            || section_order.iter().collect::<BTreeSet<_>>()
                != self.section_order.iter().collect::<BTreeSet<_>>()
        {
            return false;
        }
        self.section_order = section_order;
        self.dirty_generation = self.dirty_generation.wrapping_add(1);
        self.status = SaveStatus::Dirty;
        self.conflict = None;
        self.last_change_ms = now_ms;
        true
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
            self.section_order = conflict.ui_metadata.section_order;
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

fn normalized_section_order(sections: &[AssessmentSection], saved_order: &[Uuid]) -> Vec<Uuid> {
    let canonical = sections
        .iter()
        .map(|section| section.id)
        .collect::<Vec<_>>();
    if saved_order.len() == canonical.len()
        && saved_order.iter().collect::<BTreeSet<_>>() == canonical.iter().collect::<BTreeSet<_>>()
    {
        saved_order.to_vec()
    } else {
        canonical
    }
}

fn move_section(order: &[Uuid], section_id: Uuid, offset: isize) -> Option<Vec<Uuid>> {
    let current = order
        .iter()
        .position(|candidate| *candidate == section_id)?;
    let target = current.checked_add_signed(offset)?;
    if target >= order.len() {
        return None;
    }
    let mut updated = order.to_vec();
    updated.swap(current, target);
    Some(updated)
}

fn drop_section_before(order: &[Uuid], source: Uuid, target: Uuid) -> Option<Vec<Uuid>> {
    if source == target {
        return None;
    }
    let source_index = order.iter().position(|candidate| *candidate == source)?;
    let mut updated = order.to_vec();
    updated.remove(source_index);
    let target_index = updated.iter().position(|candidate| *candidate == target)?;
    updated.insert(target_index, source);
    (updated != order).then_some(updated)
}

fn ordered_sections(sections: &[AssessmentSection], order: &[Uuid]) -> Vec<AssessmentSection> {
    order
        .iter()
        .filter_map(|id| sections.iter().find(|section| section.id == *id).cloned())
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SectionProgress {
    NotStarted,
    InProgress,
    Complete,
    RequiredMissing(usize),
}

fn section_progress(
    section: &AssessmentSection,
    answers: &BTreeMap<Uuid, AttemptAnswer>,
    active: bool,
) -> SectionProgress {
    let answered = section
        .items
        .iter()
        .filter(|item| answers.contains_key(&item.id))
        .count();
    let required_missing = section
        .items
        .iter()
        .filter(|item| item.required && !answers.contains_key(&item.id))
        .count();
    if answered == section.items.len() && required_missing == 0 {
        SectionProgress::Complete
    } else if active || answered > 0 {
        if required_missing > 0 && answered > 0 {
            SectionProgress::RequiredMissing(required_missing)
        } else {
            SectionProgress::InProgress
        }
    } else {
        SectionProgress::NotStarted
    }
}

fn section_progress_label(progress: SectionProgress) -> String {
    match progress {
        SectionProgress::NotStarted => "○ Не начато".into(),
        SectionProgress::InProgress => "◐ В процессе".into(),
        SectionProgress::Complete => "✓ Заполнено".into(),
        SectionProgress::RequiredMissing(count) => {
            format!("! Обязательных осталось: {count}")
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

fn release_submit_intent(
    mut submit_requested: Signal<bool>,
    mut submitting: Signal<bool>,
    mut confirming_submit: Signal<bool>,
) {
    submit_requested.set(false);
    submitting.set(false);
    confirming_submit.set(false);
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
        comment: None,
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

fn grouped_history(items: &[AssessmentHistoryItem]) -> Vec<(String, Vec<AssessmentHistoryItem>)> {
    let mut groups: Vec<(String, Vec<AssessmentHistoryItem>)> = Vec::new();
    for item in items {
        if let Some((label, values)) = groups.last_mut() {
            if label == &item.day_label {
                values.push(item.clone());
                continue;
            }
        }
        groups.push((item.day_label.clone(), vec![item.clone()]));
    }
    groups
}

fn remove_completed_assignment(items: &mut Vec<AssignmentSummary>, assignment_id: Uuid) -> usize {
    let before = items.len();
    items.retain(|item| item.id != assignment_id);
    before.saturating_sub(items.len())
}

fn human_datetime(value: &str) -> String {
    const MONTHS: [&str; 12] = [
        "января",
        "февраля",
        "марта",
        "апреля",
        "мая",
        "июня",
        "июля",
        "августа",
        "сентября",
        "октября",
        "ноября",
        "декабря",
    ];
    let Some(date) = value.get(0..10).filter(|date| valid_date(date)) else {
        return "Дата недоступна".into();
    };
    let Ok(year) = date[0..4].parse::<u32>() else {
        return "Дата недоступна".into();
    };
    let Ok(month) = date[5..7].parse::<usize>() else {
        return "Дата недоступна".into();
    };
    let Ok(day) = date[8..10].parse::<u32>() else {
        return "Дата недоступна".into();
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return "Дата недоступна".into();
    }
    let month_name = MONTHS.get(month.saturating_sub(1)).unwrap_or(&"месяца");
    let date_label = format!("{day} {month_name} {year}");
    value
        .split('T')
        .nth(1)
        .and_then(|time| time.get(0..5))
        .filter(|time| valid_time(time))
        .map_or(date_label.clone(), |time| format!("{date_label}, {time}"))
}

#[cfg(target_arch = "wasm32")]
fn map_refresh_error(error: AccountApiError) -> AssessmentAttemptApiError {
    match error {
        AccountApiError::NetworkUnavailable => AssessmentAttemptApiError::NetworkUnavailable,
        _ => AssessmentAttemptApiError::AuthenticationRequired,
    }
}

#[cfg(target_arch = "wasm32")]
async fn refreshed_token(
    session: &AccountSessionAdapter,
) -> Result<AccountAccessToken, AssessmentAttemptApiError> {
    let state = session.refresh().await.map_err(map_refresh_error)?;
    let AccountSessionState::Authenticated(value) = state else {
        return Err(AssessmentAttemptApiError::AuthenticationRequired);
    };
    Ok(value.access_token)
}

#[cfg(target_arch = "wasm32")]
async fn replace_draft_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    attempt_id: Uuid,
    payload: &ReplaceDraftRequest,
) -> Result<AttemptDocument, AssessmentAttemptApiError> {
    match api.replace_draft(token, attempt_id, payload).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.replace_draft(&refreshed, attempt_id, payload).await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn submit_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    attempt_id: Uuid,
) -> Result<CompletionResult, AssessmentAttemptApiError> {
    match api.submit(token, attempt_id).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.submit(&refreshed, attempt_id).await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn result_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    company_id: Uuid,
    attempt_id: Uuid,
) -> Result<AssessmentResultDetail, AssessmentAttemptApiError> {
    match api.result(token, company_id, attempt_id).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.result(&refreshed, company_id, attempt_id).await
        }
        result => result,
    }
}

fn completion_from_result(result: AssessmentResultDetail) -> CompletionResult {
    CompletionResult {
        scoring_algorithm: result.scoring_algorithm,
        submitted_at: result.submitted_at,
        answered_count: result.answered_count,
        required_count: result.required_count,
        total_count: result.total_count,
        scoring_version: None,
        numerator: None,
        denominator: None,
        score_percent: result.score_percent,
        coverage: None,
        eligible_count: None,
        excluded_count: None,
        critical_failure_count: Some(result.critical_failure_count),
        stop_factor_count: Some(result.stop_factor_count),
        sections: Vec::new(),
    }
}

#[cfg(target_arch = "wasm32")]
async fn history_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    company_id: Uuid,
    period: &AssignmentHistoryPeriod,
    cursor: Option<&str>,
) -> Result<AssessmentHistoryPage, AssessmentAttemptApiError> {
    match api.history(token, company_id, period, cursor).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.history(&refreshed, company_id, period, cursor).await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn assignments_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    company_id: Uuid,
    period: &AssignmentHistoryPeriod,
) -> Result<Vec<AssignmentSummary>, AssessmentAttemptApiError> {
    match api.list_assignments(token, company_id, period).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.list_assignments(&refreshed, company_id, period).await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn result_pdf_with_one_refresh(
    api: &AssessmentAttemptApiClient,
    session: &AccountSessionAdapter,
    token: &AccountAccessToken,
    company_id: Uuid,
    attempt_id: Uuid,
) -> Result<Vec<u8>, AssessmentAttemptApiError> {
    match api.result_pdf(token, company_id, attempt_id).await {
        Err(AssessmentAttemptApiError::AuthenticationRequired) => {
            let refreshed = refreshed_token(session).await?;
            api.result_pdf(&refreshed, company_id, attempt_id).await
        }
        result => result,
    }
}

async fn save_assessment_pdf(bytes: &[u8], local_date: &str) -> Result<(), ()> {
    if !local_date
        .bytes()
        .all(|value| value.is_ascii_digit() || value == b'-')
    {
        return Err(());
    }
    let parts = js_sys::Array::new();
    let array = js_sys::Uint8Array::from(bytes);
    parts.push(&array.buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/pdf");
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(|_| ())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| ())?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or(())?;
    let anchor = document.create_element("a").map_err(|_| ())?;
    anchor.set_attribute("href", &url).map_err(|_| ())?;
    anchor
        .set_attribute("download", &format!("restos-assessment-{local_date}.pdf"))
        .map_err(|_| ())?;
    anchor.set_attribute("hidden", "").map_err(|_| ())?;
    document
        .body()
        .ok_or(())?
        .append_child(&anchor)
        .map_err(|_| ())?;
    anchor.dyn_ref::<web_sys::HtmlElement>().ok_or(())?.click();
    anchor.remove();
    TimeoutFuture::new(0).await;
    web_sys::Url::revoke_object_url(&url).map_err(|_| ())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssessmentListView {
    Active,
    History,
    Planned,
}

#[component]
pub fn AssessmentAttemptsPage(
    view: AssessmentListView,
    on_measure: EventHandler<()>,
    on_team: EventHandler<()>,
    on_result: EventHandler<Uuid>,
    initial_attempt: Option<AttemptDocument>,
) -> Element {
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
    let mut history_items = use_signal(Vec::<AssessmentHistoryItem>::new);
    let mut history_cursor = use_signal(|| None::<String>);
    let mut history_request_cursor = use_signal(|| None::<String>);
    let mut history_loading_more = use_signal(|| false);
    let mut history_period = use_signal(|| "all".to_string());
    let mut history_from = use_signal(String::new);
    let mut history_to = use_signal(String::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut opening = use_signal(|| None::<Uuid>);
    let mut pdf_busy = use_signal(|| None::<Uuid>);
    let mut pdf_error = use_signal(|| None::<String>);
    let mut pdf_generation = use_signal(|| 0_u64);
    let initial_draft = initial_attempt.as_ref().map(DraftState::from_attempt);
    let has_initial_attempt = initial_attempt.is_some();
    let mut attempt = use_signal(|| initial_attempt);
    let mut draft = use_signal(|| initial_draft);
    let mut current_section = use_signal(|| 0_usize);
    let mut completion = use_signal(|| None::<CompletionResult>);
    let confirming_submit = use_signal(|| false);
    let submitting = use_signal(|| false);
    let submit_requested = use_signal(|| false);

    use_effect(move || {
        if completion().is_some() {
            if let Some(completed) = attempt() {
                assignments.with_mut(|items| {
                    remove_completed_assignment(items, completed.assignment_id);
                });
            }
        }
    });

    let load_session = session.clone();
    let load_api = api.clone();
    use_effect(move || {
        if has_initial_attempt {
            loading.set(false);
            return;
        }
        if view == AssessmentListView::Planned {
            assignments.set(Vec::new());
            error.set(None);
            loading.set(false);
            return;
        }
        let _epoch = lifecycle_epoch();
        let period_code = history_period();
        let custom_from = history_from();
        let custom_to = history_to();
        let request_cursor = history_request_cursor();
        let state = load_session.state();
        let company = match &state {
            AccountSessionState::Authenticated(value) => value.selected_company.map(|id| id.0),
            _ => None,
        };
        if *scoped_company.peek() != company {
            scoped_company.set(company);
            company_generation += 1;
            history_request_cursor.set(None);
            history_cursor.set(None);
            history_items.set(Vec::new());
        }
        let operation_company_generation = *company_generation.peek();
        let generation = list_generation();
        assignments.set(Vec::new());
        if view == AssessmentListView::History {
            if request_cursor.is_none() {
                history_items.set(Vec::new());
                loading.set(true);
            } else {
                history_loading_more.set(true);
            }
        }
        attempt.set(None);
        draft.set(None);
        completion.set(None);
        error.set(None);
        if view != AssessmentListView::History {
            loading.set(true);
        }
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
            let period = match period_code.as_str() {
                "all" => AssignmentHistoryPeriod::AllTime,
                "yesterday" => AssignmentHistoryPeriod::Yesterday,
                "previous_week" => AssignmentHistoryPeriod::PreviousWeek,
                "previous_month" => AssignmentHistoryPeriod::PreviousMonth,
                "custom" => AssignmentHistoryPeriod::Custom {
                    date_from: custom_from,
                    date_to: custom_to,
                },
                _ => AssignmentHistoryPeriod::Today,
            };
            if view == AssessmentListView::History {
                let result = history_with_one_refresh(
                    &api,
                    &session,
                    &token,
                    company_id,
                    &period,
                    request_cursor.as_deref(),
                )
                .await;
                let current_company = match session.state() {
                    AccountSessionState::Authenticated(value) => {
                        value.selected_company.map(|id| id.0)
                    }
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
                history_loading_more.set(false);
                match result {
                    Ok(page) => {
                        if request_cursor.is_some() {
                            history_items.with_mut(|current| {
                                for item in page.items {
                                    if !current
                                        .iter()
                                        .any(|value| value.attempt_id == item.attempt_id)
                                    {
                                        current.push(item);
                                    }
                                }
                            });
                        } else {
                            history_items.set(page.items);
                        }
                        history_cursor.set(page.next_cursor);
                    }
                    Err(problem) => error.set(Some(safe_error(&problem).into())),
                }
                return;
            }
            let result =
                assignments_with_one_refresh(&api, &session, &token, company_id, &period).await;
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
        return rsx! { AttemptEditor { active, attempt, draft, completion, current_section, confirming_submit, submitting, submit_requested, attempt_generation, save_generation, submit_generation, on_team, on_result: on_result.clone() } };
    }

    let assignment_session = session.clone();
    let assignment_api = api.clone();
    let open_assignment = EventHandler::new(move |id: Uuid| {
        let Some(item) = assignments().into_iter().find(|item| item.id == id) else {
            return;
        };
        let action = assignment_action(&item.status, item.read_only);
        if !attempt_action_is_admitted(action, opening().is_some()) {
            return;
        }
        let token = match assignment_session.state() {
            AccountSessionState::Authenticated(value) => value.access_token,
            _ => {
                error.set(Some("Сессия недоступна. Войдите снова.".into()));
                return;
            }
        };
        opening.set(Some(id));
        attempt_generation += 1;
        let generation = attempt_generation();
        let operation_epoch = lifecycle_epoch();
        let operation_company_generation = company_generation();
        let api = assignment_api.clone();
        spawn(async move {
            let result = api.create_or_resume(&token, id).await;
            if attempt_generation() != generation
                || lifecycle_epoch() != operation_epoch
                || company_generation() != operation_company_generation
            {
                return;
            }
            opening.set(None);
            match result {
                Ok(value) => {
                    draft.set(Some(DraftState::from_attempt(&value)));
                    attempt.set(Some(value));
                    current_section.set(0);
                }
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });
    let open_result = EventHandler::new(move |id: Uuid| on_result.call(id));
    let pdf_session = session.clone();
    let pdf_api = api.clone();
    let download_pdf = EventHandler::new(move |id: Uuid| {
        if pdf_busy().is_some() {
            return;
        }
        let Some(item) = history_items()
            .into_iter()
            .find(|item| item.attempt_id == id && item.pdf_available)
        else {
            return;
        };
        let AccountSessionState::Authenticated(account) = pdf_session.state() else {
            pdf_error.set(Some("Сессия недоступна. Войдите снова.".into()));
            return;
        };
        let Some(company_id) = account.selected_company.map(|value| value.0) else {
            pdf_error.set(Some("Выберите организацию.".into()));
            return;
        };
        pdf_busy.set(Some(id));
        pdf_error.set(None);
        pdf_generation += 1;
        let generation = pdf_generation();
        let operation_epoch = lifecycle_epoch();
        let operation_company_generation = company_generation();
        let api = pdf_api.clone();
        let session = pdf_session.clone();
        spawn(async move {
            let result =
                result_pdf_with_one_refresh(&api, &session, &account.access_token, company_id, id)
                    .await;
            if pdf_generation() != generation
                || lifecycle_epoch() != operation_epoch
                || company_generation() != operation_company_generation
            {
                return;
            }
            pdf_busy.set(None);
            match result {
                Ok(bytes) => {
                    if save_assessment_pdf(&bytes, &item.local_date).await.is_err() {
                        pdf_error.set(Some("Не удалось сохранить PDF.".into()));
                    }
                }
                Err(problem) => pdf_error.set(Some(safe_error(&problem).into())),
            }
        });
    });

    rsx! {
        section { class: "attempt-page", aria_labelledby: "assigned-assessments-title",
            header { class: "attempt-hero home-measurement-hero",
                div {
                    p { class: "management-eyebrow", "МОИ ОЦЕНКИ" }
                    h1 { id: "assigned-assessments-title", "Оценки и черновики" }
                    p { "Продолжите назначенную оценку или начните новый замер." }
                }
                button { class: "btn-primary home-measurement-cta", r#type: "button", onclick: move |_| on_measure.call(()), "Сделать замер" }
            }
            if view == AssessmentListView::History {
                div { class: "assessment-period-proposal",
                    label {
                        span { "История завершённых оценок" }
                        select {
                            value: history_period(),
                            onchange: move |event| {
                                history_period.set(event.value());
                                history_request_cursor.set(None);
                                history_cursor.set(None);
                                list_generation += 1;
                            },
                            option { value: "all", "Все время" }
                            option { value: "yesterday", "Вчера" }
                            option { value: "previous_week", "Прошлая неделя" }
                            option { value: "previous_month", "Прошлый месяц" }
                            option { value: "custom", "Свой период" }
                        }
                    }
                    if history_period() == "custom" {
                        label { span { "С" } input { r#type: "date", value: history_from(), onchange: move |event| { history_from.set(event.value()); history_request_cursor.set(None); history_cursor.set(None); list_generation += 1; } } }
                        label { span { "По" } input { r#type: "date", value: history_to(), onchange: move |event| { history_to.set(event.value()); history_request_cursor.set(None); history_cursor.set(None); list_generation += 1; } } }
                    }
                    p { "Период применяется к завершённой истории по дате отправки." }
                }
            }
            if view == AssessmentListView::Planned {
                article { class: "planned-metrics-empty", role: "status",
                    strong { "Плановые показатели" }
                    p { "Пока нет утверждённых targets и формулы. Значения не подменяются демонстрационными данными." }
                }
            }
            div { class: if error().is_some() || pdf_error().is_some() { "account-live account-live--error" } else { "account-live" }, role: if error().is_some() || pdf_error().is_some() { "alert" } else { "status" }, aria_live: if error().is_some() || pdf_error().is_some() { "assertive" } else { "polite" },
                if let Some(message) = error().or_else(|| pdf_error()) { "{message}" }
            }
            if loading() { p { class: "account-muted", "Загрузка оценок..." } }
            if error().is_some() { button { class: "btn-secondary", r#type: "button", onclick: move |_| list_generation += 1, "Повторить" } }
            if !loading() && view == AssessmentListView::Active && assignments().iter().all(|item| !matches!(item.status.as_str(), "assigned" | "in_progress")) {
                div { class: "account-empty", p { "Активных оценок пока нет." } }
            }
            if !loading() && view == AssessmentListView::History && history_items().is_empty() {
                div { class: "account-empty", p { "В выбранном периоде история пуста." } }
            }
            if !loading() && view != AssessmentListView::Planned {
                    if view == AssessmentListView::Active {
                        section { class: "attempt-assignment-group", aria_labelledby: "active-assessments-title",
                            h2 { id: "active-assessments-title", "Активные" }
                            div { class: "attempt-assignment-list",
                                for item in assignments().into_iter().filter(|item| matches!(item.status.as_str(), "assigned" | "in_progress")) {
                                    AssignmentCard { key: "active-{item.id}", item, opening: opening(), on_open: open_assignment.clone() }
                                }
                            }
                        }
                    }
                    if view == AssessmentListView::History {
                        section { class: "attempt-assignment-group", aria_labelledby: "assessment-history-title",
                            h2 { id: "assessment-history-title", "История" }
                            for (day_label, items) in grouped_history(&history_items()) {
                                section { class: "assessment-history-day", aria_label: "{day_label}",
                                    h3 { "{day_label}" }
                                    div { class: "attempt-assignment-list",
                                        for item in items {
                                            HistoryCard { key: "history-{item.attempt_id}", item, pdf_busy: pdf_busy(), on_open: open_result.clone(), on_pdf: download_pdf.clone() }
                                        }
                                    }
                                }
                            }
                            if let Some(cursor) = history_cursor() {
                                button { class: "btn-secondary assessment-history-more", r#type: "button", disabled: history_loading_more(), onclick: move |_| { history_request_cursor.set(Some(cursor.clone())); list_generation += 1; },
                                    if history_loading_more() { "Загрузка..." } else { "Показать ещё" }
                                }
                            }
                        }
                    }
            }
        }
    }
}

fn result_answer_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => if *value { "Да" } else { "Нет" }.into(),
        Value::Number(value) => value.to_string(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        _ => "—".into(),
    }
}

#[component]
pub fn AssessmentResultPage(attempt_id: Uuid, on_back: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentAttemptApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut operation_generation = use_signal(|| 0_u64);
    let mut reload_generation = use_signal(|| 0_u64);
    let mut result = use_signal(|| None::<AssessmentResultDetail>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut pdf_busy = use_signal(|| false);

    let load_session = session.clone();
    let load_api = api.clone();
    use_effect(move || {
        let _reload = reload_generation();
        let state = load_session.state();
        let Some((token, company_id)) = (match state {
            AccountSessionState::Authenticated(value) => value
                .selected_company
                .map(|company| (value.access_token, company.0)),
            _ => None,
        }) else {
            loading.set(false);
            error.set(Some("Сессия недоступна. Войдите снова.".into()));
            return;
        };
        operation_generation.with_mut(|value| *value += 1);
        let current_generation = *operation_generation.peek();
        let operation_epoch = lifecycle_epoch();
        loading.set(true);
        error.set(None);
        result.set(None);
        let api = load_api.clone();
        let session = load_session.clone();
        spawn(async move {
            let loaded =
                result_with_one_refresh(&api, &session, &token, company_id, attempt_id).await;
            if operation_generation() != current_generation || lifecycle_epoch() != operation_epoch
            {
                return;
            }
            loading.set(false);
            match loaded {
                Ok(value) if value.company_id == company_id && value.attempt_id == attempt_id => {
                    result.set(Some(value));
                }
                Ok(_) => error.set(Some("Результат недоступен.".into())),
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });

    let download_session = session.clone();
    let download_api = api.clone();
    let download = EventHandler::new(move |_| {
        if pdf_busy() {
            return;
        }
        let AccountSessionState::Authenticated(account) = download_session.state() else {
            error.set(Some("Сессия недоступна.".into()));
            return;
        };
        let Some(company_id) = account.selected_company.map(|value| value.0) else {
            error.set(Some("Выберите организацию.".into()));
            return;
        };
        let local_date = result()
            .and_then(|value| value.local_submitted_at.get(0..10).map(str::to_string))
            .unwrap_or_default();
        pdf_busy.set(true);
        error.set(None);
        operation_generation.with_mut(|value| *value += 1);
        let current_generation = *operation_generation.peek();
        let operation_epoch = lifecycle_epoch();
        let api = download_api.clone();
        let session = download_session.clone();
        spawn(async move {
            let loaded = result_pdf_with_one_refresh(
                &api,
                &session,
                &account.access_token,
                company_id,
                attempt_id,
            )
            .await;
            if operation_generation() != current_generation || lifecycle_epoch() != operation_epoch
            {
                return;
            }
            pdf_busy.set(false);
            match loaded {
                Ok(bytes) if save_assessment_pdf(&bytes, &local_date).await.is_ok() => {}
                Ok(_) => error.set(Some("Не удалось сохранить PDF.".into())),
                Err(problem) => error.set(Some(safe_error(&problem).into())),
            }
        });
    });

    rsx! {
        section { class: "attempt-page assessment-result-page", aria_labelledby: "assessment-result-title",
            button { class: "btn-ghost assessment-result-back", r#type: "button", onclick: move |_| on_back.call(()), "← Вернуться в историю" }
            if loading() { p { class: "account-muted", role: "status", "Загрузка результа..." } }
            if let Some(message) = error() {
                div { class: "account-live account-live--error", role: "alert", "{message}" }
                button { class: "btn-secondary", r#type: "button", onclick: move |_| reload_generation += 1, "Повторить" }
            }
            if let Some(value) = result() {
                header { class: "attempt-result-header",
                    div {
                        p { class: "management-eyebrow", "ИТОГОВЫЙ РЕЗУЛЬТАТ" }
                        h1 { id: "assessment-result-title", "{value.template_name}" }
                        p { "Завершён · {human_datetime(&value.local_submitted_at)}" }
                        if let Some(venue) = value.venue_name.as_ref() { p { "Ресторан: {venue}" } }
                        p { "Объект оценки: {value.subject_name}" }
                    }
                    if let Some(score) = value.score_display.as_ref() { strong { class: "assessment-result-score", "{score}" } }
                }
                div { class: "attempt-result-actions",
                    button { class: "btn-primary", r#type: "button", disabled: pdf_busy(), onclick: move |_| download.call(()), if pdf_busy() { "Подготовка PDF..." } else { "Скачать PDF" } }
                }
                if value.critical_failure_count > 0 || value.stop_factor_count > 0 {
                    div { class: "journey-warning", role: "status", "Критических отклонений: {value.critical_failure_count}; стоп-факторов: {value.stop_factor_count}." }
                }
                for (section_index, section) in value.sections.iter().enumerate() {
                    section { class: "assessment-result-section", aria_labelledby: "result-section-{section_index}",
                        header {
                            h2 { id: "result-section-{section_index}", "{section.title}" }
                            if let Some(score) = section.score_display.as_ref() { strong { "{score}" } }
                        }
                        div { class: "assessment-result-answers",
                            for (item_index, item) in section.items.iter().enumerate() {
                                article { class: "assessment-result-answer", key: "answer-{section_index}-{item_index}",
                                    h3 { "{item.prompt}" }
                                    p { class: "assessment-result-answer__value", "{result_answer_text(&item.value)}" }
                                    if let Some(comment) = item.comment.as_ref() { p { class: "assessment-result-answer__comment", "Комментарий: {comment}" } }
                                }
                            }
                        }
                    }
                }
                if !value.related_tasks.is_empty() {
                    section { class: "assessment-result-section", aria_labelledby: "assessment-result-tasks",
                        h2 { id: "assessment-result-tasks", "Связанные задачи" }
                        ul { for task in value.related_tasks.iter() { li { "{task.title} · {task.status}" } } }
                    }
                }
            }
        }
    }
}

#[component]
fn AssignmentCard(
    item: AssignmentSummary,
    opening: Option<Uuid>,
    on_open: EventHandler<Uuid>,
) -> Element {
    let id = item.id;
    let action = assignment_action(&item.status, item.read_only);
    rsx! {
        article { class: "attempt-assignment-card",
            div {
                h3 { "{item.template_name}" }
                p { "{assignment_status(&item.status, item.read_only)}" }
                small { "Назначено: {human_datetime(&item.assigned_at)}" }
                if let Some(submitted) = item.submitted_at.as_ref() { small { "Отправлено: {human_datetime(submitted)}" } }
                if let Some(due) = item.due_at.as_ref() { small { "Срок: {human_datetime(due)}" } }
            }
            if let Some(action) = action {
                button { class: "btn-primary", r#type: "button", disabled: opening.is_some(), onclick: move |_| on_open.call(id),
                    if opening == Some(id) { "Открытие..." } else { "{action.label()}" }
                }
            }
        }
    }
}

#[component]
fn HistoryCard(
    item: AssessmentHistoryItem,
    pdf_busy: Option<Uuid>,
    on_open: EventHandler<Uuid>,
    on_pdf: EventHandler<Uuid>,
) -> Element {
    let id = item.attempt_id;
    let open_label = format!(
        "{}; {}; {}; {}",
        item.template_name,
        assignment_status(&item.status, true),
        item.day_label,
        item.score_display
            .clone()
            .unwrap_or_else(|| "без показателя".into())
    );
    rsx! {
        article {
            class: "attempt-assignment-card assessment-history-card",
            role: if item.has_result { "button" } else { "group" },
            tabindex: if item.has_result { "0" } else { "-1" },
            aria_label: "{open_label}",
            onclick: move |_| if item.has_result { on_open.call(id); },
            onkeydown: move |event| {
                let key = event.key();
                if item.has_result
                    && (key == Key::Enter || key == Key::Character(" ".into()))
                {
                    event.prevent_default();
                    on_open.call(id);
                }
            },
            div {
                h4 { "{item.template_name}" }
                p { "{assignment_status(&item.status, true)}" }
                small { "{human_datetime(&item.local_event_at)}" }
                if let Some(venue) = item.venue_name.as_ref() { small { "Ресторан: {venue}" } }
                small { "Объект оценки: {item.subject_name}" }
            }
            div { class: "assessment-history-card__actions",
                if let Some(score) = item.score_display.as_ref() { strong { class: "assessment-history-score", "{score}" } }
                if item.pdf_available {
                    button {
                        class: "btn-secondary assessment-history-pdf",
                        r#type: "button",
                        aria_label: "Скачать PDF результа {item.template_name}",
                        disabled: pdf_busy.is_some(),
                        onclick: move |event| { event.stop_propagation(); on_pdf.call(id); },
                        onkeydown: move |event| event.stop_propagation(),
                        if pdf_busy == Some(id) { "Подготовка PDF..." } else { "Скачать PDF" }
                    }
                } else {
                    span { class: "account-muted", "PDF недоступен" }
                }
            }
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
            "expired" => "Истекла",
            _ => "Недоступна",
        }
    }
}

fn scroll_to_attempt_section(index: usize) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(section) = document.get_element_by_id(&format!("attempt-section-{index}")) else {
        return;
    };
    section.scroll_into_view();
}

fn next_unanswered_item_id(
    attempt: &AttemptDocument,
    answers: &BTreeMap<Uuid, AttemptAnswer>,
    current_item_id: Uuid,
) -> Option<Uuid> {
    let items = attempt
        .document
        .sections
        .iter()
        .flat_map(|section| section.items.iter())
        .collect::<Vec<_>>();
    let current = items.iter().position(|item| item.id == current_item_id)?;
    items
        .iter()
        .skip(current + 1)
        .chain(items.iter().take(current))
        .find(|item| !answers.contains_key(&item.id))
        .map(|item| item.id)
}

fn focus_attempt_element(id: &str) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(element) = document.get_element_by_id(id) else {
        return;
    };
    element.scroll_into_view();
    if let Some(element) = element.dyn_ref::<web_sys::HtmlElement>() {
        let _ = element.focus();
    }
}

fn schedule_attempt_focus(id: String) {
    spawn(async move {
        TimeoutFuture::new(0).await;
        focus_attempt_element(&id);
    });
}

fn visible_attempt_section(section_count: usize) -> Option<usize> {
    let document = web_sys::window()?.document()?;
    let container_top = document
        .get_element_by_id("attempt-sections")?
        .get_bounding_client_rect()
        .top();
    (0..section_count).min_by(|left, right| {
        let distance = |index: usize| {
            document
                .get_element_by_id(&format!("attempt-section-{index}"))
                .map(|section| (section.get_bounding_client_rect().top() - container_top).abs())
                .unwrap_or(f64::MAX)
        };
        distance(*left).total_cmp(&distance(*right))
    })
}

#[component]
fn AttemptTimer() -> Element {
    let mut seconds = use_signal(|| 0_u64);
    use_effect(move || {
        spawn(async move {
            loop {
                TimeoutFuture::new(1_000).await;
                seconds += 1;
            }
        });
    });
    let minutes = seconds() / 60;
    let remainder = seconds() % 60;
    rsx! {
        div { class: "attempt-stopwatch", role: "timer", aria_label: "Время прохождения {minutes} минут {remainder} секунд",
            span { class: "attempt-stopwatch__orbit", aria_hidden: "true" }
            strong { "{minutes:02}:{remainder:02}" }
            small { "Время замера" }
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
    on_team: EventHandler<()>,
    on_result: EventHandler<Uuid>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentAttemptApiClient>();
    let workflow_api = use_context::<OrganizationWorkflowApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let read_only = active.read_only || active.status == "submitted" || completion().is_some();
    let mut dragged_section = use_signal(|| None::<Uuid>);
    let mut quick_task_open = use_signal(|| false);
    let mut quick_task_title = use_signal(String::new);
    let mut quick_task_description = use_signal(String::new);
    let mut quick_task_busy = use_signal(|| false);
    let mut quick_task_error = use_signal(|| None::<String>);
    let mut quick_task_created = use_signal(|| false);
    let result_details_open = use_signal(|| false);
    let mut completion_task_reload = use_signal(|| 0_u64);
    let rendered_order = draft()
        .map(|state| state.section_order)
        .unwrap_or_else(|| normalized_section_order(&active.document.sections, &[]));
    let rendered_sections = ordered_sections(&active.document.sections, &rendered_order);
    let sections = &rendered_sections;
    let section_index = current_section().min(sections.len().saturating_sub(1));
    let section_count = sections.len();
    let total = sections
        .iter()
        .map(|section| section.items.len())
        .sum::<usize>();
    let answered = draft().map(|state| state.answers.len()).unwrap_or(0);
    let status = draft()
        .map(|state| state.status)
        .unwrap_or(SaveStatus::Saved);
    let section_answers = draft().map(|state| state.answers).unwrap_or_default();
    let retry_active = active.clone();
    let retry_api = api.clone();
    let retry_session = session.clone();
    let overwrite_active = active.clone();
    let overwrite_api = api.clone();
    let overwrite_session = session.clone();
    let submit_active = active.clone();
    let submit_api = api.clone();
    let submit_session = session.clone();
    let order_active = active.clone();
    let order_api = api.clone();
    let order_session = session.clone();
    let current_section_id = sections.get(section_index).map(|section| section.id);
    let completion_task_api = workflow_api.clone();
    let completion_task_session = session.clone();
    let completion_attempt_id = active.id;
    let completion_tasks = use_resource(move || {
        let completed = completion().is_some();
        let _reload = completion_task_reload();
        let api = completion_task_api.clone();
        let session = completion_task_session.clone();
        async move {
            if !completed {
                return None;
            }
            let AccountSessionState::Authenticated(account) = session.state() else {
                return None;
            };
            let company_id = account.selected_company?.0;
            Some(
                api.tasks(&account.access_token, company_id, "created")
                    .await
                    .map(|tasks| {
                        tasks
                            .into_iter()
                            .filter(|task| {
                                task.assessment_attempt_id == Some(completion_attempt_id)
                            })
                            .collect::<Vec<_>>()
                    }),
            )
        }
    });
    let change_order = EventHandler::new(move |section_order: Vec<Uuid>| {
        if read_only {
            return;
        }
        let changed = draft.write().as_mut().is_some_and(|state| {
            state.change_section_order(section_order.clone(), js_sys::Date::now())
        });
        if !changed {
            return;
        }
        if let Some(active_id) = current_section_id {
            if let Some(index) = section_order.iter().position(|id| *id == active_id) {
                current_section.set(index);
            }
        }
        save_generation += 1;
        schedule_autosave(
            order_active.clone(),
            draft,
            save_generation,
            attempt_generation,
            lifecycle_epoch,
            order_api.clone(),
            order_session.clone(),
            false,
            submit_generation,
            submit_requested,
            submitting,
            confirming_submit,
            completion,
        );
    });

    rsx! { section { class: "attempt-editor", aria_labelledby: "attempt-title",
        header { class: "attempt-editor-header",
            div { class: "attempt-editor-toolbar",
                button { class: "btn-ghost attempt-editor-back", r#type: "button", onclick: move |_| { attempt_generation += 1; attempt.set(None); draft.set(None); completion.set(None); }, "← К замерам" }
                div { class: "attempt-editor-heading", h1 { id: "attempt-title", "Прохождение оценки" } p { "Отвечено {answered} из {total}" } }
                if !read_only { button { class: "attempt-quick-task-button", r#type: "button", onclick: move |_| { quick_task_error.set(None); quick_task_created.set(false); quick_task_open.set(true); }, "+ Быстрая задача" } }
                div { class: "attempt-save-status", role: "status", aria_live: "polite", "{save_label(status)}" }
            }
            progress { class: "attempt-editor-progress", max: "{total.max(1)}", value: "{answered.min(total)}", aria_label: "Отвечено {answered} из {total}" }
        }
        if let Some(reason) = active.read_only_reason.as_ref() { p { class: "account-safe-error", "{read_only_message(reason)}" } }
        if completion().is_none() {
            div { class: "attempt-editor-layout",
                nav { class: "attempt-section-nav", aria_label: "Разделы оценки",
                    for (index, section) in sections.iter().enumerate() {
                        {
                            let section_id = section.id;
                            let progress = section_progress(section, &section_answers, index == section_index);
                            let order_for_up = rendered_order.clone();
                            let order_for_down = rendered_order.clone();
                            let order_for_drop = rendered_order.clone();
                            let move_up = change_order;
                            let move_down = change_order;
                            let drop_change = change_order;
                            rsx! {
                                div {
                                    key: "{section_id}",
                                    class: if index == section_index { "attempt-section-nav-item active" } else { "attempt-section-nav-item" },
                                    ondragover: move |event| event.prevent_default(),
                                    ondrop: move |event| {
                                        event.prevent_default();
                                        if let Some(source) = dragged_section() {
                                            if let Some(updated) = drop_section_before(&order_for_drop, source, section_id) {
                                                drop_change.call(updated);
                                            }
                                        }
                                        dragged_section.set(None);
                                    },
                                    button {
                                        class: "attempt-section-nav-main",
                                        aria_current: (index == section_index).then_some("step"),
                                        r#type: "button",
                                        onclick: move |_| {
                                            current_section.set(index);
                                            scroll_to_attempt_section(index);
                                        },
                                        strong { "{section.title}" }
                                        span { class: "attempt-section-nav-status", "{section_progress_label(progress)}" }
                                    }
                                    div { class: "attempt-section-order-actions", aria_label: "Изменить порядок раздела {section.title}",
                                        button {
                                            class: "attempt-section-drag",
                                            r#type: "button",
                                            draggable: "true",
                                            disabled: read_only,
                                            aria_label: "Перетащить раздел {section.title}",
                                            ondragstart: move |_| dragged_section.set(Some(section_id)),
                                            ondragend: move |_| dragged_section.set(None),
                                            "⠿"
                                        }
                                        button {
                                            r#type: "button",
                                            disabled: read_only || index == 0,
                                            aria_label: "Поднять раздел {section.title}",
                                            onclick: move |_| if let Some(updated) = move_section(&order_for_up, section_id, -1) { move_up.call(updated); },
                                            "↑"
                                        }
                                        button {
                                            r#type: "button",
                                            disabled: read_only || index + 1 == section_count,
                                            aria_label: "Опустить раздел {section.title}",
                                            onclick: move |_| if let Some(updated) = move_section(&order_for_down, section_id, 1) { move_down.call(updated); },
                                            "↓"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { id: "attempt-sections", class: "attempt-sections", onscroll: move |_| if let Some(index) = visible_attempt_section(section_count) { current_section.set(index); },
                    for (index, section) in sections.iter().enumerate() {
                        section { id: "attempt-section-{index}", class: "attempt-section", h2 { "{section.title}" } if let Some(description) = section.description.as_ref() { p { "{description}" } }
                            for item in section.items.iter() { AnswerControl { key: "{item.id}", item: item.clone(), read_only, draft, active: active.clone(), attempt_generation, save_generation, submit_generation, submit_requested, submitting, confirming_submit, completion } }
                        }
                    }
                }
            }
            AttemptTimer {}
        }
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
        if let Some(result) = completion() {
            div {
                class: "attempt-result",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "attempt-complete-title",
                h2 { id: "attempt-complete-title", "Замер завершён и сохранён" }
                p { "Замер: оценка по выбранному шаблону" }
                p { "Ресторан: выбранный для назначения" }
                p { "Отправлено: только что" }
                p { "Ответов: {result.answered_count} из {result.total_count}; обязательных: {result.required_count}" }
                if let Some(score) = result.score_percent.as_deref() {
                    { let score_display = format_percent(score).unwrap_or_else(|| "—".into()); rsx! { p { "Итоговый показатель: {score_display}" } } }
                    if let Some(coverage) = result.coverage.as_deref() {
                        p { "Полнота данных: {coverage}" }
                    }
                    if result.critical_failure_count.unwrap_or_default() > 0 {
                        p {
                            class: "attempt-error",
                            "Критические нарушения: {result.critical_failure_count.unwrap_or_default()}"
                        }
                    }
                    if result.stop_factor_count.unwrap_or_default() > 0 {
                        p {
                            class: "attempt-error",
                            "Стоп-факторы: {result.stop_factor_count.unwrap_or_default()}"
                        }
                    }
                    if result_details_open() && !result.sections.is_empty() {
                        ul { class: "attempt-section-results",
                            for section in result.sections.iter() {
                                li {
                                    if let Some(score) = section.score_percent.as_deref() {
                                        { let score_display = format_percent(score).unwrap_or_else(|| "—".into()); rsx! { strong { "{section.title}: {score_display}" } } }
                                    } else {
                                        strong { "{section.title}: нет данных" }
                                    }
                                    span { " · покрытие {section.coverage}" }
                                    if section.critical_failure_count > 0 {
                                        span { class: "attempt-error", " · критических отклонений {section.critical_failure_count}" }
                                    }
                                    if section.stop_factor_count > 0 {
                                        span { class: "attempt-error", " · стоп-факторов {section.stop_factor_count}" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    p { "Результат зафиксирован по полноте заполнения." }
                }
                match completion_tasks() {
                    Some(Some(Ok(tasks))) if !tasks.is_empty() => rsx! {
                        section { class: "attempt-completion-tasks", aria_labelledby: "attempt-completion-tasks-title",
                            h3 { id: "attempt-completion-tasks-title", "Проверьте задачи" }
                            ul { class: "task-list",
                                for task in tasks.iter() {
                                    li { key: "completion-task-{task.task_id}-{task.version}",
                                        details { class: "task-detail", open: task.status == "draft",
                                            summary { strong { "{task.title}" } span { class: "status-chip", "{task.status}" } }
                                            if let Some(description) = task.description.clone() { p { "{description}" } }
                                            if task.status == "draft" {
                                                DraftTaskActions { task: task.clone(), on_changed: move |_| completion_task_reload += 1 }
                                            }
                                        }
                                    }
                                }
                            }
                            p { "Черновики не видны сотрудникам, пока вы явно не назначите их." }
                        }
                    },
                    Some(Some(Err(_))) => rsx! { p { class: "journey-warning", role: "status", "Замер сохранён. Список связанных задач временно не загрузился." } },
                    _ => rsx! {},
                }
                div { class: "attempt-result-actions",
                    button { class: "btn-primary", r#type: "button", onclick: move |_| { attempt_generation += 1; attempt.set(None); draft.set(None); completion.set(None); }, "Готово" }
                    button { class: "btn-secondary", r#type: "button", onclick: move |_| on_result.call(active.id), "Посмотреть результат" }
                    if completion_tasks().is_some_and(|value| value.is_some_and(|result| result.is_ok_and(|tasks| !tasks.is_empty()))) {
                        button { class: "btn-secondary", r#type: "button", onclick: move |_| on_team.call(()), "Проверить и отправить задачи" }
                    }
                }
                p { "Результат сохранён в RestOS и доступен ответственному руководителю." }
            }
        }
        else if !read_only { button { id: "attempt-complete-action", class: "btn-primary attempt-complete-action", r#type: "button", disabled: submit_preparation(status, read_only, submitting()) == SubmitPreparation::Blocked, onclick: move |_| confirming_submit.set(true), "Завершить оценку" } }
        if confirming_submit() { div { class: "attempt-confirm", role: "dialog", aria_modal: "true", aria_labelledby: "submit-confirm-title", h2 { id: "submit-confirm-title", "Отправить оценку?" } p { "После отправки ответы нельзя будет изменить." }
            button { class: "btn-primary", r#type: "button", disabled: submitting(), onclick: move |_| {
                submitting.set(true);
                submit_requested.set(true);
                submit_generation += 1;
                let live_status = draft().map(|state| state.status).unwrap_or(SaveStatus::Saved);
                match submit_preparation(live_status, read_only, false) {
                    SubmitPreparation::SubmitNow => start_submit(submit_active.id, submit_api.clone(), submit_session.clone(), attempt_generation, lifecycle_epoch, submit_generation, submit_requested, submitting, confirming_submit, completion, draft),
                    SubmitPreparation::SaveFirst => schedule_autosave(submit_active.clone(), draft, save_generation, attempt_generation, lifecycle_epoch, submit_api.clone(), submit_session.clone(), true, submit_generation, submit_requested, submitting, confirming_submit, completion),
                    SubmitPreparation::Blocked => release_submit_intent(submit_requested, submitting, confirming_submit),
                }
            }, if submitting() { "Подготовка..." } else { "Подтвердить" } }
            button { class: "btn-ghost", r#type: "button", disabled: submitting(), onclick: move |_| confirming_submit.set(false), "Отмена" }
        } }
        if quick_task_open() { div { class: "attempt-confirm quick-task-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "quick-task-title",
            h2 { id: "quick-task-title", "Быстрая задача" }
            p { "Задача будет связана с текущим замером и сохранена в черновики." }
            div { class: "form-field", label { class: "field-label", r#for: "quick-task-name", "Что нужно сделать" } input { id: "quick-task-name", class: "field-input", maxlength: "255", value: "{quick_task_title}", disabled: quick_task_busy(), oninput: move |event| { quick_task_title.set(event.value()); quick_task_error.set(None); quick_task_created.set(false); } } }
            div { class: "form-field", label { class: "field-label", r#for: "quick-task-description", "Описание — необязательно" } textarea { id: "quick-task-description", class: "field-input", maxlength: "4000", value: "{quick_task_description}", disabled: quick_task_busy(), oninput: move |event| quick_task_description.set(event.value()) } }
            if quick_task_created() { p { class: "journey-success", role: "status", "Задача сохранена в черновики." } }
            if let Some(message) = quick_task_error() { p { class: "journey-error", role: "alert", "{message}" } }
            button { class: "btn-primary", r#type: "button", disabled: quick_task_busy() || quick_task_title().trim().is_empty(), onclick: move |_| {
                let AccountSessionState::Authenticated(account) = session.state() else { quick_task_error.set(Some("Сессия недоступна. Войдите снова.".into())); return; };
                let Some(company_id) = account.selected_company.map(|value| value.0) else { quick_task_error.set(Some("Выберите организацию.".into())); return; };
                let Some(request_id) = browser_request_id() else { quick_task_error.set(Some("Не удалось подготовить безопасный запрос.".into())); return; };
                quick_task_busy.set(true); quick_task_error.set(None); let task_api = workflow_api.clone(); let attempt_api = api.clone(); let token = account.access_token; let assignment_id = active.assignment_id; let attempt_id = active.id;
                let title = quick_task_title().trim().to_string(); let description = (!quick_task_description().trim().is_empty()).then(|| quick_task_description().trim().to_string());
                spawn(async move {
                    let result = async {
                        let assignment = attempt_api.get_assignment(&token, assignment_id).await.map_err(|problem| match problem { AssessmentAttemptApiError::NetworkUnavailable => OrganizationWorkflowApiError::NetworkUnavailable, AssessmentAttemptApiError::AuthenticationRequired => OrganizationWorkflowApiError::AuthenticationRequired, _ => OrganizationWorkflowApiError::NotFound })?;
                        let venue_id = assignment.venue_id.ok_or(OrganizationWorkflowApiError::InvalidRequest)?;
                        task_api.create_task(&token, company_id, &CreateTaskRequest { request_id, venue_id, assessment_attempt_id: Some(attempt_id), title, description }).await
                    }.await;
                    match result { Ok(_) => { quick_task_title.set(String::new()); quick_task_description.set(String::new()); quick_task_created.set(true); }, Err(problem) => quick_task_error.set(Some(safe_task_error(&problem).into())) }
                    quick_task_busy.set(false);
                });
            }, if quick_task_busy() { "Сохранение…" } else { "Сохранить в черновики" } }
            button { class: "btn-secondary", r#type: "button", disabled: quick_task_busy(), onclick: move |_| quick_task_open.set(false), "Закрыть" }
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
    let evidence_mode = item.evidence_mode;
    let option_ids = item
        .options
        .iter()
        .map(|option| option.id)
        .collect::<BTreeSet<_>>();
    let change_active = active.clone();
    let advance_active = active.clone();
    let change_api = api.clone();
    let change_session = session.clone();
    let comment_active = active.clone();
    let comment_api = api.clone();
    let comment_session = session.clone();
    let change = EventHandler::new(move |value: Value| {
        if read_only {
            return;
        }
        if submit_requested() {
            submit_requested.set(false);
            submitting.set(false);
            submit_generation += 1;
        }
        let mut answer =
            answer_for_parts(item_id, &change_answer_type, max_length, &option_ids, value);
        let mut required_comment = false;
        let mut next_item = None;
        let changed = draft.write().as_mut().is_some_and(|state| {
            if let Some(answer) = answer.as_mut() {
                answer.comment = state
                    .answers
                    .get(&item_id)
                    .and_then(|current| current.comment.clone());
            }
            let changed = state.change(answer, item_id, js_sys::Date::now());
            if changed && change_answer_type == "boolean" {
                required_comment = matches!(
                    evidence_mode,
                    crate::assessment_attempt_api::EvidenceMode::RequiredComment
                        | crate::assessment_attempt_api::EvidenceMode::PhotoAndComment
                ) && state
                    .answers
                    .get(&item_id)
                    .and_then(|answer| answer.comment.as_deref())
                    .is_none_or(|comment| comment.trim().is_empty());
                if !required_comment {
                    next_item = next_unanswered_item_id(&advance_active, &state.answers, item_id);
                }
            }
            changed
        });
        if !changed {
            return;
        }
        save_generation += 1;
        schedule_autosave(
            change_active.clone(),
            draft,
            save_generation,
            attempt_generation,
            lifecycle_epoch,
            change_api.clone(),
            change_session.clone(),
            false,
            submit_generation,
            submit_requested,
            submitting,
            confirming_submit,
            completion,
        );
        if change_answer_type == "boolean" {
            if required_comment {
                schedule_attempt_focus(format!("attempt-observation-{item_id}"));
            } else if let Some(next_item) = next_item {
                schedule_attempt_focus(format!("attempt-question-{next_item}"));
            } else {
                schedule_attempt_focus("attempt-complete-action".into());
            }
        }
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
    let current_comment = current
        .as_ref()
        .and_then(|answer| answer.comment.clone())
        .unwrap_or_default();
    let comment_item_id = item_id;
    let mut comment_change = move |value: String| {
        if read_only || value.chars().count() > DEFAULT_TEXT_LIMIT {
            return;
        }
        let changed = draft.write().as_mut().is_some_and(|state| {
            let Some(answer) = state.answers.get(&comment_item_id).cloned() else {
                return false;
            };
            let mut updated = answer;
            updated.comment = (!value.is_empty()).then_some(value);
            state.change(Some(updated), comment_item_id, js_sys::Date::now())
        });
        if !changed {
            return;
        }
        save_generation += 1;
        schedule_autosave(
            comment_active.clone(),
            draft,
            save_generation,
            attempt_generation,
            lifecycle_epoch,
            comment_api.clone(),
            comment_session.clone(),
            false,
            submit_generation,
            submit_requested,
            submitting,
            confirming_submit,
            completion,
        );
    };
    let question_class = match current.as_ref().and_then(|answer| answer.value.as_bool()) {
        Some(true) if answer_type == "boolean" => "attempt-question attempt-question--positive",
        Some(false) if answer_type == "boolean" => "attempt-question attempt-question--negative",
        _ => "attempt-question",
    };
    rsx! { fieldset { id: "attempt-question-{item_id}", class: question_class, tabindex: "-1", disabled: read_only, legend { "{label}" } if let Some(guidance) = item.guidance.as_ref() { p { class: "attempt-guidance", "{guidance}" } }
        match answer_type.as_str() {
            "boolean" => rsx! { div { class: "attempt-boolean-actions",
                button { class: if current.as_ref().and_then(|a| a.value.as_bool()) == Some(true) { "is-selected is-positive" } else { "" }, r#type: "button", aria_pressed: current.as_ref().and_then(|a| a.value.as_bool()) == Some(true), onclick: move |_| boolean_change.call(json!(true)), "✓ Да" }
                button { class: if current.as_ref().and_then(|a| a.value.as_bool()) == Some(false) { "is-selected is-negative" } else { "" }, r#type: "button", aria_pressed: current.as_ref().and_then(|a| a.value.as_bool()) == Some(false), onclick: move |_| boolean_change.call(json!(false)), "× Нет" }
                button { r#type: "button", disabled: current.is_none(), onclick: move |_| {
                    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
                        if let Some(field) = document.get_element_by_id(&format!("attempt-observation-{item_id}")) {
                            let _ = field.dyn_ref::<web_sys::HtmlElement>().map(|element| element.focus());
                        }
                    }
                }, "+ Наблюдение" }
            } },
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
        if answer_type == "boolean" || matches!(item.evidence_mode, crate::assessment_attempt_api::EvidenceMode::OptionalComment | crate::assessment_attempt_api::EvidenceMode::RequiredComment | crate::assessment_attempt_api::EvidenceMode::PhotoAndComment) {
            label { class: "attempt-comment",
                span { if item.evidence_mode == crate::assessment_attempt_api::EvidenceMode::RequiredComment { "Комментарий · обязательно" } else { "Комментарий" } }
                textarea { id: "attempt-observation-{item_id}", maxlength: "{DEFAULT_TEXT_LIMIT}", value: "{current_comment}", placeholder: "Зафиксируйте наблюдение", oninput: move |event| comment_change(event.value()) }
            }
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
    mut confirming_submit: Signal<bool>,
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
            release_submit_intent(submit_requested, submitting, confirming_submit);
            return;
        };
        if submit_after_save(state_snapshot.status, submit_requested()) {
            start_submit(
                active.id,
                api,
                session,
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
        if state_snapshot.status != SaveStatus::Dirty {
            release_submit_intent(submit_requested, submitting, confirming_submit);
            return;
        }
        let payload = match state_snapshot.payload(&active) {
            Ok(payload) => payload,
            Err(()) => {
                if let Some(state) = draft.write().as_mut() {
                    state.status = SaveStatus::ValidationError;
                }
                release_submit_intent(submit_requested, submitting, confirming_submit);
                return;
            }
        };
        let snapshot_generation = state_snapshot.dirty_generation;
        let token = match session.state() {
            AccountSessionState::Authenticated(value) => value.access_token,
            _ => {
                if let Some(state) = draft.write().as_mut() {
                    state.in_flight = false;
                    state.status = SaveStatus::Failed;
                }
                release_submit_intent(submit_requested, submitting, confirming_submit);
                return;
            }
        };
        if let Some(state) = draft.write().as_mut() {
            state.in_flight = true;
            state.status = SaveStatus::Saving;
        }
        let result = bounded_request(replace_draft_with_one_refresh(
            &api, &session, &token, active.id, &payload,
        ))
        .await
        .unwrap_or(Err(AssessmentAttemptApiError::NetworkUnavailable));
        if attempt_generation() != scheduled_attempt_generation
            || lifecycle_epoch() != scheduled_lifecycle_epoch
        {
            if submit_requested() {
                submit_requested.set(false);
                submitting.set(false);
                confirming_submit.set(false);
            }
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
                release_submit_intent(submit_requested, submitting, confirming_submit);
            }
            Err(_) => {
                if let Some(state) = draft.write().as_mut() {
                    state.in_flight = false;
                    state.status = SaveStatus::Failed;
                }
                release_submit_intent(submit_requested, submitting, confirming_submit);
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
        let (token, company_id) = match session.state() {
            AccountSessionState::Authenticated(value) => {
                let Some(company_id) = value.selected_company.map(|id| id.0) else {
                    submit_requested.set(false);
                    submitting.set(false);
                    confirming_submit.set(false);
                    if let Some(state) = draft.write().as_mut() {
                        state.status = SaveStatus::SubmitFailed;
                    }
                    return;
                };
                (value.access_token, company_id)
            }
            _ => {
                submit_requested.set(false);
                submitting.set(false);
                confirming_submit.set(false);
                if let Some(state) = draft.write().as_mut() {
                    state.status = SaveStatus::SubmitFailed;
                }
                return;
            }
        };
        let submitted =
            bounded_request(submit_with_one_refresh(&api, &session, &token, attempt_id)).await;
        let result = match submitted {
            Some(Ok(value)) => Ok(value),
            Some(Err(problem)) => {
                match bounded_request(result_with_one_refresh(
                    &api, &session, &token, company_id, attempt_id,
                ))
                .await
                {
                    Some(Ok(value)) if value.status == "submitted" => {
                        Ok(completion_from_result(value))
                    }
                    _ => Err(problem),
                }
            }
            None => {
                match bounded_request(result_with_one_refresh(
                    &api, &session, &token, company_id, attempt_id,
                ))
                .await
                {
                    Some(Ok(value)) if value.status == "submitted" => {
                        Ok(completion_from_result(value))
                    }
                    _ => Err(AssessmentAttemptApiError::NetworkUnavailable),
                }
            }
        };
        if attempt_generation() != operation_attempt_generation
            || lifecycle_epoch() != operation_lifecycle_epoch
            || submit_generation() != operation_submit_generation
            || !submit_requested()
        {
            if submit_generation() == operation_submit_generation {
                submit_requested.set(false);
                submitting.set(false);
                confirming_submit.set(false);
            }
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
            weight: Some("1".into()),
            min_value: None,
            max_value: None,
            passing_value: None,
            evidence_mode: crate::assessment_attempt_api::EvidenceMode::None,
            criticality: crate::assessment_attempt_api::Criticality::Normal,
            config: crate::assessment_attempt_api::InputConfig {
                placeholder: None,
                max_length: Some(20),
                critical_threshold: None,
            },
            options: vec![crate::assessment_attempt_api::AssessmentOption {
                id: OPTION_ID,
                label: "A".into(),
                sort_order: 1,
            }],
        }
    }

    fn assignment(id: u128, status: &str) -> AssignmentSummary {
        AssignmentSummary {
            id: Uuid::from_u128(id),
            company_id: Uuid::from_u128(100),
            venue_id: None,
            status: status.into(),
            assigned_at: "2026-08-15T19:28:00+03:00".into(),
            due_at: None,
            submitted_at: None,
            template_name: "Синтетический замер".into(),
            template_version: 1,
            read_only: false,
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
            ui_metadata: Default::default(),
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
    fn weighted_comment_evidence_survives_the_single_draft_payload() {
        let attempt = test_attempt_with_item(item("boolean"));
        let mut answer = answer_for(&item("boolean"), json!(false)).unwrap();
        answer.comment = Some("Нарушение зафиксировано".into());
        let mut state = DraftState::from_attempt(&attempt);
        assert!(state.change(Some(answer), ITEM_ID, 100.0));
        let payload = state.payload(&attempt).unwrap();
        assert_eq!(
            payload.answers[0].comment.as_deref(),
            Some("Нарушение зафиксировано")
        );
        assert_eq!(payload.answers.len(), 1);
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
    fn assessment_history_completion_removes_active_assignment_once() {
        let mut items = vec![assignment(1, "in_progress"), assignment(2, "assigned")];
        assert_eq!(
            remove_completed_assignment(&mut items, Uuid::from_u128(1)),
            1
        );
        assert_eq!(items, vec![assignment(2, "assigned")]);
        assert_eq!(
            remove_completed_assignment(&mut items, Uuid::from_u128(1)),
            0
        );
    }

    #[wasm_bindgen_test]
    fn assessment_history_dates_are_human_readable_and_hide_timezone() {
        assert_eq!(
            human_datetime("2026-08-15T19:28:00+03:00"),
            "15 августа 2026, 19:28"
        );
        assert_eq!(human_datetime("2026-08-15"), "15 августа 2026");
        assert_eq!(human_datetime("Europe/Moscow"), "Дата недоступна");
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
    fn completion_requests_have_a_finite_ui_timeout() {
        assert_eq!(ASSESSMENT_REQUEST_TIMEOUT_MS, 20_000);
        assert!(ASSESSMENT_REQUEST_TIMEOUT_MS < 60_000);
    }

    #[wasm_bindgen_test]
    fn boolean_auto_advance_targets_the_next_unanswered_item() {
        let mut first = item("boolean");
        first.id = Uuid::from_u128(11);
        let mut second = item("boolean");
        second.id = Uuid::from_u128(12);
        let mut third = item("boolean");
        third.id = Uuid::from_u128(13);
        let mut attempt = test_attempt();
        attempt
            .document
            .sections
            .push(crate::assessment_attempt_api::AssessmentSection {
                id: Uuid::from_u128(10),
                parent_section_id: None,
                title: "Section".into(),
                description: None,
                sort_order: 0,
                items: vec![first.clone(), second.clone(), third.clone()],
            });
        let mut answers = BTreeMap::new();
        answers.insert(
            first.id,
            answer_for(&first, json!(true)).expect("valid synthetic answer"),
        );
        assert_eq!(
            next_unanswered_item_id(&attempt, &answers, first.id),
            Some(second.id)
        );
        answers.insert(
            second.id,
            answer_for(&second, json!(false)).expect("valid synthetic answer"),
        );
        assert_eq!(
            next_unanswered_item_id(&attempt, &answers, second.id),
            Some(third.id)
        );
        answers.insert(
            third.id,
            answer_for(&third, json!(true)).expect("valid synthetic answer"),
        );
        assert_eq!(next_unanswered_item_id(&attempt, &answers, third.id), None);
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
            ui_metadata: Default::default(),
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
                comment: None,
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
                comment: None,
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

    #[wasm_bindgen_test]
    fn measurement_navigation_order_is_a_bounded_non_scoring_permutation() {
        let first = Uuid::from_u128(101);
        let second = Uuid::from_u128(102);
        let third = Uuid::from_u128(103);
        let canonical = vec![first, second, third];
        assert_eq!(
            move_section(&canonical, second, -1),
            Some(vec![second, first, third])
        );
        assert_eq!(move_section(&canonical, first, -1), None);
        assert_eq!(
            drop_section_before(&canonical, third, first),
            Some(vec![third, first, second])
        );
        assert_eq!(drop_section_before(&canonical, first, first), None);
    }

    #[wasm_bindgen_test]
    fn measurement_navigation_status_is_textual_and_not_color_only() {
        let required = item("text");
        let section = AssessmentSection {
            id: Uuid::from_u128(201),
            parent_section_id: None,
            title: "Очень длинное название раздела".into(),
            description: None,
            sort_order: 0,
            items: vec![required.clone()],
        };
        let mut answers = BTreeMap::new();
        assert_eq!(
            section_progress(&section, &answers, false),
            SectionProgress::NotStarted
        );
        assert!(section_progress_label(SectionProgress::NotStarted).contains("Не начато"));
        answers.insert(required.id, answer_for(&required, json!("готово")).unwrap());
        assert_eq!(
            section_progress(&section, &answers, false),
            SectionProgress::Complete
        );
        assert!(section_progress_label(SectionProgress::Complete).contains("Заполнено"));
    }

    #[wasm_bindgen_test]
    fn measurement_navigation_order_round_trips_through_draft_payload() {
        let mut attempt = test_attempt();
        attempt.document.sections = vec![
            AssessmentSection {
                id: Uuid::from_u128(301),
                parent_section_id: None,
                title: "Первый".into(),
                description: None,
                sort_order: 0,
                items: vec![],
            },
            AssessmentSection {
                id: Uuid::from_u128(302),
                parent_section_id: None,
                title: "Второй".into(),
                description: None,
                sort_order: 1,
                items: vec![],
            },
        ];
        let mut state = DraftState::from_attempt(&attempt);
        assert!(
            state.change_section_order(vec![Uuid::from_u128(302), Uuid::from_u128(301)], 100.0,)
        );
        let payload = state.payload(&attempt).unwrap();
        assert_eq!(
            payload.section_order,
            vec![Uuid::from_u128(302), Uuid::from_u128(301)]
        );
        assert!(payload.answers.is_empty());
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
            ui_metadata: Default::default(),
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
