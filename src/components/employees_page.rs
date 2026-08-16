//! Employee list page + invitation flow

use dioxus::prelude::*;
use qrcodegen::{QrCode, QrCodeEcc};
use wasm_bindgen_futures::spawn_local;

use super::shared::{ErrorView, LoadingView};
use crate::api;
use crate::auth::AuthState;
use crate::clipboard_safe;
use crate::types::{CreateInvitationRequest, Employee, EmployeeTypeOption, Invitation, OrgInfo};

fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    match parts.as_slice() {
        [] => "?".to_string(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [a, b, ..] => format!(
            "{}{}",
            a.chars().next().unwrap_or('?'),
            b.chars().next().unwrap_or('?')
        )
        .to_uppercase(),
    }
}

fn av_color(name: &str) -> &'static str {
    match name.bytes().next().unwrap_or(0) % 4 {
        0 => "av-amber",
        1 => "av-blue",
        2 => "av-green",
        _ => "av-purple",
    }
}

fn role_badge_class(code: &str, is_admin: bool) -> &'static str {
    if is_admin || code == "admin" || code == "administrator" {
        "badge-amber"
    } else if code == "manager" {
        "badge-blue"
    } else {
        "badge-muted"
    }
}

fn qr_data_url(value: &str) -> Option<String> {
    let qr = QrCode::encode_text(value, QrCodeEcc::Medium).ok()?;
    let border = 4;
    let size = qr.size();
    let view_box = size + border * 2;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {view_box} {view_box}\" shape-rendering=\"crispEdges\">\
<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\
<path d=\""
    );

    for y in 0..size {
        for x in 0..size {
            if qr.get_module(x, y) {
                let px = x + border;
                let py = y + border;
                svg.push_str(&format!("M{px},{py}h1v1h-1z"));
            }
        }
    }

    svg.push_str("\" fill=\"black\"/></svg>");
    Some(format!(
        "data:image/svg+xml;utf8,{}",
        urlencoding::encode(&svg)
    ))
}

fn spawn_copy_invite_link(
    text: String,
    mut copy_ok: Signal<bool>,
    mut clipboard_err: Signal<Option<String>>,
) {
    spawn_local(async move {
        clipboard_err.set(None);
        match clipboard_safe::copy_text_async(&text).await {
            Ok(()) => copy_ok.set(true),
            Err(msg) => {
                copy_ok.set(false);
                clipboard_err.set(Some(msg));
            }
        }
    });
}

fn confirm_dismiss(full_name: &str) -> bool {
    let msg = format!(
        "Уволить сотрудника \"{full_name}\"? Действие можно отменить только через восстановление."
    );
    web_sys::window()
        .and_then(|w| w.confirm_with_message(&msg).ok())
        .unwrap_or(false)
}

