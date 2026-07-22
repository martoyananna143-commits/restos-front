//! Evaluation form — redesigned to match new UI spec.
//! Phase 1 (Select): card-based employee + criterion set + eval type selection.
//! Phase 2 (Fill): sticky progress header, collapsible question cards, score ring.
//! Phase 3 (Success): result card with score breakdown.

use dioxus::prelude::*;
use crate::api;
use crate::types::{
    Answer, AnswerValue, Criterion, CriterionSetOption, Employee,
    EvaluationTypeOption, StartEvaluationRequest, SubmitRequest,
};

// ── Phase state ──────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Phase {
    Select,
    Loading,
    Fill {
        evaluation_id:   i64,
        criteria:        Vec<Criterion>,
        employee_name:   String,
        set_name:        String,
        initial_answers: Vec<Answer>,
    },
    Success { evaluation_id: i64, score: f64, employee_name: String, set_name: String, total: usize },
    Error(String),
}

// ── Root component ───────────────────────────────────────────────────────────

#[component]
pub fn EvaluationForm(
    token: String,
    #[props(default)]
    resume_evaluation_id: Option<i64>,
    on_done: EventHandler<()>,
    on_back: EventHandler<()>,
) -> Element {
    match resume_evaluation_id {
        Some(id) if id > 0 => rsx! {
            EvalFormResume {
                token: token.clone(),
                evaluation_id: id,
                on_done,
                on_back,
            }
        },
        _ => rsx! {
            EvalFormInner {
                token: token.clone(),
                on_done,
                on_back,
            }
        },
    }
}

#[component]
fn EvalFormInner(
    token: String,
    on_done: EventHandler<()>,
    on_back: EventHandler<()>,
) -> Element {
    let mut phase = use_signal(|| Phase::Select);

    // EvaluationForm never unmounts — phase (Signal, Copy) is always valid
    // inside spawn async blocks.  We build the on_confirm handler here so it
    // can drive the API call and update phase without any component-boundary issues.
    let tok = token.clone();
    let on_confirm = move |(emp_id, type_id, set_id, ename, sname): (i64, i64, i64, String, String)| {
        phase.set(Phase::Loading);
        let tok2 = tok.clone();
        let ename2 = ename.clone();
        let sname2 = sname.clone();
        spawn(async move {
            let req = StartEvaluationRequest {
                evaluated_employee_id: emp_id,
                criterion_set_id:      set_id,
                evaluation_type_id:    type_id,
            };
            match api::start_evaluation(&tok2, req).await {
                Ok(resp) => phase.set(Phase::Fill {
                    evaluation_id: resp.evaluation_id,
                    criteria:      resp.criteria,
                    employee_name: ename2,
                    set_name:      sname2,
                    initial_answers: vec![],
                }),
                Err(e) => phase.set(Phase::Error(e)),
            }
        });
    };

    let current = phase.read().clone();
    match current {
        Phase::Select => rsx! {
            SelectPhase {
                token: token.clone(),
                on_confirm,
                on_back,
            }
        },
        Phase::Loading => rsx! {
            div { class: "app-screen",
                div { class: "screen-scroll",
                    div { style: "display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:300px;gap:12px;",
                        div { class: "loader-ring" }
                        div { class: "text2", "Подготовка замера..." }
                    }
                }
            }
        },
        Phase::Fill {
            evaluation_id,
            criteria,
            employee_name,
            set_name,
            initial_answers,
        } => rsx! {
            FillPhase {
                token: token.clone(),
                evaluation_id,
                criteria,
                employee_name: employee_name.clone(),
                set_name: set_name.clone(),
                initial_answers: initial_answers.clone(),
                on_done: move |(score, ename, sname, total): (f64, String, String, usize)| {
                    phase.set(Phase::Success {
                        evaluation_id,
                        score,
                        employee_name: ename,
                        set_name: sname,
                        total,
                    });
                },
                on_error: move |msg: String| phase.set(Phase::Error(msg)),
                on_back:  move |_| phase.set(Phase::Select),
            }
        },
        Phase::Success { evaluation_id, score, employee_name, set_name, total } => rsx! {
            SuccessScreen {
                token: token.clone(),
                evaluation_id,
                score,
                employee_name,
                set_name,
                total,
                on_done,
                on_new: move |_| phase.set(Phase::Select),
            }
        },
        Phase::Error(msg) => rsx! {
            div { class: "app-screen",
                div { class: "screen-scroll",
                    div { style: "padding: 40px 18px; display:flex; flex-direction:column; align-items:center; gap:16px;",
                        div { style: "font-size:40px;", "⚠️" }
                        div { style: "font-size:15px; color:var(--red); text-align:center;", "{msg}" }
                        button {
                            class: "btn-primary",
                            style: "margin-top:8px;",
                            onclick: move |_| phase.set(Phase::Select),
                            "Попробовать снова"
                        }
                    }
                }
            }
        },
    }
}

