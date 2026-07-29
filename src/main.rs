//! Restos Web App — standalone, no Telegram/token dependency.
//!
//! Auth state is stored in localStorage. If not authenticated, shows login/register.
//! Navigation is signal-based (no URL routing).

use std::collections::HashSet;

use dioxus::prelude::*;

mod account_api;
mod account_session;
mod assessment_api;
mod api;
mod auth;
mod clipboard_safe;
mod components;
mod device_identity;
mod storage;
mod types;
mod user_error;
mod retained;

use account_api::AccountApiClient;
use account_session::AccountSessionAdapter;
use assessment_api::AssessmentApiClient;
use auth::AuthState;
use components::{
    AccountPage, AiAssistantPage, AnalyticsPage, AssessmentsPage, AuthPage, EmployeesPage,
    EvaluationForm, HomePage, InternshipsPage, PinStepUpScreen,
};
use components::nav_bar::{BrandMark, NavBar};
use components::{ErrorView, SessionGateSkeleton};
use crate::types::MeResponse;

const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");

fn main() {
    install_runtime_error_guards();
    dioxus::launch(App);
}

fn install_runtime_error_guards() {
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(
            window.as_ref(),
            &wasm_bindgen::JsValue::from_str("__RESTOS_FATAL__"),
            &wasm_bindgen::JsValue::FALSE,
        );
    }
}

fn is_tab_visible(tab: &str, current: &str, visited: &HashSet<String>) -> bool {
    current == tab || visited.contains(tab)
}

fn layer_style(tab: &str, current: &str) -> &'static str {
    if current == tab {
        "display:block;"
    } else {
        "display:none;"
    }
}

fn layer_class(tab: &str, current: &str) -> &'static str {
    if current == tab {
        "page-layer page-layer--active page-transition"
    } else {
        "page-layer"
    }
}

fn mark_visited(visited: &mut HashSet<String>, current: &str) {
    visited.insert(current.to_string());
}

fn account_api_base() -> String {
    if let Some(window) = web_sys::window() {
        if let Ok(origin) = window.location().origin() {
            if origin.contains(":8080") {
                return origin.replace(":8080", ":8000");
            }
            return origin;
        }
    }
    "http://localhost:8000".to_string()
}