#[component]
pub fn EmployeesPage(token: String, is_employee_role: bool) -> Element {
    let can_manage = !is_employee_role && AuthState::load().map(|a| a.is_admin).unwrap_or(false);

    let mut search = use_signal(String::new);
    let mut filter = use_signal(|| "all".to_string());
    let mut editing_id: Signal<Option<i64>> = use_signal(|| None);
    let mut employees: Signal<Vec<Employee>> = use_signal(Vec::new);
    let mut roles: Signal<Vec<EmployeeTypeOption>> = use_signal(Vec::new);
    let mut invitations: Signal<Vec<Invitation>> = use_signal(Vec::new);
    let mut load_error: Signal<Option<String>> = use_signal(|| None);
    let mut can_dismiss: Signal<bool> = use_signal(|| false);
    let mut my_employee_id: Signal<Option<i64>> = use_signal(|| None);

    // Invite wizard state
    let mut invite_open = use_signal(|| false);
    let mut invite_step = use_signal(|| 1_i32);
    let mut invite_name = use_signal(String::new);
    let mut invite_email = use_signal(String::new);
    let mut invite_tg = use_signal(String::new);
    let mut invite_position = use_signal(String::new);
    let mut invite_role_id: Signal<Option<i64>> = use_signal(|| None);
    let mut invite_ttl_days = use_signal(|| 7_i64);
    let mut invite_error: Signal<Option<String>> = use_signal(|| None);
    let mut invite_loading = use_signal(|| false);
    let mut invite_success: Signal<Option<Invitation>> = use_signal(|| None);
    let mut invite_preview: Signal<Option<Invitation>> = use_signal(|| None);

    // Standalone invite state
    let mut standalone_open = use_signal(|| false);
    let mut standalone_ttl_days = use_signal(|| 30_i64);
    let mut standalone_loading = use_signal(|| false);
    let mut standalone_error: Signal<Option<String>> = use_signal(|| None);
    let mut standalone_result: Signal<Option<(String, String)>> = use_signal(|| None); // (code, url)
    let mut standalone_copy_ok = use_signal(|| false);

    let mut invite_copy_ok = use_signal(|| false);
    let mut preview_copy_ok = use_signal(|| false);
    let mut clipboard_err: Signal<Option<String>> = use_signal(|| None);

    // Create org state
    let mut create_org_open = use_signal(|| false);
    let mut new_org_name = use_signal(String::new);
    let mut create_org_loading = use_signal(|| false);
    let mut create_org_error: Signal<Option<String>> = use_signal(|| None);
    let mut create_org_success: Signal<Option<String>> = use_signal(|| None);

    let t1 = token.clone();
    let initial = use_resource(move || {
        let tok = t1.clone();
        async move {
            let emps = api::fetch_employees(&tok).await;
            let rls = if is_employee_role {
                Ok(Vec::new())
            } else {
                api::fetch_employee_types(&tok).await
            };
            let me = api::fetch_me(&tok).await;
            let inv = if can_manage {
                api::fetch_invitations(&tok).await
            } else {
                Ok(Vec::new())
            };
            (emps, rls, me, inv)
        }
    });

    use_effect(move || {
        if let Some((emps_result, roles_result, me_result, inv_result)) = initial() {
            match emps_result {
                Ok(emps) => employees.set(emps),
                Err(e) => load_error.set(Some(e)),
            }
            if let Ok(rls) = roles_result {
                roles.set(rls);
            }
            if let Ok(me) = me_result {
                can_dismiss.set(me.is_org_creator);
                my_employee_id.set(Some(me.employee_id));
            }
            if let Ok(inv) = inv_result {
                invitations.set(inv);
            }
        }
    });

    if let Some(e) = load_error() {
        return rsx! { ErrorView { message: e } };
    }
    if initial().is_none() {
        return rsx! { LoadingView { message: if is_employee_role { "Загрузка профиля...".to_string() } else { "Загрузка команды...".to_string() } } };
    }

    let emps = employees.read();
    let search_val = search.read().to_lowercase();
    let filter_val = filter.read().clone();
    let role_list = roles.read();
    let invites = invitations.read();

    let pending_count = invites.iter().filter(|i| !i.used).count();
    let admin_count = emps.iter().filter(|e| e.is_admin).count();
    let active_count = emps.iter().filter(|e| e.is_active).count();

    let filtered: Vec<&Employee> = emps
        .iter()
        .filter(|e| {
            let name_match =
                search_val.is_empty() || e.full_name.to_lowercase().contains(&search_val);
            let role_match = match filter_val.as_str() {
                "admin" => e.is_admin,
                "employee" => !e.is_admin,
                _ => true,
            };
            name_match && role_match
        })
        .collect();

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "page-header page-header-spacious",
                    div {
                        div { class: "label-text", if is_employee_role { "Сотрудник" } else { "Организация" } }
                        div { class: "page-title page-title-lg", if is_employee_role { "Мой профиль" } else { "Команда" } }
                        if !is_employee_role {
                            div { class: "page-subtitle", "{emps.len()} сотрудников" }
                        }
                    }
                    if can_manage {
                        div { class: "header-actions", style: "display:flex; gap:6px;",
                            button {
                                class: "icon-btn",
                                title: "Создать организацию",
                                style: "font-size:14px;",
                                onclick: move |_| {
                                    create_org_open.set(!create_org_open());
                                    standalone_open.set(false);
                                    invite_open.set(false);
                                    create_org_error.set(None);
                                    create_org_success.set(None);
                                },
                                "🏢"
                            }
                            button {
                                class: "icon-btn",
                                title: "Пригласить для регистрации (новая орг.)",
                                style: "font-size:14px;",
                                onclick: move |_| {
                                    standalone_open.set(!standalone_open());
                                    create_org_open.set(false);
                                    invite_open.set(false);
                                    standalone_error.set(None);
                                    standalone_result.set(None);
                                },
                                "🔗"
                            }
                            button {
                                class: "icon-btn icon-btn-solid",
                                title: "Пригласить сотрудника",
                                onclick: move |_| {
                                    invite_open.set(!invite_open());
                                    create_org_open.set(false);
                                    standalone_open.set(false);
                                    invite_error.set(None);
                                    if invite_open() {
                                        invite_success.set(None);
                                    }
                                },
                                if invite_open() { "✕" } else { "+" }
                            }
                        }
                    }
                }

                if let Some(ref msg) = clipboard_err() {
                    div { class: "error-msg", role: "alert", style: "margin: 12px 0 0;",
                        div { style: "display:flex; justify-content: space-between; align-items: flex-start; gap: 10px;",
                            div {
                                div { class: "label-text", "Не удалось скопировать" }
                                div { class: "body-text", style: "margin-top: 6px; word-break: break-word;", "{msg}" }
                            }
                            button {
                                class: "btn-ghost btn-icon-sm",
                                r#type: "button",
                                onclick: move |_| clipboard_err.set(None),
                                "✕"
                            }
                        }
                    }
                }

                if !is_employee_role {
                    // Stats strip
                    div { class: "stats-row", style: "margin-top: 10px;",
                        div { class: "stat-card",
                            span { class: "label-text", "Активных" }
                            div { class: "stat-num", "{active_count}" }
                        }
                        div { class: "stat-card",
                            span { class: "label-text", "Ожидают" }
                            div { class: "stat-num", style: "color: var(--amber);", "{pending_count}" }
                        }
                        div { class: "stat-card",
                            span { class: "label-text", "Админов" }
                            div { class: "stat-num", "{admin_count}" }
                        }
                    }
                }

                // ── Standalone invite panel ───────────────────────────────
                if can_manage && standalone_open() {
                    {
                        let tok = token.clone();
                        rsx! {
                            div { class: "card", style: "margin: 14px 0 0;",
                                div { class: "heading-md", style: "margin-bottom:12px;",
                                    "🔗 Пригласить для регистрации"
                                }
                                div { class: "body-text", style: "color:var(--text3); margin-bottom:12px;",
                                    "Создаёт ссылку-приглашение, по которой новый пользователь зарегистрируется и создаст свою организацию."
                                }

                                if let Some((code, url)) = standalone_result() {
                                    {
                                        let qr_src = qr_data_url(&url);
                                        rsx! {
                                            div { style: "display:flex; flex-direction:column; gap:10px;",
                                                div { class: "card card-green",
                                                    div { class: "heading-md", "Ссылка создана" }
                                                }
                                                div { class: "form-field",
                                                    label { class: "field-label", "Ссылка приглашения" }
                                                    div { style: "display:flex; gap:8px; align-items:center; flex-wrap:wrap;",
                                                        input { class: "field-input", readonly: true, value: "{url}", style: "flex:1 1 240px; min-width:0;" }
                                                        button {
                                                            class: if standalone_copy_ok() { "btn-secondary" } else { "btn-ghost" },
                                                            r#type: "button",
                                                            onclick: {
                                                                let invite_url = url.clone();
                                                                move |_| {
                                                                    spawn_copy_invite_link(
                                                                        invite_url.clone(),
                                                                        standalone_copy_ok,
                                                                        clipboard_err,
                                                                    );
                                                                }
                                                            },
                                                            if standalone_copy_ok() { "Скопировано" } else { "Скопировать" }
                                                        }
                                                    }
                                                }
                                                div {
                                                    class: "card",
                                                    style: "padding:18px; border-radius:18px; background:linear-gradient(180deg, rgba(255,255,255,0.05), rgba(255,255,255,0.03)); overflow:hidden;",
                                                    div {
                                                        style: "display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; flex-wrap:wrap;",
                                                        div {
                                                            div { class: "field-label", "QR-код приглашения" }
                                                            div { style: "font-size:12px; color:var(--text2);", "Откройте камерой телефона или мессенджером" }
                                                        }
                                                        div {
                                                            style: "padding:4px 10px; border-radius:999px; border:1px solid rgba(255,255,255,0.08); color:var(--text2); font-size:12px; max-width:100%; overflow-wrap:anywhere; word-break:break-word; text-align:left;",
                                                            "{code}"
                                                        }
                                                    }
                                                    div { style: "display:flex; justify-content:center;",
                                                        div { style: "padding:18px; border-radius:20px; background:#fff; box-shadow:0 12px 28px rgba(0,0,0,0.18);",
                                                            if let Some(src) = qr_src.clone() {
                                                                img {
                                                                    src: "{src}",
                                                                    alt: "QR",
                                                                    style: "display:block; width:220px; height:220px; border-radius:12px;",
                                                                }
                                                            } else {
                                                                div {
                                                                    style: "width:220px; height:220px; display:flex; align-items:center; justify-content:center; text-align:center; color:#666; font-size:13px;",
                                                                    "Не удалось создать QR-код"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                button {
                                                    class: "btn-ghost",
                                                    onclick: move |_| {
                                                        standalone_copy_ok.set(false);
                                                        standalone_result.set(None);
                                                    },
                                                    "Создать ещё"
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    div { style: "display:flex; flex-direction:column; gap:12px;",
                                        div { class: "form-field",
                                            label { class: "field-label", "Срок действия" }
                                            {
                                                let cur = standalone_ttl_days();
                                                rsx! {
                                                    div { style: "display:flex; gap:8px; flex-wrap:wrap;",
                                                        for (days, label) in [(7_i64,"7 дней"), (30,"30 дней"), (90,"90 дней")] {
                                                            {
                                                                let is_active = cur == days;
                                                                rsx! {
                                                                    button {
                                                                        style: if is_active {
                                                                            "padding:6px 16px; border-radius:100px; background:rgba(245,166,35,0.15); color:#f5a623; border:1px solid rgba(245,166,35,0.35); font-size:13px; cursor:pointer; font-weight:500; white-space:nowrap;"
                                                                        } else {
                                                                            "padding:6px 16px; border-radius:100px; background:rgba(255,255,255,0.06); color:var(--text2); border:1px solid rgba(255,255,255,0.09); font-size:13px; cursor:pointer; white-space:nowrap;"
                                                                        },
                                                                        onclick: move |_| standalone_ttl_days.set(days),
                                                                        "{label}"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        if let Some(err) = standalone_error() {
                                            div { class: "error-msg", role: "alert", "{err}" }
                                        }
                                        button {
                                            class: "btn-primary",
                                            disabled: standalone_loading(),
                                            onclick: move |_| {
                                                let t = tok.clone();
                                                let ttl = standalone_ttl_days() * 86400;
                                                spawn(async move {
                                                    standalone_loading.set(true);
                                                    standalone_error.set(None);
                                                    standalone_copy_ok.set(false);
                                                    match api::create_standalone_invite(&t, ttl).await {
                                                        Ok(v) => {
                                                            let code = v["code"].as_str().unwrap_or("").to_string();
                                                            let url = v["invite_url"].as_str().unwrap_or("").to_string();
                                                            let full_url = if url.starts_with('/') {
                                                                if let Some(window) = web_sys::window() {
                                                                    if let Ok(origin) = window.location().origin() {
                                                                        format!("{}{}", origin, url)
                                                                    } else { url }
                                                                } else { url }
                                                            } else { url };
                                                            standalone_result.set(Some((code, full_url)));
                                                        }
                                                        Err(e) => standalone_error.set(Some(e)),
                                                    }
                                                    standalone_loading.set(false);
                                                });
                                            },
                                            if standalone_loading() { "Создание..." } else { "Создать ссылку" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Create org panel ─────────────────────────────────────
                if can_manage && create_org_open() {
                    {
                        let tok = token.clone();
                        rsx! {
                            div { class: "card", style: "margin: 14px 0 0;",
                                div { class: "heading-md", style: "margin-bottom:12px;",
                                    "🏢 Создать организацию"
                                }
                                div { class: "body-text", style: "color:var(--text3); margin-bottom:12px;",
                                    "Создаёт новую организацию. Вы автоматически станете её администратором и сможете переключиться в неё."
                                }

                                if let Some(msg) = create_org_success() {
                                    div { class: "card card-green",
                                        div { class: "heading-md", "Организация создана" }
                                        div { class: "body-text", style: "margin-top:4px;", "{msg}" }
                                    }
                                } else {
                                    div { style: "display:flex; flex-direction:column; gap:12px;",
                                        div { class: "form-field",
                                            label { class: "field-label", "Название организации" }
                                            input {
                                                class: "field-input",
                                                placeholder: "Например: ООО Ромашка",
                                                value: "{new_org_name}",
                                                oninput: move |e| new_org_name.set(e.value()),
                                            }
                                        }
                                        if let Some(err) = create_org_error() {
                                            div { class: "error-msg", role: "alert", "{err}" }
                                        }
                                        button {
                                            class: "btn-primary",
                                            disabled: create_org_loading() || new_org_name().trim().is_empty(),
                                            onclick: move |_| {
                                                let t = tok.clone();
                                                let name = new_org_name().trim().to_string();
                                                if name.is_empty() { return; }
                                                spawn(async move {
                                                    create_org_loading.set(true);
                                                    create_org_error.set(None);
                                                    match api::create_org(&t, &name).await {
                                                        Ok(resp) => {
                                                            create_org_success.set(Some(format!(
                                                                "«{}» создана. Переключитесь в неё из меню организаций.",
                                                                name
                                                            )));
                                                            new_org_name.set(String::new());
                                                            // Update auth state with new token + available_orgs
                                                            if let Some(mut auth) = AuthState::load() {
                                                                auth.access_token = resp.access_token;
                                                                auth.org_id = resp.org_id;
                                                                auth.is_admin = resp.is_admin;
                                                                auth.is_superuser = resp.is_superuser;
                                                                auth.available_orgs = resp.available_orgs;
                                                                auth.save();
                                                            }
                                                        }
                                                        Err(e) => create_org_error.set(Some(e)),
                                                    }
                                                    create_org_loading.set(false);
                                                });
                                            },
                                            if create_org_loading() { "Создание..." } else { "Создать" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if can_manage && invite_open() {
                    InviteWizard {
                        token: token.clone(),
                        roles: role_list.clone(),
                        invitations: invitations,
                        invite_step,
                        invite_name,
                        invite_email,
                        invite_tg,
                        invite_position,
                        invite_role_id,
                        invite_ttl_days,
                        invite_error,
                        invite_loading,
                        invite_success,
                        invite_copy_ok,
                        clipboard_err,
                    }
                }

                if can_manage && !invites.is_empty() {
                    div { class: "pad", style: "margin-top: 14px;",
                        div { class: "section-header",
                            span { class: "label-text", "Ожидают регистрации · {pending_count}" }
                        }
                        div { class: "card", style: "padding: 0; overflow: hidden;",
                            for inv in invites.iter().take(8) {
                                {
                                    let code = inv.code.clone();
                                    let tok = token.clone();
                                    let mut invitations_sig = invitations;
                                    let mut preview_sig = invite_preview;
                                    let preview_item = inv.clone();
                                    let role_name = inv.employee_type_name.clone().unwrap_or_else(|| "Сотрудник".to_string());
                                    let contact = inv
                                        .contact_email
                                        .clone()
                                        .or(inv.contact_telegram.clone())
                                        .unwrap_or_else(|| "Без контакта".to_string());
                                    rsx! {
                                        div { class: "list-row", style: "padding: 12px 14px;",
                                            div { class: "av av-sm av-muted", "?" }
                                            div { style: "flex: 1; min-width: 0;",
                                                div { class: "rank-name", style: "font-size: 13px;", "{contact}" }
                                                div { class: "caption-text", "{role_name} {inv.position.clone().unwrap_or_default()}" }
                                            }
                                            span {
                                                class: if inv.used { "badge badge-muted" } else { "badge badge-amber" },
                                                if inv.used { "Использован" } else { "Ожидает" }
                                            }
                                            if !inv.used {
                                                button {
                                                    class: "btn-ghost btn-icon-sm",
                                                    title: "Посмотреть ссылку и QR",
                                                    onclick: move |_| {
                                                        preview_sig.set(Some(preview_item.clone()));
                                                    },
                                                    "🔗"
                                                }
                                                button {
                                                    class: "btn-ghost btn-icon-sm",
                                                    title: "Отменить приглашение",
                                                    onclick: move |_| {
                                                        let t = tok.clone();
                                                        let c = code.clone();
                                                        spawn(async move {
                                                            let _ = api::delete_invitation(&t, &c).await;
                                                            if let Ok(new_list) = api::fetch_invitations(&t).await {
                                                                invitations_sig.set(new_list);
                                                                if let Some(p) = preview_sig() {
                                                                    if p.code == c {
                                                                        preview_sig.set(None);
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    },
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(preview) = invite_preview() {
                            {
                                let qr_src = qr_data_url(&preview.invite_url);
                                rsx! {
                                    div { class: "card card-glass", style: "margin-top: 10px;",
                                div { class: "section-header",
                                    span { class: "heading-md", "Приглашение: {preview.contact_email.clone().or(preview.contact_telegram.clone()).unwrap_or_else(|| \"Сотрудник\".to_string())}" }
                                    button {
                                        class: "btn-ghost btn-icon-sm",
                                        onclick: move |_| invite_preview.set(None),
                                        "✕"
                                    }
                                }
                                div { class: "form-field",
                                    label { class: "field-label", "Ссылка приглашения" }
                                    div { style: "display:flex; gap:8px; align-items:center; flex-wrap:wrap;",
                                        input {
                                            class: "field-input",
                                            readonly: true,
                                            value: "{preview.invite_url}",
                                            style: "flex:1 1 240px; min-width:0;",
                                        }
                                        button {
                                            class: if preview_copy_ok() { "btn-secondary" } else { "btn-ghost" },
                                            r#type: "button",
                                            onclick: {
                                                let invite_url = preview.invite_url.clone();
                                                move |_| {
                                                    spawn_copy_invite_link(
                                                        invite_url.clone(),
                                                        preview_copy_ok,
                                                        clipboard_err,
                                                    );
                                                }
                                            },
                                            if preview_copy_ok() { "Скопировано" } else { "Скопировать" }
                                        }
                                    }
                                }
                                div {
                                    class: "card",
                                    style: "margin-top: 10px; padding:18px; border-radius:18px; background:linear-gradient(180deg, rgba(255,255,255,0.05), rgba(255,255,255,0.03)); overflow:hidden;",
                                    div {
                                        style: "display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:12px; flex-wrap:wrap;",
                                        div {
                                            div { class: "field-label", "QR-код приглашения" }
                                            div { style: "font-size:12px; color:var(--text2);", "Сканируйте, чтобы быстро открыть ссылку" }
                                        }
                                        div {
                                            style: "padding:4px 10px; border-radius:999px; border:1px solid rgba(255,255,255,0.08); color:var(--text2); font-size:12px; max-width:100%; overflow-wrap:anywhere; word-break:break-word; text-align:left;",
                                            "{preview.code}"
                                        }
                                    }
                                    div { style: "display:flex; justify-content:center;",
                                        div { style: "padding:18px; border-radius:20px; background:#fff; box-shadow:0 12px 28px rgba(0,0,0,0.18);",
                                            if let Some(src) = qr_src {
                                                img {
                                                    src: "{src}",
                                                    alt: "QR code",
                                                    style: "display:block; width:220px; height:220px; border-radius:12px;",
                                                }
                                            } else {
                                                div {
                                                    style: "width:220px; height:220px; display:flex; align-items:center; justify-content:center; text-align:center; color:#666; font-size:13px;",
                                                    "Не удалось создать QR-код"
                                                }
                                            }
                                        }
                                    }
                                }
                                div { style: "display:flex; gap:8px; margin-top: 10px;",
                                    button {
                                        class: "btn-secondary",
                                        onclick: move |_| {
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.open_with_url(&preview.invite_url);
                                            }
                                        },
                                        "Открыть"
                                    }
                                    button {
                                        class: "btn-primary",
                                        onclick: move |_| {
                                            preview_copy_ok.set(false);
                                            invite_preview.set(None);
                                        },
                                        "Закрыть"
                                    }
                                }
                                    }
                                }
                            }
                        }
                    }
                }

                if !is_employee_role {
                    // Search bar
                    div { class: "search-bar", style: "margin-top: 10px;",
                        span { class: "search-icon", "🔍" }
                        input {
                            class: "search-input input",
                            r#type: "search",
                            placeholder: "Поиск по имени...",
                            value: "{search}",
                            oninput: move |e| search.set(e.value()),
                        }
                    }

                    // Filter chips
                    div { class: "filter-row filter-row-tight",
                        for (val, lbl) in [("all","Все"), ("admin","Администраторы"), ("employee","Сотрудники")] {
                            {
                                let v = val.to_string();
                                let active = filter_val == val;
                                rsx! {
                                    button {
                                        class: if active { "chip active" } else { "chip" },
                                        onclick: move |_| filter.set(v.clone()),
                                        "{lbl}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Employee list
                div { class: "list-section",
                    if filtered.is_empty() {
                        div { class: "empty-state",
                            div { class: "empty-icon", "👥" }
                            p { class: "empty-text", "Никого не найдено" }
                        }
                    } else {
                        for emp in filtered.iter() {
                            {
                                let emp_id = emp.id;
                                let is_editing = editing_id() == Some(emp_id);
                                let t = token.clone();
                                rsx! {
                                    EmployeeCard {
                                        key: "{emp.id}",
                                        employee: (*emp).clone(),
                                        roles: role_list.clone(),
                                        is_editing,
                                        token: t,
                                        can_manage,
                                        can_dismiss: can_dismiss(),
                                        my_employee_id: my_employee_id(),
                                        on_edit_toggle: move |_| {
                                            if editing_id() == Some(emp_id) {
                                                editing_id.set(None);
                                            } else {
                                                editing_id.set(Some(emp_id));
                                            }
                                        },
                                        on_role_changed: move |new_type_id: i64| {
                                            employees.write().iter_mut().for_each(|e| {
                                                if e.id == emp_id {
                                                    e.employee_type_id = new_type_id;
                                                }
                                            });
                                            editing_id.set(None);
                                        },
                                        on_dismissed: move |removed_id: i64| {
                                            employees.write().retain(|e| e.id != removed_id);
                                            editing_id.set(None);
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                div { style: "height: 20px;" }
            }
        }
    }
}

#[component]
fn InviteWizard(
    token: String,
    roles: Vec<EmployeeTypeOption>,
    invitations: Signal<Vec<Invitation>>,
    invite_step: Signal<i32>,
    invite_name: Signal<String>,
    invite_email: Signal<String>,
    invite_tg: Signal<String>,
    invite_position: Signal<String>,
    invite_role_id: Signal<Option<i64>>,
    invite_ttl_days: Signal<i64>,
    invite_error: Signal<Option<String>>,
    invite_loading: Signal<bool>,
    invite_success: Signal<Option<Invitation>>,
    invite_copy_ok: Signal<bool>,
    clipboard_err: Signal<Option<String>>,
) -> Element {
    let step = invite_step();
    let success = invite_success();
    let step_label = if success.is_some() {
        "Готово".to_string()
    } else {
        format!("Шаг {} / 4", step)
    };
    let selected_role_name = roles
        .iter()
        .find(|r| Some(r.id) == invite_role_id())
        .map(|r| r.name.clone())
        .unwrap_or_else(|| "Не выбрано".to_string());
    let contact_label = if !invite_email().is_empty() {
        invite_email()
    } else {
        invite_tg()
    };
    let position_label = if invite_position().is_empty() {
        "-".to_string()
    } else {
        invite_position()
    };
    let qr_url = success.as_ref().and_then(|ok| qr_data_url(&ok.invite_url));

    rsx! {
        div { class: "pad", style: "margin-top: 14px;",
            div { class: "card card-glass",
                div { class: "section-header",
                    span { class: "heading-md", "Приглашение сотрудника" }
                    span { class: "badge badge-muted", "{step_label}" }
                }

                if let Some(err) = invite_error() {
                    div { class: "error-msg", role: "alert", style: "margin-top: 10px;", "{err}" }
                }

                if let Some(ok) = success {
                    div { style: "margin-top: 12px; display:flex; flex-direction:column; gap:10px;",
                        div { class: "card card-green",
                            div { class: "heading-md", "Приглашение отправлено" }
                            div { class: "body-text", style: "margin-top:4px;",
                                "Ссылка создана для роли: {ok.employee_type_name.clone().unwrap_or_else(|| \"Сотрудник\".to_string())}"
                            }
                        }
                        div { class: "form-field",
                            label { class: "field-label", "Ссылка приглашения" }
                            div { style: "display:flex; gap:8px; align-items:center; flex-wrap:wrap;",
                                input {
                                    class: "field-input",
                                    readonly: true,
                                    value: "{ok.invite_url}",
                                    style: "flex:1 1 240px; min-width:0;",
                                }
                                button {
                                    class: if invite_copy_ok() { "btn-secondary" } else { "btn-ghost" },
                                    r#type: "button",
                                    onclick: {
                                        let invite_url = ok.invite_url.clone();
                                        move |_| {
                                            spawn_copy_invite_link(
                                                invite_url.clone(),
                                                invite_copy_ok,
                                                clipboard_err,
                                            );
                                        }
                                    },
                                    if invite_copy_ok() { "Скопировано" } else { "Скопировать" }
                                }
                            }
                        }
                        div {
                            class: "card",
                            style: "padding:18px; border-radius:18px; background:linear-gradient(180deg, rgba(255,255,255,0.05), rgba(255,255,255,0.03)); overflow:hidden;",
                            div { class: "field-label", style: "margin-bottom:8px;", "QR-код приглашения" }
                            div { style: "display:flex; justify-content:center;",
                                div { style: "padding:18px; border-radius:20px; background:#fff; box-shadow:0 12px 28px rgba(0,0,0,0.18);",
                                    if let Some(src) = qr_url.clone() {
                                        img {
                                            src: "{src}",
                                            alt: "QR code",
                                            style: "display:block; width:220px; height:220px; border-radius:12px;",
                                        }
                                    } else {
                                        div {
                                            style: "width:220px; height:220px; display:flex; align-items:center; justify-content:center; text-align:center; color:#666; font-size:13px;",
                                            "Не удалось создать QR-код"
                                        }
                                    }
                                }
                            }
                            div { class: "caption-text", style: "margin-top:8px; text-align:center;", "Отсканируйте, чтобы открыть ссылку регистрации" }
                        }
                        div { style: "display:flex; gap:8px;",
                            button {
                                class: "btn-secondary",
                                onclick: move |_| {
                                    invite_success.set(None);
                                    invite_copy_ok.set(false);
                                    invite_step.set(1);
                                    invite_name.set(String::new());
                                    invite_email.set(String::new());
                                    invite_tg.set(String::new());
                                    invite_position.set(String::new());
                                    invite_role_id.set(None);
                                    invite_ttl_days.set(7);
                                },
                                "Пригласить ещё"
                            }
                            button {
                                class: "btn-primary",
                                onclick: move |_| {
                                    if let Some(window) = web_sys::window() {
                                        let _ = window.open_with_url(&ok.invite_url);
                                    }
                                },
                                "Открыть ссылку"
                            }
                        }
                    }
                } else {
                    if step == 1 {
                        div { style: "margin-top: 12px; display:flex; flex-direction:column; gap:12px;",
                            div { class: "form-field",
                                label { class: "field-label", "Email" }
                                input { class: "field-input", value: "{invite_email}", oninput: move |e| invite_email.set(e.value()), placeholder: "name@company.com" }
                            }
                            div { class: "form-field",
                                label { class: "field-label", "Telegram (опционально)" }
                                input { class: "field-input", value: "{invite_tg}", oninput: move |e| invite_tg.set(e.value()), placeholder: "@username" }
                            }
                            div { class: "form-field",
                                label { class: "field-label", "Имя (опционально)" }
                                input { class: "field-input", value: "{invite_name}", oninput: move |e| invite_name.set(e.value()), placeholder: "Иван Иванов" }
                            }
                        }
                    }

                    if step == 2 {
                        div { style: "margin-top: 12px; display:flex; flex-direction:column; gap:8px;",
                            for role in roles.iter() {
                                {
                                    let rid = role.id;
                                    let selected = invite_role_id() == Some(rid);
                                    rsx! {
                                        button {
                                            class: if selected { "config-item-card active" } else { "config-item-card" },
                                            onclick: move |_| invite_role_id.set(Some(rid)),
                                            div { class: "config-item-content",
                                                div { class: "config-item-title", "{role.name}" }
                                                div { class: "config-item-subtitle", "{role.code}" }
                                            }
                                            if selected {
                                                span { class: "badge badge-amber", "Выбрано" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if step == 3 {
                        div { style: "margin-top: 12px; display:flex; flex-direction:column; gap:12px;",
                            div { class: "form-field",
                                label { class: "field-label", "Должность" }
                                input { class: "field-input", value: "{invite_position}", oninput: move |e| invite_position.set(e.value()), placeholder: "Официант" }
                            }
                            div { class: "body-text", "Можно указать вручную или оставить пустым." }
                        }
                    }

                    if step == 4 {
                        div { style: "margin-top: 12px; display:flex; flex-direction:column; gap:12px;",
                            div { class: "card",
                                div { class: "list-row", style: "padding: 6px 0;",
                                    span { class: "label-text", "Контакт" }
                                    span { class: "body-text", "{contact_label}" }
                                }
                                div { class: "list-row", style: "padding: 6px 0;",
                                    span { class: "label-text", "Роль" }
                                    span { class: "body-text", "{selected_role_name}" }
                                }
                                div { class: "list-row", style: "padding: 6px 0;",
                                    span { class: "label-text", "Должность" }
                                    span { class: "body-text", "{position_label}" }
                                }
                            }
                            div { style: "display:flex; gap:8px; flex-wrap:wrap; margin-top:4px;",
                                {
                                    let cur = invite_ttl_days();
                                    rsx! {
                                        for (days, label) in [(1_i64,"1 день"), (7,"7 дней"), (30,"30 дней")] {
                                            {
                                                let is_active = cur == days;
                                                rsx! {
                                                    button {
                                                        style: if is_active {
                                                            "padding:6px 16px; border-radius:100px; background:rgba(245,166,35,0.15); color:#f5a623; border:1px solid rgba(245,166,35,0.35); font-size:13px; cursor:pointer; font-weight:500; white-space:nowrap;"
                                                        } else {
                                                            "padding:6px 16px; border-radius:100px; background:rgba(255,255,255,0.06); color:var(--text2); border:1px solid rgba(255,255,255,0.09); font-size:13px; cursor:pointer; white-space:nowrap;"
                                                        },
                                                        onclick: move |_| invite_ttl_days.set(days),
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { style: "display:flex; gap:8px; margin-top: 14px;",
                        if step > 1 {
                            button {
                                class: "btn-secondary",
                                onclick: move |_| {
                                    invite_error.set(None);
                                    invite_step.set(step - 1);
                                },
                                "Назад"
                            }
                        }
                        button {
                            class: "btn-primary",
                            disabled: invite_loading(),
                            onclick: move |_| {
                                invite_error.set(None);

                                if step == 1 {
                                    if invite_email().trim().is_empty() && invite_tg().trim().is_empty() {
                                        invite_error.set(Some("Укажите email или Telegram".to_string()));
                                        return;
                                    }
                                    invite_step.set(2);
                                    return;
                                }
                                if step == 2 {
                                    if invite_role_id().is_none() {
                                        invite_error.set(Some("Выберите роль".to_string()));
                                        return;
                                    }
                                    invite_step.set(3);
                                    return;
                                }
                                if step == 3 {
                                    invite_step.set(4);
                                    return;
                                }

                                // Submit step
                                let Some(role_id) = invite_role_id() else {
                                    invite_error.set(Some("Выберите роль".to_string()));
                                    return;
                                };
                                let req = CreateInvitationRequest {
                                    employee_type_id: role_id,
                                    position: if invite_position().trim().is_empty() { None } else { Some(invite_position()) },
                                    full_name: if invite_name().trim().is_empty() { None } else { Some(invite_name()) },
                                    contact_email: if invite_email().trim().is_empty() { None } else { Some(invite_email()) },
                                    contact_telegram: if invite_tg().trim().is_empty() { None } else { Some(invite_tg()) },
                                    ttl_seconds: invite_ttl_days() * 24 * 3600,
                                };
                                let tok = token.clone();
                                let mut invitations_sig = invitations;
                                spawn(async move {
                                    invite_loading.set(true);
                                    match api::create_invitation(&tok, &req).await {
                                        Ok(inv) => {
                                            invite_success.set(Some(inv));
                                            if let Ok(list) = api::fetch_invitations(&tok).await {
                                                invitations_sig.set(list);
                                            }
                                        }
                                        Err(e) => invite_error.set(Some(e)),
                                    }
                                    invite_loading.set(false);
                                });
                            },
                            if invite_loading() {
                                "Отправка..."
                            } else if step < 4 {
                                "Далее"
                            } else {
                                "Отправить приглашение"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EmployeeCard(
    employee: Employee,
    roles: Vec<EmployeeTypeOption>,
    is_editing: bool,
    token: String,
    can_manage: bool,
    can_dismiss: bool,
    my_employee_id: Option<i64>,
    on_edit_toggle: EventHandler<()>,
    on_role_changed: EventHandler<i64>,
    on_dismissed: EventHandler<i64>,
) -> Element {
    let emp = employee.clone();
    let dismiss_token = token.clone();
    let init = initials(&emp.full_name);
    let av_cls = av_color(&emp.full_name);
    let current_role = roles.iter().find(|r| r.id == emp.employee_type_id);
    let role_name = current_role.map(|r| r.name.as_str()).unwrap_or("Сотрудник");
    let role_code = current_role.map(|r| r.code.as_str()).unwrap_or("employee");
    let role_badge = role_badge_class(role_code, emp.is_admin);

    rsx! {
        div { class: "employee-card",
            div { class: "employee-main",
                div { class: "av {av_cls}", "{init}" }
                div { class: "employee-info",
                    div { class: "employee-name", "{emp.full_name}" }
                    if let Some(pos) = &emp.position {
                        if !pos.is_empty() {
                            div { class: "employee-pos", "{pos}" }
                        }
                    }
                }
                div { class: "employee-right",
                    span { class: "badge {role_badge}", "{role_name}" }
                    if can_manage {
                        div { style: "margin-top:4px; display:flex; gap:6px;",
                            button {
                                class: "btn-ghost btn-icon-sm",
                                onclick: move |_| on_edit_toggle.call(()),
                                if is_editing { "✕" } else { "✎" }
                            }
                            if can_dismiss && my_employee_id != Some(emp.id) {
                                button {
                                    class: "btn-ghost btn-icon-sm",
                                    title: "Уволить сотрудника",
                                    onclick: move |_| {
                                        if !confirm_dismiss(&emp.full_name) {
                                            return;
                                        }
                                        let tok = dismiss_token.clone();
                                        let emp_id = emp.id;
                                        spawn(async move {
                                            if api::dismiss_employee(&tok, emp_id).await.is_ok() {
                                                on_dismissed.call(emp_id);
                                            }
                                        });
                                    },
                                    "🗑"
                                }
                            }
                        }
                    }
                }
            }

            if can_manage && is_editing {
                div { class: "role-editor",
                    label { class: "field-label", "Изменить роль:" }
                    div { class: "role-chips",
                        for role in roles.iter() {
                            {
                                let rid = role.id;
                                let selected = emp.employee_type_id == rid;
                                let t = token.clone();
                                let emp_id = emp.id;
                                rsx! {
                                    button {
                                        class: if selected { "chip active" } else { "chip" },
                                        onclick: move |_| {
                                            let tok = t.clone();
                                            let new_id = rid;
                                            spawn(async move {
                                                let _ = api::update_employee_role(&tok, emp_id, new_id).await;
                                            });
                                            on_role_changed.call(rid);
                                        },
                                        "{role.name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