#[component]
fn EvalFormResume(
    token: String,
    evaluation_id: i64,
    on_done: EventHandler<()>,
    on_back: EventHandler<()>,
) -> Element {
    let t = token.clone();
    let res = use_resource(move || {
        let tok = t.clone();
        let eid = evaluation_id;
        async move { api::fetch_evaluation_resume(&tok, eid).await }
    });
    let mut success = use_signal(|| None::<(f64, String, String, usize)>);

    if let Some((score, ename, sname, total)) = success() {
        return rsx! {
            SuccessScreen {
                token: token.clone(),
                evaluation_id,
                score,
                employee_name: ename,
                set_name: sname,
                total,
                on_done,
                on_new: move |_| on_back.call(()),
            }
        };
    }

    rsx! {
        match res() {
            None => rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        div { style: "display:flex;flex-direction:column;align-items:center;justify-content:center;min-height:300px;gap:12px;",
                            div { class: "loader-ring" }
                            div { class: "text2", "Загрузка черновика..." }
                        }
                    }
                }
            },
            Some(Err(e)) => rsx! {
                div { class: "app-screen",
                    div { class: "screen-scroll",
                        div { style: "padding: 40px 18px; display:flex; flex-direction:column; align-items:center; gap:16px;",
                            div { style: "font-size:40px;", "⚠️" }
                            div { style: "font-size:15px; color:var(--red); text-align:center;", "{e}" }
                            button {
                                class: "btn-primary",
                                style: "margin-top:8px;",
                                onclick: move |_| on_back.call(()),
                                "Назад"
                            }
                        }
                    }
                }
            },
            Some(Ok(r)) => rsx! {
                FillPhase {
                    token: token.clone(),
                    evaluation_id: r.evaluation_id,
                    criteria: r.criteria.clone(),
                    employee_name: r.evaluated_employee_name.clone(),
                    set_name: r.criterion_set_name.clone(),
                    initial_answers: r.saved_answers.clone(),
                    on_done: move |(sc, en, sn, tot)| success.set(Some((sc, en, sn, tot))),
                    on_error: move |_| {},
                    on_back: move |_| on_back.call(()),
                }
            },
        }
    }
}

// ── Phase 1: Select ──────────────────────────────────────────────────────────