#[component]
fn App() -> Element {
    // Stage 20B only provides the isolated Account session boundary. Automatic refresh is
    // intentionally deferred until the Account UI owns an explicit startup policy.
    use_context_provider(|| {
        AccountSessionAdapter::new(
            AccountApiClient::new(account_api_base()).expect("Account API base must be valid"),
        )
    });
    use_context_provider(|| {
        AssessmentApiClient::new(account_api_base())
            .expect("Assessment API base must be valid")
    });
    let mut auth = use_signal(|| AuthState::load());

    use_effect(move || {
        let script = r#"
if (!window.__restosErrorGuardsInstalled) {
  window.__restosErrorGuardsInstalled = true;
  const markFatal = function(message) {
    try {
      if (message) { console.error('[Restos]', message); }
      window.__RESTOS_FATAL__ = true;
      window.__RESTOS_FATAL_MESSAGE__ = 'При работе приложения произошла ошибка. Обновите страницу или попробуйте позже.';
      if (!document.getElementById('restos-fatal-overlay')) {
        const overlay = document.createElement('div');
        overlay.id = 'restos-fatal-overlay';
        overlay.style.cssText = 'position:fixed;inset:0;z-index:99999;display:flex;align-items:center;justify-content:center;padding:24px;background:#0e0f11;color:#f3f1ee;';
        overlay.innerHTML = '<div style="width:min(100%,420px);padding:22px;border-radius:20px;border:1px solid rgba(255,255,255,0.08);background:linear-gradient(180deg,rgba(255,255,255,0.05),rgba(255,255,255,0.03));box-shadow:0 18px 40px rgba(0,0,0,0.24);font-family:system-ui,sans-serif;"><div style="font-size:20px;font-weight:600;margin-bottom:12px;">Не удалось отобразить приложение</div><div style="font-size:14px;line-height:1.5;color:#c8c4be;margin-bottom:10px;" id="restos-fatal-message"></div><div style="font-size:14px;line-height:1.5;color:#9d9992;margin-bottom:16px;">Попробуйте перезагрузить страницу. Если ошибка повторяется, это окно покажется вместо белого экрана.</div><button style="width:100%;height:44px;border-radius:12px;border:none;background:#f5a623;color:#161719;font-weight:600;cursor:pointer;" onclick="window.location.reload()">Перезагрузить</button></div>';
        document.body.appendChild(overlay);
      }
      const msg = document.getElementById('restos-fatal-message');
      if (msg) {
        msg.textContent = window.__RESTOS_FATAL_MESSAGE__;
      }
      window.dispatchEvent(new CustomEvent('restos-fatal'));
    } catch (_) {}
  };
  window.addEventListener('error', function(event) {
    markFatal(event && (event.message || (event.error && event.error.message)));
  });
  window.addEventListener('unhandledrejection', function(event) {
    const reason = event && event.reason;
    markFatal((reason && (reason.message || reason.toString && reason.toString())) || 'unhandledrejection');
  });
}
"#;

        if let Some(document) = web_sys::window().and_then(|w| w.document()) {
            if let Ok(script_el) = document.create_element("script") {
                script_el.set_inner_html(script);
                if let Some(head) = document.head() {
                    let _ = head.append_child(&script_el);
                }
            }
        }
    });

    let fatal_error = use_memo(move || {
        web_sys::window().and_then(|window| {
            let is_fatal = js_sys::Reflect::get(
                window.as_ref(),
                &wasm_bindgen::JsValue::from_str("__RESTOS_FATAL__"),
            ).ok()?.as_bool().unwrap_or(false);
            if !is_fatal {
                return None;
            }
            js_sys::Reflect::get(
                window.as_ref(),
                &wasm_bindgen::JsValue::from_str("__RESTOS_FATAL_MESSAGE__"),
            ).ok()?.as_string().or_else(|| Some("Произошла критическая ошибка интерфейса".to_string()))
        })
    });

    rsx! {
        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: MAIN_CSS }
        document::Meta { name: "viewport", content: "width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover" }
        document::Meta { name: "theme-color", content: "#f8f5f1" }
        document::Meta { name: "color-scheme", content: "light" }
        // PWA / installability
        document::Meta { name: "application-name", content: "RestOS" }
        document::Meta { name: "mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-capable", content: "yes" }
        document::Meta { name: "apple-mobile-web-app-status-bar-style", content: "black-translucent" }
        document::Meta { name: "apple-mobile-web-app-title", content: "RestOS" }
        document::Link { rel: "manifest", href: "/manifest.json" }
        document::Link { rel: "apple-touch-icon", href: "/icons/apple-touch-icon.png" }
        document::Link { rel: "icon", r#type: "image/svg+xml", href: "/favicon.svg" }
        // Service worker registration
        document::Script {
            r#type: "text/javascript",
            dangerous_inner_html: "
if ('serviceWorker' in navigator) {{
  window.addEventListener('load', function() {{
    navigator.serviceWorker.register('/sw.js', {{ scope: '/' }})
      .then(function(reg) {{ console.log('[PWA] SW registered, scope:', reg.scope); }})
      .catch(function(err) {{ console.warn('[PWA] SW registration failed:', err); }});
  }});
}}
"
        }
        document::Title { "RestOS" }
        if let Some(message) = fatal_error() {
            div { class: "fatal-screen",
                div { class: "fatal-card",
                    div { class: "fatal-title", "Не удалось отобразить приложение" }
                    div { class: "fatal-text", "{user_error::for_ui_message(&message)}" }
                    div { class: "fatal-text", "Попробуйте перезагрузить страницу. Если ошибка повторяется, приложение покажет это окно вместо белого экрана." }
                    button {
                        class: "btn-primary w-full",
                        onclick: move |_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window.location().reload();
                            }
                        },
                        "Перезагрузить"
                    }
                }
            }
        } else {
            match auth.read().clone() {
                None => rsx! {
                    AuthPage {
                        on_auth: move |state: AuthState| auth.set(Some(state)),
                    }
                },
                Some(_) => rsx! {
                    MainApp {
                        auth,
                        on_logout: move |_| {
                            AuthState::clear();
                            auth.set(None);
                        },
                        on_switch_auth: move |new_state: AuthState| {
                            new_state.save();
                            auth.set(Some(new_state));
                        },
                    }
                }
            }
        }
    }
}

