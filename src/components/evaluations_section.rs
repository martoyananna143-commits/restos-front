//! Evaluations section with config subpages.

use dioxus::prelude::*;

use crate::components::{
    AiAssistantPage, CriteriaPage, CriterionSetsPage, EvaluationTypesPage, EvaluationsPage,
};

#[derive(Clone, PartialEq)]
enum Screen {
    Overview,
    Criteria,
    CriterionSets,
    EvaluationTypes,
    AiAssistant,
}

#[component]
pub fn EvaluationsSection(token: String, on_start_eval: EventHandler<i64>) -> Element {
    let mut screen = use_signal(|| Screen::Overview);

    match screen() {
        Screen::Overview => rsx! {
            EvaluationsPage {
                token,
                on_start_eval,
                on_open_criteria: move |_| screen.set(Screen::Criteria),
                on_open_sets: move |_| screen.set(Screen::CriterionSets),
                on_open_types: move |_| screen.set(Screen::EvaluationTypes),
                on_open_ai: move |_| screen.set(Screen::AiAssistant),
            }
        },
        Screen::Criteria => rsx! {
            CriteriaPage {
                token,
                on_back: move |_| screen.set(Screen::Overview),
            }
        },
        Screen::CriterionSets => rsx! {
            CriterionSetsPage {
                token,
                on_back: move |_| screen.set(Screen::Overview),
                on_open_ai: move |_| screen.set(Screen::AiAssistant),
            }
        },
        Screen::EvaluationTypes => rsx! {
            EvaluationTypesPage {
                token,
                on_back: move |_| screen.set(Screen::Overview),
            }
        },
        Screen::AiAssistant => rsx! {
            AiAssistantPage {
                token,
                on_back: move |_| screen.set(Screen::Overview),
                on_created_set: move |_| screen.set(Screen::CriterionSets),
            }
        },
    }
}
