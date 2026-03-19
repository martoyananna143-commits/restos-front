//! Yarbot Web App — standalone, no Telegram/token dependency.
//!
//! Auth state is stored in localStorage. If not authenticated, shows login/register.
//! Navigation is signal-based (no URL routing).

use dioxus::prelude::*;

mod api;
mod auth;
mod components;
mod storage;
mod telegram;
mod types;

use auth::AuthState;
use components::{
    AnalyticsPage, AuthPage, EmployeesPage, EvaluationForm, EvaluationsSection, HomePage,
};
use components::nav_bar::NavBar;

const MAIN_CSS: Asset = asset!("/assets/styling/main.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut auth = use_signal(|| AuthState::load());

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        match auth.read().clone() {
            None => rsx! {
                AuthPage {
                    on_auth: move |state: AuthState| auth.set(Some(state)),
                }
            },
            Some(state) => rsx! {
                MainApp { auth_state: state, on_logout: move |_| {
                    AuthState::clear();
                    auth.set(None);
                }}
            },
        }
    }
}

#[component]
fn MainApp(auth_state: AuthState, on_logout: EventHandler<()>) -> Element {
    let mut page = use_signal(|| "home".to_string());
    // Stores the evaluation_id after starting a new evaluation
    let mut active_eval_id: Signal<Option<i64>> = use_signal(|| None);

    let mut navigate = move |p: String| {
        if p == "logout" {
            on_logout.call(());
        } else {
            page.set(p);
        }
    };

    let token = auth_state.access_token.clone();
    let current = page.read().clone();

    rsx! {
        div { class: "app-root",
            match current.as_str() {
                "home" => rsx! {
                    HomePage {
                        token: token.clone(),
                        on_navigate: move |p: String| page.set(p),
                    }
                },
                "evaluations" => rsx! {
                    EvaluationsSection {
                        token: token.clone(),
                        on_start_eval: move |eval_id: i64| {
                            active_eval_id.set(Some(eval_id));
                            page.set("form".to_string());
                        },
                    }
                },
                "analytics" => rsx! {
                    AnalyticsPage { token: token.clone() }
                },
                "employees" => rsx! {
                    EmployeesPage { token: token.clone() }
                },
                "form" => rsx! {
                    EvaluationForm {
                        token: token.clone(),
                        on_done: move |_| page.set("evaluations".to_string()),
                        on_back: move |_| page.set("evaluations".to_string()),
                    }
                },
                _ => rsx! {
                    HomePage {
                        token: token.clone(),
                        on_navigate: move |p: String| page.set(p),
                    }
                },
            }

            // Bottom nav — not shown during form filling
            if current != "form" {
                NavBar {
                    active: current.clone(),
                    on_navigate: move |p: String| navigate(p),
                }
            }
        }
    }
}
