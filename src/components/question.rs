//! Single question card — styled for step-by-step evaluation flow.

use super::inputs::{BooleanInput, CommentInput, NumberInput, TextInput};
use crate::types::{AnswerValue, Criterion};
use dioxus::prelude::*;

/// Per-question answer state — kept in the parent's Vec<QuestionState>.
#[derive(Clone, PartialEq)]
pub struct QuestionState {
    pub criterion_id: i64,
    pub value: Signal<Option<AnswerValue>>,
    pub comment: Signal<Option<String>>,
}

impl QuestionState {
    pub fn new(criterion_id: i64) -> Self {
        Self {
            criterion_id,
            value: Signal::new(None),
            comment: Signal::new(None),
        }
    }

    pub fn with_values(
        criterion_id: i64,
        value: Option<AnswerValue>,
        comment: Option<String>,
    ) -> Self {
        Self {
            criterion_id,
            value: Signal::new(value),
            comment: Signal::new(comment),
        }
    }

    #[inline]
    pub fn is_answered(&self) -> bool {
        self.value.read().is_some()
    }
}

/// Full-screen question card shown in the step-by-step flow.
#[component]
pub fn QuestionCard(
    criterion: Criterion,
    index: usize,
    total: usize,
    state: QuestionState,
    disabled: bool,
) -> Element {
    let is_boolean = criterion.value_type == "boolean";
    let is_number = criterion.value_type == "number";

    let initial = (state.value)();
    let init_bool = initial.clone();
    let init_num = initial.clone();
    let init_str = initial;

    let bool_val: Signal<Option<bool>> = use_signal(move || match &init_bool {
        Some(AnswerValue::Boolean(b)) => Some(*b),
        _ => None,
    });
    let num_val: Signal<Option<f64>> = use_signal(move || match &init_num {
        Some(AnswerValue::Number(n)) => Some(*n),
        _ => None,
    });
    let str_val: Signal<Option<String>> = use_signal(move || match &init_str {
        Some(AnswerValue::Text(s)) => Some(s.clone()),
        _ => None,
    });

    // Sync local sub-signals back to the parent state signal.
    use_effect(move || {
        let val = if is_boolean {
            bool_val().map(AnswerValue::Boolean)
        } else if is_number {
            num_val().map(AnswerValue::Number)
        } else {
            str_val().map(|s| AnswerValue::Text(s))
        };
        state.value.set(val);
    });

    let type_badge = if is_boolean {
        "Да / Нет"
    } else if is_number {
        "Числовая"
    } else {
        "Текст"
    };

    rsx! {
        div { class: "pad", style: "margin-top:16px;",
            div { class: "card card-amber", style: "padding:20px;",
                // Type badge
                span { class: "badge badge-amber", style: "margin-bottom:12px; display:inline-flex;",
                    "{type_badge}"
                }

                // Criterion name — serif large
                div { class: "heading-xl",
                    "{criterion.name}"
                }

                // Description
                if let Some(ref desc) = criterion.description {
                    p { class: "body-text", style: "margin-top:10px;", "{desc}" }
                }

                // Answer input
                if is_boolean {
                    BooleanInput { value: bool_val, disabled }
                } else if is_number {
                    NumberInput {
                        value: num_val,
                        placeholder: "1–5".to_string(),
                        disabled,
                    }
                } else {
                    div { style: "margin-top:16px;",
                        TextInput {
                            value: str_val,
                            placeholder: "Введите ответ...".to_string(),
                            disabled,
                        }
                    }
                }

                // Comment
                CommentInput { value: state.comment, disabled }
            }
        }
    }
}