#[component]
fn SelectPhase(
    token: String,
    on_confirm: EventHandler<(i64, i64, i64, String, String)>,
    on_back: EventHandler<()>,
) -> Element {
    let t1 = token.clone();
    let employees = use_resource(move || {
        let tok = t1.clone();
        async move { api::fetch_employees(&tok).await }
    });

    let t2 = token.clone();
    let eval_types = use_resource(move || {
        let tok = t2.clone();
        async move { api::fetch_evaluation_types(&tok).await }
    });

    let t3 = token.clone();
    let crit_sets = use_resource(move || {
        let tok = t3.clone();
        async move { api::fetch_criterion_sets(&tok, false).await }
    });

    let mut selected_emp:  Signal<Option<i64>> = use_signal(|| None);
    let mut selected_type: Signal<Option<i64>> = use_signal(|| None);
    let mut selected_set:  Signal<Option<i64>> = use_signal(|| None);
    let mut emp_search = use_signal(String::new);

    let emps_ok  = employees.read().as_ref().and_then(|r| r.as_ref().ok().cloned()).unwrap_or_default();
    let types_ok = eval_types.read().as_ref().and_then(|r| r.as_ref().ok().cloned()).unwrap_or_default();
    let sets_ok  = crit_sets.read().as_ref().and_then(|r| r.as_ref().ok().cloned()).unwrap_or_default();
    let loading  = employees.read().is_none() || eval_types.read().is_none() || crit_sets.read().is_none();

    // Auto-select default criterion set
    use_effect(move || {
        if selected_set() .is_none() {
            if let Some(def) = crit_sets.read().as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|sets| sets.iter().find(|s| s.is_default))
            {
                selected_set.set(Some(def.id));
            }
        }
    });
    // Auto-select first eval type if only one
    use_effect(move || {
        if selected_type().is_none() {
            if let Some(t) = eval_types.read().as_ref()
                .and_then(|r| r.as_ref().ok())
                .and_then(|types| if types.len() == 1 { types.first() } else { None })
            {
                selected_type.set(Some(t.id));
            }
        }
    });

    let selected_count = [
        selected_emp().is_some(),
        selected_set().is_some(),
        selected_type().is_some(),
    ].iter().filter(|&&b| b).count();
    let step_total = 3;
    let can_start = selected_emp().is_some() && selected_set().is_some()
        && selected_type().is_some();
    let mini_pct = selected_count * 100 / step_total.max(1);

    let emps_for_closure = emps_ok.clone();
    let sets_for_closure = sets_ok.clone();
    let types_for_closure = types_ok.clone();
    let on_submit = move |_: Event<MouseData>| {
        let emp_id  = match selected_emp()  { Some(v) => v, None => return };
        let set_id  = match selected_set()  { Some(v) => v, None => return };
        // Use explicitly selected type, or auto-pick first available
        let type_id = match selected_type()
            .or_else(|| types_for_closure.first().map(|t| t.id))
        {
            Some(v) => v,
            None => return,
        };

        let ename = emps_for_closure.iter().find(|e| e.id == emp_id)
            .map(|e| e.full_name.clone()).unwrap_or_default();
        let sname = sets_for_closure.iter().find(|s| s.id == set_id)
            .map(|s| s.name.clone()).unwrap_or_default();

        // Pass selection back to EvaluationForm — no async here, no unmounting issues.
        on_confirm.call((emp_id, type_id, set_id, ename, sname));
    };

    // Filter employees by search
    let search_val = emp_search.read().to_lowercase();
    let filtered_emps: Vec<&Employee> = emps_ok.iter()
        .filter(|e| search_val.is_empty() || e.full_name.to_lowercase().contains(&search_val))
        .collect();

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll", style: "padding-bottom: 24px;",

                // ── Header ────────────────────────────────
                div { class: "eval-select-header",
                    button {
                        class: "icon-btn",
                        onclick: move |_| on_back.call(()),
                        span { style: "font-size:16px; color:var(--text2);", "←" }
                    }
                    div { style: "flex:1;",
                        div { style: "font-size:11px; color:var(--text3); font-weight:500; letter-spacing:0.05em; text-transform:uppercase;", "Замеры" }
                        div { style: "font-size:18px; font-weight:500; letter-spacing:-0.02em; margin-top:1px;", "Новый замер" }
                    }
                    span { class: "eval-step-badge", "{selected_count} / {step_total}" }
                }

                // Mini progress bar
                div { class: "eval-mini-track",
                    div { class: "eval-mini-fill", style: "width:{mini_pct}%;" }
                }

                if loading {
                    div { style: "display:flex;align-items:center;justify-content:center;min-height:200px;",
                        div { class: "loader-ring" }
                    }
                } else {

                    // ── Кого оцениваем ────────────────────
                    div { style: "padding: 16px 18px 0;",
                        div { class: "eval-section-label", "Кого оцениваем" }
                        div { class: "eval-search-wrap",
                            span { class: "eval-search-icon", "🔍" }
                            input {
                                class: "eval-search-inp",
                                r#type: "text",
                                placeholder: "Поиск по имени...",
                                value: emp_search.read().clone(),
                                oninput: move |e| emp_search.set(e.value()),
                            }
                        }
                        div { class: "eval-person-list",
                            if filtered_emps.is_empty() {
                                div { style: "font-size:13px; color:var(--text3); padding:12px 0;", "Сотрудники не найдены" }
                            }
                            for emp in filtered_emps.iter() {
                                {
                                    let eid = emp.id;
                                    let is_sel = selected_emp() == Some(eid);
                                    let initials = emp.full_name.split_whitespace()
                                        .take(2).filter_map(|w| w.chars().next())
                                        .collect::<String>();
                                    let position = emp.position.clone().unwrap_or_default();
                                    rsx! {
                                        div {
                                            class: if is_sel { "eval-person-card selected" } else { "eval-person-card" },
                                            onclick: move |_| selected_emp.set(Some(eid)),
                                            div { class: "eval-av", "{initials}" }
                                            div { style: "flex:1;",
                                                div { style: "font-size:14px; font-weight:500;", "{emp.full_name}" }
                                                if !position.is_empty() {
                                                    div { style: "font-size:12px; color:var(--text3);", "{position}" }
                                                }
                                            }
                                            div { class: "check-circle",
                                                if is_sel { span { "✓" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Набор критериев ───────────────────
                    div { style: "padding: 18px 18px 0;",
                        div { class: "eval-section-label", "Набор критериев" }
                        div { class: "eval-person-list",
                            for s in sets_ok.iter() {
                                {
                                    let sid = s.id;
                                    let is_sel = selected_set() == Some(sid);
                                    let s_name = s.name.clone();
                                    let s_desc = s.description.clone().unwrap_or_default();
                                    let s_default = s.is_default;
                                    let q_count = s.criterion_ids.as_ref().map(|ids| ids.len()).unwrap_or(0);
                                    let q_label = if q_count == 1 { "1 вопрос".to_string() } else { format!("{q_count} вопросов") };
                                    rsx! {
                                        div {
                                            class: if is_sel { "eval-person-card selected" } else { "eval-person-card" },
                                            onclick: move |_| selected_set.set(Some(sid)),
                                            span { style: "font-size:18px;", "📦" }
                                            div { style: "flex:1;",
                                                div { style: "display:flex; align-items:center; gap:7px;",
                                                    span { style: "font-size:14px; font-weight:500;", "{s_name}" }
                                                    if s_default {
                                                        span { class: "badge badge-amber", style: "font-size:10px;", "По умолч." }
                                                    }
                                                }
                                                div { style: "font-size:12px; color:var(--text3); margin-top:2px;",
                                                    if !s_desc.is_empty() {
                                                        "{s_desc} · {q_label}"
                                                    } else {
                                                        "{q_label}"
                                                    }
                                                }
                                            }
                                            div { class: "check-circle",
                                                if is_sel { span { "✓" } }
                                            }
                                        }
                                    }
                                }
                            }
                            if sets_ok.is_empty() {
                                div { style: "font-size:13px; color:var(--text3); padding:12px 0;", "Наборы критериев не найдены" }
                            }
                        }
                    }

                    // ── Тип оценки ──
                    if types_ok.is_empty() {
                        div { style: "padding: 12px 18px 0;",
                            div {
                                style: "font-size:12px; color:var(--amber); background:var(--amber-bg,#fff8e1); border-radius:8px; padding:8px 12px;",
                                "⚠ Нет типов замеров. Создайте тип в разделе «Настройки»."
                            }
                        }
                    }
                    if !types_ok.is_empty() {
                        div { style: "padding: 18px 18px 0;",
                            div { class: "eval-section-label", "Тип замера" }
                            div { class: "eval-type-row",
                                for t in types_ok.iter() {
                                    {
                                        let tid = t.id;
                                        let is_sel = selected_type() == Some(tid);
                                        let t_name = t.name.clone();
                                        let t_desc = t.description.clone().unwrap_or_default();
                                        rsx! {
                                            div {
                                                class: if is_sel { "eval-type-card selected" } else { "eval-type-card" },
                                                onclick: move |_| selected_type.set(Some(tid)),
                                                span { style: "font-size:18px;", "🏷" }
                                                div { style: "flex:1;",
                                                    div { style: "font-size:14px; font-weight:500;", "{t_name}" }
                                                    if !t_desc.is_empty() {
                                                        div { style: "font-size:12px; color:var(--text3); margin-top:2px;", "{t_desc}" }
                                                    }
                                                }
                                                div { class: "check-circle",
                                                    if is_sel { span { "✓" } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── Actions ───────────────────────────
                    div { style: "padding: 20px 18px 0; display:flex; gap:10px;",
                        button {
                            class: "btn-ghost",
                            style: "flex:1;",
                            onclick: move |_| on_back.call(()),
                            "Отмена"
                        }
                        button {
                            class: "btn-primary",
                            style: "flex:2;",
                            disabled: !can_start,
                            onclick: on_submit,
                            "Начать замер →"
                        }
                    }
                }
            }
        }
    }
}

// ── Phase 2: Fill ────────────────────────────────────────────────────────────

/// Commit open text fields to `value` so progress / submit match what the user typed (incl. without blur).
fn flush_text_answers_to_value(states: &mut [CriterionState]) {
    for s in states.iter_mut() {
        if matches!(s.criterion.value_type.as_str(), "boolean" | "number") {
            continue;
        }
        let t = s.text_buffer.read().trim().to_string();
        if t.is_empty() {
            s.value.set(None);
        } else {
            s.value.set(Some(AnswerValue::Text(t.clone())));
            s.text_buffer.set(t);
        }
    }
}

#[derive(Clone, PartialEq)]
struct CriterionState {
    criterion: Criterion,
    value:     Signal<Option<AnswerValue>>,
    /// For text/string criteria: live textarea text; `value` is updated on blur (and before submit).
    text_buffer: Signal<String>,
    comment:   Signal<Option<String>>,
}

#[component]
fn FillPhase(
    token:         String,
    evaluation_id: i64,
    criteria:      Vec<Criterion>,
    employee_name: String,
    set_name:      String,
    #[props(default)]
    initial_answers: Vec<Answer>,
    on_done:  EventHandler<(f64, String, String, usize)>,
    on_error: EventHandler<String>,
    on_back:  EventHandler<()>,
) -> Element {
    let mut states_sig: Signal<Vec<CriterionState>> = use_signal(Vec::new);
    use_effect(move || {
        if states_sig.read().is_empty() && !criteria.is_empty() {
            states_sig.set(
                criteria
                    .iter()
                    .map(|c| {
                        let (init_v, init_c) = initial_answers
                            .iter()
                            .find(|a| a.criterion_id == c.id)
                            .map(|a| (Some(a.value.clone()), a.comment.clone()))
                            .unwrap_or((None, None));
                        let init_buf = match &init_v {
                            Some(AnswerValue::Text(t)) => t.clone(),
                            _ => String::new(),
                        };
                        CriterionState {
                            criterion: c.clone(),
                            value: Signal::new(init_v),
                            text_buffer: Signal::new(init_buf),
                            comment: Signal::new(init_c),
                        }
                    })
                    .collect(),
            );
        }
    });
    let states = states_sig.read().clone();
    let mut submitting = use_signal(|| false);
    let mut saving_draft = use_signal(|| false);
    let mut submit_err = use_signal(|| None::<String>);
    let total = states.len();

    // Read answered count (subscribes parent to all value signals)
    let answered = states.iter().filter(|s| s.value.read().is_some()).count();
    let all_done = answered == total && total > 0;
    let pct = if total > 0 { answered * 100 / total } else { 0 };

    // Compute score preview
    let (score_sum, bool_yes, bool_total, num_sum, num_count, text_answered) =
        states.iter().fold((0.0_f64, 0usize, 0usize, 0.0_f64, 0usize, 0usize), |acc, s| {
            let (mut ss, mut by, mut bt, mut ns, mut nc, mut ta) = acc;
            match s.criterion.value_type.as_str() {
                "boolean" => {
                    bt += 1;
                    if let Some(AnswerValue::Boolean(b)) = s.value.read().clone() {
                        if b { by += 1; ss += 100.0; }
                    }
                }
                "number" => {
                    if let Some(AnswerValue::Number(n)) = s.value.read().clone() {
                        ns += n; nc += 1; ss += n / 5.0 * 100.0;
                    }
                }
                _ => {
                    if s.value.read().is_some() { ta += 1; ss += 100.0; }
                }
            }
            (ss, by, bt, ns, nc, ta)
        });

    let preview_pct = if answered > 0 { score_sum / answered as f64 } else { 0.0 };
    let num_avg = if num_count > 0 { num_sum / num_count as f64 } else { 0.0 };

    // SVG ring
    let circumference = 131.9_f64;
    let ring_color = if all_done { "rgba(74,222,128,0.85)" } else { "rgba(245,166,35,0.8)" };
    let ring_text_color = if all_done { "rgba(74,222,128,0.9)" } else { "rgba(245,166,35,0.9)" };
    let ring_offset = circumference * (1.0 - preview_pct / 100.0);

    let tok  = token.clone();
    let ename = employee_name.clone();
    let sname = set_name.clone();
    let mut states_clone = states.clone();

    let on_submit = move |_: Event<MouseData>| {
        if *submitting.read() { return; }
        submitting.set(true);
        submit_err.set(None);
        flush_text_answers_to_value(&mut states_clone);
        let answers: Vec<Answer> = states_clone.iter().filter_map(|s| {
            let v = s.value.read().clone()?;
            Some(Answer {
                criterion_id: s.criterion.id,
                value: v,
                comment: s.comment.read().clone(),
            })
        }).collect();
        let req = SubmitRequest { answers, comment: None };
        let tok2 = tok.clone();
        let en2  = ename.clone();
        let sn2  = sname.clone();
        let tot2 = total;
        spawn(async move {
            match api::submit_evaluation(&tok2, evaluation_id, req).await {
                Ok(resp) => on_done.call((resp.score_percentage, en2, sn2, tot2)),
                Err(e)   => {
                    submitting.set(false);
                    submit_err.set(Some(e.clone()));
                    on_error.call(e);
                }
            }
        });
    };

    let states_draft = states.clone();
    let tok_draft = token.clone();
    let eid_draft = evaluation_id;
    let on_back_draft = on_back;

    let on_save_draft = {
        let mut states_draft = states_draft.clone();
        let tok_draft = tok_draft.clone();
        let eid_draft = eid_draft;
        move |_: Event<MouseData>| {
            if *saving_draft.read() {
                return;
            }
            saving_draft.set(true);
            submit_err.set(None);
            flush_text_answers_to_value(&mut states_draft);
            let answers: Vec<Answer> = states_draft
                .iter()
                .filter_map(|s| {
                    let v = s.value.read().clone()?;
                    Some(Answer {
                        criterion_id: s.criterion.id,
                        value: v,
                        comment: s.comment.read().clone(),
                    })
                })
                .collect();
            let req = SubmitRequest {
                answers,
                comment: None,
            };
            let tok2 = tok_draft.clone();
            let eid = eid_draft;
            spawn(async move {
                let _ = api::save_evaluation_draft(&tok2, eid, &req).await;
                saving_draft.set(false);
            });
        }
    };

    let on_back_save = {
        let mut states_draft = states_draft.clone();
        let tok_draft = tok_draft.clone();
        let eid_draft = eid_draft;
        let on_back_draft = on_back_draft;
        move |_: Event<MouseData>| {
            if *saving_draft.read() {
                return;
            }
            saving_draft.set(true);
            submit_err.set(None);
            flush_text_answers_to_value(&mut states_draft);
            let answers: Vec<Answer> = states_draft
                .iter()
                .filter_map(|s| {
                    let v = s.value.read().clone()?;
                    Some(Answer {
                        criterion_id: s.criterion.id,
                        value: v,
                        comment: s.comment.read().clone(),
                    })
                })
                .collect();
            let req = SubmitRequest {
                answers,
                comment: None,
            };
            let tok2 = tok_draft.clone();
            let eid = eid_draft;
            spawn(async move {
                let _ = api::save_evaluation_draft(&tok2, eid, &req).await;
                saving_draft.set(false);
                on_back_draft.call(());
            });
        }
    };

    let remaining = total.saturating_sub(answered);

    rsx! {
        div { class: "app-screen",

            if let Some(em) = submit_err() {
                div {
                    style: "padding:10px 16px; margin:8px 12px 0; background:rgba(239,68,68,0.12); border-radius:8px; font-size:13px; color:var(--red);",
                    "{em}"
                }
            }

            // ── Sticky progress header ─────────────────
            div {
                class: if all_done { "prog-header" } else { "prog-header" },
                style: if all_done { "border-bottom-color: rgba(74,222,128,0.2);" } else { "" },
                div { class: "prog-header-row",
                    div { style: "display:flex; align-items:center; gap:10px;",
                        button {
                            class: "prog-back-btn",
                            onclick: on_back_save,
                            "←"
                        }
                        div {
                            div { style: "font-size:13px; font-weight:500; color:var(--text);", "{employee_name}" }
                            div { style: "font-size:11px; color:var(--text3);", "{set_name}" }
                        }
                    }
                    if all_done {
                        span { class: "badge badge-green", style: "font-size:11px;", "{answered} / {total} ✓" }
                    } else {
                        div { style: "text-align:right;",
                            div { style: "font-size:13px; font-weight:500; color:var(--amber);", "{answered} / {total}" }
                            div { style: "font-size:11px; color:var(--text3);", "отвечено" }
                        }
                    }
                }
                div { class: "prog-track", style: "margin-top:10px;",
                    div {
                        class: if all_done { "prog-fill prog-fill-done" } else { "prog-fill" },
                        style: "width:{pct}%;"
                    }
                }
            }

            // ── Scrollable questions ───────────────────
            div { class: "screen-scroll", style: "padding-bottom: 0;",
                div { class: "q-list",
                    for (idx, state) in states.iter().enumerate() {
                        {
                            let s = state.clone();
                            rsx! { CriterionCard { state: s, index: idx } }
                        }
                    }
                }

                // ── Score preview ──────────────────────
                if answered > 0 {
                    div {
                        class: if all_done { "score-preview score-done" } else { "score-preview" },
                        style: "margin-bottom: 0;",
                        svg {
                            width: "52", height: "52", view_box: "0 0 52 52",
                            style: "flex-shrink:0;",
                            circle {
                                cx: "26", cy: "26", r: "21",
                                fill: "none", stroke: "rgba(255,255,255,0.07)", stroke_width: "4"
                            }
                            circle {
                                cx: "26", cy: "26", r: "21",
                                fill: "none", stroke: "{ring_color}", stroke_width: "4",
                                stroke_dasharray: "131.9",
                                stroke_dashoffset: "{ring_offset:.1}",
                                stroke_linecap: "round",
                                transform: "rotate(-90 26 26)"
                            }
                            text {
                                x: "26", y: "31", text_anchor: "middle",
                                font_size: "12", font_weight: "500",
                                fill: "{ring_text_color}",
                                font_family: "DM Sans, sans-serif",
                                "{preview_pct:.0}%"
                            }
                        }
                        div { style: "flex:1;",
                            if all_done {
                                div { style: "font-size:13px; font-weight:500;",
                                    "Итоговый балл: "
                                    span { style: "color:var(--green);", "{preview_pct:.1}%" }
                                }
                                div { style: "font-size:12px; color:var(--text3); margin-top:2px;",
                                    "Все {total} критериев заполнены"
                                }
                            } else {
                                div { style: "font-size:13px; font-weight:500;", "Предварительный балл" }
                                div { style: "font-size:12px; color:var(--text3); margin-top:3px;",
                                    "На основе {answered} из {total} ответов"
                                }
                            }
                            div { style: "display:flex; gap:5px; margin-top:7px; flex-wrap:wrap;",
                                if bool_total > 0 {
                                    span { style: "display:flex;align-items:center;gap:4px;font-size:11px;color:var(--green);",
                                        span { style: "width:6px;height:6px;border-radius:50%;background:var(--green);display:inline-block;" }
                                        "Да/Нет: {bool_yes}/{bool_total}"
                                    }
                                }
                                if num_count > 0 {
                                    span { style: "display:flex;align-items:center;gap:4px;font-size:11px;color:var(--blue);",
                                        span { style: "width:6px;height:6px;border-radius:50%;background:var(--blue);display:inline-block;" }
                                        "Числовое: {num_avg:.1}/5"
                                    }
                                }
                                if text_answered > 0 {
                                    span { style: "display:flex;align-items:center;gap:4px;font-size:11px;color:var(--purple);",
                                        span { style: "width:6px;height:6px;border-radius:50%;background:var(--purple);display:inline-block;" }
                                        "Текст: {text_answered}"
                                    }
                                }
                            }
                        }
                        if all_done {
                            span { class: "badge badge-green", style: "font-size:10px; align-self:flex-start;", "Готово" }
                        } else {
                            span { class: "badge badge-muted", style: "font-size:10px; align-self:flex-start;", "Черновик" }
                        }
                    }
                }

                // ── Submit area ────────────────────────
                div { class: "submit-area",
                    if all_done {
                        button {
                            class: "btn-submit-ready",
                            disabled: *submitting.read(),
                            onclick: on_submit,
                            if *submitting.read() { "Отправка..." } else { "✓ Завершить замер" }
                        }
                    } else {
                        button {
                            class: "btn-primary",
                            style: "opacity:0.5; cursor:not-allowed;",
                            disabled: true,
                            "Завершить замер · осталось {remaining}"
                        }
                    }
                    button {
                        class: "btn-ghost",
                        style: "text-align:center;",
                        disabled: *saving_draft.read(),
                        onclick: on_save_draft,
                        if *saving_draft.read() { "Сохранение..." } else { "Сохранить черновик" }
                    }
                }
            }
        }
    }
}

// ── Question card ────────────────────────────────────────────────────────────

#[component]
fn CriterionCard(state: CriterionState, index: usize) -> Element {
    let mut val     = state.value;
    let text_buffer  = state.text_buffer;
    let mut comment = state.comment;
    let mut collapsed         = use_signal(|| false);
    let mut show_comment_inp  = use_signal(|| false);

    let c = state.criterion.clone();
    let is_answered = val.read().is_some();

    // Auto-collapse when user gives an answer
    use_effect(move || {
        if val.read().is_some() {
            collapsed.set(true);
        }
    });

    let (card_class, num_class) = match (is_answered, c.value_type.as_str()) {
        (true, "number") => ("q-card answered-num", "q-num q-done-num"),
        (true, "text") | (true, "string") => ("q-card answered-str", "q-num q-done-str"),
        (true, _)  => ("q-card answered",     "q-num q-done"),
        (false, _) => ("q-card q-focused",    "q-num q-active"),
    };
    let num_label = if is_answered { "✓".to_string() } else { (index + 1).to_string() };

    if is_answered && collapsed() {
        // ── Compact (answered) view ──────────────────
        rsx! {
            div {
                class: "{card_class}",
                style: "cursor:pointer;",
                onclick: move |_| collapsed.set(false),

                div { class: "q-header", style: "padding-bottom: 10px;",
                    div { class: "{num_class}", "{num_label}" }
                    div { style: "flex:1;",
                        div { class: "q-title", "{c.name}" }
                    }
                    // Inline compact answer
                    {compact_answer_badge(&c.value_type, &val.read())}
                }

                // Compact comment
                {if let Some(cmt) = comment.read().clone() {
                    if !cmt.is_empty() {
                        rsx! {
                            div { style: "padding: 0 14px 11px; padding-left: 46px;",
                                div { class: "q-compact-comment", "«{cmt}»" }
                            }
                        }
                    } else { rsx! {} }
                } else { rsx! {} }}
            }
        }
    } else {
        // ── Expanded view ────────────────────────────
        let type_dot_class = match c.value_type.as_str() {
            "number" => "type-dot type-num",
            "text" | "string" => "type-dot type-str",
            _ => "type-dot type-bool",
        };
        let desc = c.description.clone().unwrap_or_default();

        rsx! {
            div { class: "{card_class}",
                div { class: "q-header",
                    div { class: "{num_class}", "{num_label}" }
                    div { style: "flex:1;",
                        div { class: "q-title", "{c.name}" }
                        if !desc.is_empty() {
                            div { class: "q-desc", "{desc}" }
                        }
                    }
                    span { class: "{type_dot_class}", style: "margin-top:5px;" }
                }
                div { class: "q-body",
                    match c.value_type.as_str() {
                        "boolean" => rsx! { YnInput { value: val } },
                        "number"  => rsx! { ScaleInput { value: val } },
                        _         => rsx! { TextQInput { value: val, text_buffer } },
                    }

                    // Comment section
                    if let Some(cmt) = comment.read().clone() {
                        if !cmt.is_empty() || show_comment_inp() {
                            div { class: "comment-filled",
                                div { class: "comment-label", "Комментарий" }
                                div { style: "display:flex; gap:8px;",
                                    textarea {
                                        class: "q-text-inp",
                                        rows: "2",
                                        placeholder: "Добавьте комментарий...",
                                        value: cmt,
                                        oninput: move |e| {
                                            let v = e.value();
                                            comment.set(if v.is_empty() { None } else { Some(v) });
                                        }
                                    }
                                    button {
                                        r#type: "button",
                                        style: "width:32px; height:32px; flex-shrink:0; background:var(--surface2); border:1px solid var(--border); border-radius:8px; color:var(--text3); cursor:pointer; font-size:12px; font-family:var(--sans); align-self:flex-start;",
                                        onclick: move |_| {
                                            comment.set(None);
                                            show_comment_inp.set(false);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        } else {
                            button {
                                class: "comment-toggle",
                                onclick: move |_| show_comment_inp.set(true),
                                span { style: "font-size:13px;", "＋" }
                                "Добавить комментарий"
                            }
                        }
                    } else if show_comment_inp() {
                        div { class: "comment-filled",
                            div { class: "comment-label", "Комментарий" }
                            div { style: "display:flex; gap:8px;",
                                textarea {
                                    class: "q-text-inp",
                                    rows: "2",
                                    placeholder: "Добавьте комментарий...",
                                    value: "",
                                    oninput: move |e| {
                                        let v = e.value();
                                        comment.set(if v.is_empty() { None } else { Some(v) });
                                    }
                                }
                                button {
                                    r#type: "button",
                                    style: "width:32px; height:32px; flex-shrink:0; background:var(--surface2); border:1px solid var(--border); border-radius:8px; color:var(--text3); cursor:pointer; font-size:12px; font-family:var(--sans); align-self:flex-start;",
                                    onclick: move |_| {
                                        comment.set(None);
                                        show_comment_inp.set(false);
                                    },
                                    "✕"
                                }
                            }
                        }
                    } else {
                        button {
                            class: "comment-toggle",
                            onclick: move |_| show_comment_inp.set(true),
                            span { style: "font-size:13px;", "＋" }
                            "Добавить комментарий"
                        }
                    }
                }
            }
        }
    }
}

fn compact_answer_badge(value_type: &str, val: &Option<AnswerValue>) -> Element {
    match (value_type, val) {
        ("boolean", Some(AnswerValue::Boolean(true))) => rsx! {
            div { class: "yn-compact yn-compact-yes", span { "✓" } "Да" }
        },
        ("boolean", Some(AnswerValue::Boolean(false))) => rsx! {
            div { class: "yn-compact yn-compact-no", span { "✕" } "Нет" }
        },
        ("number", Some(AnswerValue::Number(n))) => {
            let nv = *n as usize;
            rsx! {
                div { style: "display:flex; align-items:center; gap:5px;",
                    span { style: "font-size:18px; font-weight:500; color:var(--blue);", "{n:.0}" }
                    span { style: "font-size:12px; color:var(--text3);", "/5" }
                }
            }
        },
        (_, Some(AnswerValue::Text(t))) => {
            let preview = if t.len() > 20 { format!("{}…", &t[..20]) } else { t.clone() };
            rsx! {
                div { style: "font-size:12px; color:var(--text2); font-style:italic; max-width:100px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;",
                    "«{preview}»"
                }
            }
        },
        _ => rsx! { span {} },
    }
}

// ── Inline input components ──────────────────────────────────────────────────

#[component]
fn YnInput(value: Signal<Option<AnswerValue>>) -> Element {
    let current = value.read().clone();
    let is_yes = matches!(current, Some(AnswerValue::Boolean(true)));
    let is_no  = matches!(current, Some(AnswerValue::Boolean(false)));

    rsx! {
        div { class: "yn-row",
            button {
                r#type: "button",
                class: if is_no { "yn-btn yn-no sel" } else { "yn-btn yn-no" },
                onclick: move |_| value.set(Some(AnswerValue::Boolean(false))),
                span { class: "yn-icon", "✕" }
                "Нет"
            }
            button {
                r#type: "button",
                class: if is_yes { "yn-btn yn-yes sel" } else { "yn-btn yn-yes" },
                onclick: move |_| value.set(Some(AnswerValue::Boolean(true))),
                span { class: "yn-icon", "✓" }
                "Да"
            }
        }
    }
}

#[component]
fn ScaleInput(value: Signal<Option<AnswerValue>>) -> Element {
    let current = value.read().clone();
    rsx! {
        div { class: "scale-row",
            for n in 1..=5_u8 {
                {
                    let nf = n as f64;
                    let is_sel = matches!(current, Some(AnswerValue::Number(v)) if (v - nf).abs() < f64::EPSILON);
                    rsx! {
                        button {
                            r#type: "button",
                            class: if is_sel { "scale-btn sel" } else { "scale-btn" },
                            onclick: move |_| {
                                if matches!(value.read().clone(), Some(AnswerValue::Number(v)) if (v - nf).abs() < f64::EPSILON) {
                                    value.set(None);
                                } else {
                                    value.set(Some(AnswerValue::Number(nf)));
                                }
                            },
                            "{n}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn TextQInput(value: Signal<Option<AnswerValue>>, text_buffer: Signal<String>) -> Element {
    rsx! {
        textarea {
            class: "q-text-inp",
            rows: "3",
            placeholder: "Напишите ваш отзыв...",
            value: "{text_buffer}",
            oninput: move |e| {
                text_buffer.set(e.value());
            },
            onblur: move |_| {
                let t = text_buffer.read().trim().to_string();
                value.set(if t.is_empty() { None } else { Some(AnswerValue::Text(t.clone())) });
                text_buffer.set(t);
            },
        }
    }
}

// ── Success screen ────────────────────────────────────────────────────────────

#[component]
fn SuccessScreen(
    token: String,
    evaluation_id: i64,
    score: f64,
    employee_name: String,
    set_name: String,
    total: usize,
    on_done: EventHandler<()>,
    on_new:  EventHandler<()>,
) -> Element {
    let score_color = if score >= 80.0 { "var(--amber)" } else if score >= 50.0 { "var(--amber)" } else { "var(--red)" };

    // Build date string
    let date_str = {
        use js_sys::Date;
        let d = Date::new_0();
        format!("{:02}.{:02}.{}", d.get_date(), d.get_month() + 1, d.get_full_year())
    };

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll", style: "padding-bottom: 30px;",

                // ── Success header ─────────────────────
                div { style: "padding:32px 18px 0; display:flex; flex-direction:column; align-items:center; text-align:center; gap:12px;",
                    div { class: "success-check", "✓" }
                    div { class: "success-serif",
                        "Замер"
                        br {}
                        em { style: "font-style:italic; color:var(--amber);", "отправлена" }
                    }
                    div { style: "font-size:13px; color:var(--text2); line-height:1.55; max-width:240px;",
                        "Замер сотрудника сохранён и доступен в разделе аналитики"
                    }
                }

                // ── Result card ────────────────────────
                div { class: "success-result-card", style: "margin-top: 24px;",
                    // Employee row
                    div { style: "display:flex; align-items:center; gap:11px; padding-bottom:12px;",
                        div { class: "eval-av", style: "width:44px; height:44px; font-size:14px;",
                            {employee_name.split_whitespace().take(2).filter_map(|w| w.chars().next()).collect::<String>()}
                        }
                        div {
                            div { style: "font-size:15px; font-weight:500;", "{employee_name}" }
                            div { style: "font-size:12px; color:var(--text3);", "{set_name}" }
                        }
                    }
                    div { class: "result-divider" }

                    // Score breakdown
                    div { style: "padding-top:12px; display:flex; flex-direction:column; gap:9px;",
                        div { class: "result-row",
                            span { class: "result-label", "Итоговый балл" }
                            span { class: "result-score", style: "color:{score_color};", "{score:.1}%" }
                        }
                        div { class: "result-row",
                            span { class: "result-label", "Набор критериев" }
                            span { class: "result-value", "{set_name}" }
                        }
                        div { class: "result-row",
                            span { class: "result-label", "Дата" }
                            span { class: "result-value", "{date_str}" }
                        }
                        div { class: "result-row",
                            span { class: "result-label", "Критериев" }
                            span { class: "result-value", "{total} из {total}" }
                        }
                    }
                }

                // ── Export ─────────────────────────────
                {
                    let api_base = crate::api::get_api_base_pub();
                    let excel_url = format!(
                        "{}/api/web/evaluations/{}/export/excel?access_token={}",
                        api_base, evaluation_id, token
                    );
                    let pdf_url = format!(
                        "{}/api/web/evaluations/{}/export/pdf?access_token={}",
                        api_base, evaluation_id, token
                    );
                    rsx! {
                        div { class: "export-section",
                            span { class: "export-section-label", "Скачать отчёт" }
                            div { class: "export-cards-row",
                                a {
                                    class: "export-card",
                                    href: "{excel_url}",
                                    download: "evaluation_{evaluation_id}.xlsx",
                                    div { class: "export-card-icon", "📊" }
                                    span { class: "export-card-label", "Excel" }
                                    span { class: "export-card-sub", "Таблица с\nответами" }
                                }
                                a {
                                    class: "export-card",
                                    href: "{pdf_url}",
                                    download: "evaluation_{evaluation_id}.pdf",
                                    div { class: "export-card-icon", "📄" }
                                    span { class: "export-card-label", "PDF" }
                                    span { class: "export-card-sub", "Готовый\nотчёт" }
                                }
                            }
                        }
                    }
                }

                // ── Actions ────────────────────────────
                div { style: "padding:12px 18px 0; display:flex; flex-direction:column; gap:9px;",
                    div { style: "display:flex; gap:10px;",
                        button {
                            class: "btn-ghost",
                            style: "flex:1; font-size:13px;",
                            onclick: move |_| on_new.call(()),
                            "Ещё замер"
                        }
                        button {
                            class: "btn-primary",
                            style: "flex:2; font-size:13px;",
                            onclick: move |_| on_done.call(()),
                            "К списку замеров"
                        }
                    }
                }
            }
        }
    }
}