/// Sidebar + stacked tabs: once a tab has been opened it stays mounted (hidden) so data and
/// `use_resource` state are preserved when switching away.
#[component]
fn MainShell(
    current: String,
    visited: Signal<HashSet<String>>,
    token: String,
    can_use_evaluations: bool,
    is_employee_role: bool,
    active_eval_id: Signal<Option<i64>>,
    page: Signal<String>,
    on_logout: EventHandler<()>,
    on_switch_auth: EventHandler<AuthState>,
    on_nav_bar: EventHandler<String>,
) -> Element {
    let vis = visited();
    let mut drawer_open = use_signal(|| false);

    use_effect(move || {
        let overflow = if drawer_open() { "hidden" } else { "" };
        let _ = js_sys::eval(&format!(
            "if (document.body) document.body.style.overflow = '{}';",
            overflow
        ));
    });

    rsx! {
        div {
            class: if current != "form" { "app-root app-shell" } else { "app-root" },
            onkeydown: move |event| {
                if event.key() == Key::Escape && drawer_open() {
                    drawer_open.set(false);
                }
            },
            if current != "form" {
                NavBar {
                    active: current.clone(),
                    show_evaluations: can_use_evaluations,
                    is_employee_role,
                    open: drawer_open(),
                    on_close: move |_| drawer_open.set(false),
                    on_logout: move |_| {
                        drawer_open.set(false);
                        let _ = js_sys::eval("if (document.body) document.body.style.overflow = '';");
                        on_logout.call(());
                    },
                    on_navigate: move |p: String| {
                        drawer_open.set(false);
                        on_nav_bar.call(p);
                    },
                }
                div { class: "mobile-app-bar",
                    button {
                        class: "mobile-menu-button",
                        r#type: "button",
                        aria_label: "Открыть меню",
                        aria_expanded: if drawer_open() { "true" } else { "false" },
                        onclick: move |_| drawer_open.set(true),
                        span { aria_hidden: "true", "☰" }
                    }
                    div { class: "mobile-brand", aria_label: "RestOS", BrandMark {} }
                }
            }
            div { class: if current != "form" { "page-stack app-shell-content" } else { "page-stack" },
                // Recreate the dashboard when returning to it so newly completed measurements
                // are included immediately instead of showing a retained resource snapshot.
                if current == "home" {
                    div { class: "{layer_class(\"home\", &current)}", style: "{layer_style(\"home\", &current)}",
                        HomePage {
                            token: token.clone(),
                            can_use_evaluations,
                            on_navigate: move |p: String| {
                                if p == "logout" {
                                    on_logout.call(());
                                    return;
                                }
                                if !can_use_evaluations && (p == "form" || p == "evaluations") {
                                    page.set("analytics".to_string());
                                    return;
                                }
                                if p == "form" {
                                    active_eval_id.set(None);
                                }
                                page.set(p);
                            },
                            on_switch_auth,
                        }
                    }
                }
                if can_use_evaluations && is_tab_visible("evaluations", &current, &vis) {
                    div { class: "{layer_class(\"evaluations\", &current)}", style: "{layer_style(\"evaluations\", &current)}",
                        AssessmentsPage {}
                    }
                }
                if is_tab_visible("analytics", &current, &vis) {
                    div { class: "{layer_class(\"analytics\", &current)}", style: "{layer_style(\"analytics\", &current)}",
                        AnalyticsPage { token: token.clone() }
                    }
                }
                if is_tab_visible("employees", &current, &vis) {
                    div { class: "{layer_class(\"employees\", &current)}", style: "{layer_style(\"employees\", &current)}",
                        EmployeesPage { token: token.clone(), is_employee_role }
                    }
                }
                if is_tab_visible("internships", &current, &vis) {
                    div { class: "{layer_class(\"internships\", &current)}", style: "{layer_style(\"internships\", &current)}",
                        InternshipsPage {}
                    }
                }
                if is_tab_visible("ai-assistant", &current, &vis) {
                    div { class: "{layer_class(\"ai-assistant\", &current)}", style: "{layer_style(\"ai-assistant\", &current)}",
                        AiAssistantPage {
                            token: token.clone(),
                            on_back: move |_| page.set("home".to_string()),
                            on_created_set: move |_| {},
                        }
                    }
                }
                if is_tab_visible("profile", &current, &vis) {
                    div { class: "{layer_class(\"profile\", &current)}", style: "{layer_style(\"profile\", &current)}",
                        AccountPage {
                            token: token.clone(),
                            on_logout: move |_| on_logout.call(()),
                            on_auth_update: move |s| on_switch_auth.call(s),
                        }
                    }
                }
                if can_use_evaluations && is_tab_visible("form", &current, &vis) {
                    div { class: "{layer_class(\"form\", &current)}", style: "{layer_style(\"form\", &current)}",
                        EvaluationForm {
                            token: token.clone(),
                            resume_evaluation_id: active_eval_id(),
                            on_done: move |_| {
                                active_eval_id.set(None);
                                page.set("evaluations".to_string());
                            },
                            on_back: move |_| {
                                active_eval_id.set(None);
                                page.set("evaluations".to_string());
                            },
                        }
                    }
                }
            }
        }
    }
}

// В MainApp — заменяем reset_scope_if_changed на scope_key подход

#[component]
fn MainApp(
    auth: Signal<Option<AuthState>>,
    on_logout: EventHandler<()>,
    on_switch_auth: EventHandler<AuthState>,
) -> Element {
    let mut page = use_signal(|| "home".to_string());
    let mut active_eval_id: Signal<Option<i64>> = use_signal(|| None);

    let mut navigate = move |p: String| {
        if p == "logout" {
            on_logout.call(());
        } else {
            page.set(p);
        }
    };

    let mut visited = use_signal(HashSet::<String>::new);
    use_effect(move || {
        visited.with_mut(|s| {
            mark_visited(s, &page());
        });
    });

    // scope_key — версионированный ключ, меняется при смене org.
    // При его смене Dioxus полностью размонтирует и пересоздаёт MainShell
    // со всеми дочерними use_resource.
    let scope_key = use_memo(move || {
        auth()
            .as_ref()
            .map(|s| format!("org:{}", s.org_id))
            .unwrap_or_else(|| "org:none".to_string())
    });

    // Сбрасываем page + visited при смене org
    let mut prev_scope = use_signal(|| scope_key());
    use_effect(move || {
        let next = scope_key();
        if prev_scope() != next {
            prev_scope.set(next);
            visited.set(HashSet::new());
            page.set("home".to_string());
        }
    });

    let auth_token = move || {
        auth()
            .as_ref()
            .map(|s| s.access_token.clone())
            .unwrap_or_default()
    };

    // Read `auth()` inside the closure so Dioxus re-fetches /me when the JWT changes
    // (e.g. after verify-pin or set-pin returns a new access_token).
    // After verify-pin the JWT in localStorage is already valid (reload proves it), but
    // `use_resource` may still hold the previous `Ok(me)` with pin_required_now=true until
    // the next /me fetch finishes. This flag lets us leave the PIN screen immediately.
    let mut pin_unlocked = use_signal(|| false);

    let mut profile_gate = use_resource(move || {
        let tok = auth_token();
        async move { crate::api::fetch_me(&tok).await }
    });

    let mut gate_stale = use_signal(|| None::<MeResponse>);
    crate::retained::cache_ok_resource(move || profile_gate(), gate_stale);

    use_effect(move || {
        if let Some(Ok(m)) = profile_gate() {
            if !m.pin_required_now {
                pin_unlocked.set(true);
            }
        }
    });

    let on_pin_verified = {
        move |new_state: AuthState| {
            new_state.save();
            pin_unlocked.set(true);
            gate_stale.set(None);
            on_switch_auth.call(new_state);
            profile_gate.restart();
        }
    };

    let token = auth_token();
    let current = page.read().clone();
    let key = scope_key();

    let me_for_shell = profile_gate()
        .and_then(|r| r.ok())
        .or_else(|| gate_stale());

    let pin_required = !pin_unlocked()
        && me_for_shell
            .as_ref()
            .map(|m| m.pin_required_now)
            .unwrap_or(true);

    let gate_body = if let Some(Err(e)) = profile_gate() {
        rsx! {
            ErrorView { message: e }
        }
    } else if pin_required {
        rsx! {
            PinStepUpScreen {
                token: auth_token(),
                on_success: on_pin_verified,
            }
        }
    } else if let Some(m) = me_for_shell {
        rsx! {
            div { key: "{key}",
                MainShell {
                    current: current.clone(),
                    visited,
                    token: token.clone(),
                    can_use_evaluations: !m.is_employee_role,
                    is_employee_role: m.is_employee_role,
                    active_eval_id,
                    page,
                    on_logout,
                    on_switch_auth,
                    on_nav_bar: move |p: String| navigate(p),
                }
            }
        }
    } else {
        rsx! {
            SessionGateSkeleton {}
        }
    };

    rsx! {
        {gate_body}
    }
}
