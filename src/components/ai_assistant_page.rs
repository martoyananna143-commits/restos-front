//! AI assistant page — chat + two-step AI criterion-set creation.

use dioxus::prelude::*;

use crate::api;
use crate::types::{AiChatRequest, AiConfirmRequest, AiCreateCriterionSetRequest, AiMessage, AiPreviewCriterion};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Chat,
    CreateSet,
}

/// Phases of the "Create set via AI" workflow.
#[derive(Clone, PartialEq)]
enum CreatePhase {
    /// User fills in prompt + optional metadata.
    Prompt,
    /// Preview ready for user review and edits.
    Review,
    /// Set was saved successfully.
    Done(String), // success message
}

#[component]
pub fn AiAssistantPage(
    token: String,
    on_back: EventHandler<()>,
    on_created_set: EventHandler<()>,
) -> Element {
    // Wrap in Signal so it's Copy and can be captured by multiple closures.
    let token = use_signal(move || token);

    let mut tab = use_signal(|| Tab::Chat);

    // ── Chat state ───────────────────────────────────────────────────────────
    let mut input = use_signal(String::new);
    let mut sending = use_signal(|| false);
    let mut chat_error = use_signal(|| Option::<String>::None);
    let mut messages = use_signal(|| {
        vec![AiMessage {
            role: "assistant".to_string(),
            content: "Я помогу с идеями и формулировками критериев. Опишите задачу или роль сотрудника, а затем создайте набор во вкладке «Набор через AI».".to_string(),
        }]
    });

    // ── Create-set state ─────────────────────────────────────────────────────
    let mut phase = use_signal(|| CreatePhase::Prompt);

    // Prompt phase inputs
    let mut create_prompt = use_signal(String::new);
    let mut create_set_name = use_signal(String::new);
    let mut create_set_description = use_signal(String::new);
    let mut create_is_default = use_signal(|| false);

    // Review phase editable state
    let mut review_set_name = use_signal(String::new);
    let mut review_description = use_signal(String::new);
    let mut review_criteria: Signal<Vec<AiPreviewCriterion>> = use_signal(Vec::new);

    // Async state
    let mut generating = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut gen_error = use_signal(|| Option::<String>::None);
    let mut save_error = use_signal(|| Option::<String>::None);

    rsx! {
        div { class: "app-screen",
            div { class: "screen-scroll",
                div { class: "config-page-header",
                    button {
                        class: "icon-btn",
                        onclick: move |_| {
                            if phase() == CreatePhase::Review {
                                phase.set(CreatePhase::Prompt);
                            } else {
                                on_back.call(());
                            }
                        },
                        "←"
                    }
                    div { class: "config-page-header-copy",
                        div { class: "section-overline", "Настройки замеров" }
                        div { class: "config-page-title",
                            match phase() {
                                CreatePhase::Review => "Предпросмотр набора",
                                _ => "AI Ассистент",
                            }
                        }
                    }
                    div { class: "icon-btn icon-btn-amber", "🤖" }
                }

                // Tab bar — hidden during review/done phases
                if phase() == CreatePhase::Prompt || matches!(phase(), CreatePhase::Done(_)) {
                    div { class: "filter-row filter-row-tight", style: "margin-top:10px;",
                        button {
                            class: if tab() == Tab::Chat { "chip active" } else { "chip" },
                            onclick: move |_| tab.set(Tab::Chat),
                            "Чат"
                        }
                        button {
                            class: if tab() == Tab::CreateSet { "chip active" } else { "chip" },
                            onclick: move |_| tab.set(Tab::CreateSet),
                            "Набор через AI"
                        }
                    }
                }

                // ── Chat tab ─────────────────────────────────────────────────
                if tab() == Tab::Chat && phase() == CreatePhase::Prompt {
                    div { class: "pad", style: "margin-top: 10px;",
                        div { class: "card info-callout-card",
                            span { class: "info-callout-icon", "✨" }
                            p { class: "info-callout-text", "Опишите контекст: должность, периодичность, тип замера и цели. Чем подробнее запрос, тем лучше ответ." }
                        }
                    }
                    div { class: "list-section config-list-section",
                        for message in messages().iter().filter(|m| m.role != "system") {
                            div {
                                class: if message.role == "user" { "ai-chat-bubble ai-chat-user" } else { "ai-chat-bubble ai-chat-assistant" },
                                div { class: "ai-chat-role", if message.role == "user" { "Вы" } else { "AI" } }
                                p { class: "ai-chat-text", "{message.content}" }
                            }
                        }
                    }
                    div { class: "pad", style: "margin-top: 12px;",
                        {chat_error().as_ref().map(|e| rsx! {
                            div { class: "error-card", style: "margin: 0 0 12px 0; padding: 12px;",
                                p { class: "error-text", "{e}" }
                            }
                        })}
                        div { class: "card config-form-card",
                            textarea {
                                class: "field-input field-textarea",
                                value: "{input()}",
                                placeholder: "Например: Составь 10 критериев для замера работы официанта на вечерней смене...",
                                oninput: move |e| input.set(e.value()),
                            }
                            button {
                                class: "btn-primary",
                                style: "margin-top: 10px;",
                                disabled: sending() || input().trim().is_empty(),
                                onclick: move |_| {
                                    let tok = token();
                                    let message_text = input().trim().to_string();
                                    if message_text.is_empty() { return; }
                                    let history = messages();
                                    let mut optimistic = history.clone();
                                    optimistic.push(AiMessage { role: "user".to_string(), content: message_text.clone() });
                                    messages.set(optimistic);
                                    input.set(String::new());
                                    chat_error.set(None);
                                    sending.set(true);
                                    spawn(async move {
                                        let req = AiChatRequest {
                                            message: message_text,
                                            conversation_history: history,
                                            mode: "criteria_sets".to_string(),
                                        };
                                        match api::ai_chat(&tok, &req).await {
                                            Ok(resp) => {
                                                if resp.conversation_history.is_empty() {
                                                    let mut fallback = messages();
                                                    fallback.push(AiMessage { role: "assistant".to_string(), content: resp.response });
                                                    messages.set(fallback);
                                                } else {
                                                    messages.set(resp.conversation_history);
                                                }
                                            }
                                            Err(e) => chat_error.set(Some(e)),
                                        }
                                        sending.set(false);
                                    });
                                },
                                if sending() { "Отправка..." } else { "Отправить в AI" }
                            }
                        }
                    }
                }

                // ── Create-set tab — PROMPT phase ────────────────────────────
                if tab() == Tab::CreateSet && phase() == CreatePhase::Prompt {
                    div { class: "pad", style: "margin-top: 12px;",
                        div { class: "card info-callout-card",
                            span { class: "info-callout-icon", "🤖" }
                            p { class: "info-callout-text", "AI сгенерирует критерии по вашему описанию. Вы сможете отредактировать список перед сохранением." }
                        }
                    }
                    div { class: "pad", style: "margin-top: 12px;",
                        {gen_error().as_ref().map(|e| rsx! {
                            div { class: "error-card", style: "margin: 0 0 12px 0; padding: 12px;",
                                p { class: "error-text", "{e}" }
                            }
                        })}
                        div { class: "card config-form-card",
                            div { class: "form-field", style: "margin-bottom: 12px;",
                                label { class: "field-label", "Описание задачи для AI" }
                                textarea {
                                    class: "field-input field-textarea",
                                    value: "{create_prompt()}",
                                    placeholder: "Например: Создай набор критериев для менеджера смены в ресторане. Нужны блоки: дисциплина, сервис, командная работа, выполнение KPI.",
                                    oninput: move |e| create_prompt.set(e.value()),
                                }
                            }
                            div { class: "form-field", style: "margin-bottom: 12px;",
                                label { class: "field-label", "Название набора (опционально)" }
                                input {
                                    class: "field-input",
                                    r#type: "text",
                                    value: "{create_set_name()}",
                                    placeholder: "Если пусто — AI предложит название",
                                    oninput: move |e| create_set_name.set(e.value()),
                                }
                            }
                            div { class: "form-field", style: "margin-bottom: 12px;",
                                label { class: "field-label", "Описание набора (опционально)" }
                                textarea {
                                    class: "field-input field-textarea",
                                    value: "{create_set_description()}",
                                    placeholder: "Краткое описание набора",
                                    oninput: move |e| create_set_description.set(e.value()),
                                }
                            }
                            label { class: "checkbox-line", style: "margin-bottom: 4px;",
                                input {
                                    r#type: "checkbox",
                                    checked: create_is_default(),
                                    onchange: move |e| create_is_default.set(e.checked()),
                                }
                                span { "Сделать набором по умолчанию" }
                            }
                            button {
                                class: "btn-primary",
                                style: "margin-top: 12px;",
                                disabled: generating() || create_prompt().trim().is_empty(),
                                onclick: move |_| {
                                    let tok = token();
                                    let prompt = create_prompt().trim().to_string();
                                    if prompt.is_empty() { return; }
                                    let req = AiCreateCriterionSetRequest {
                                        prompt,
                                        set_name: if create_set_name().trim().is_empty() { None } else { Some(create_set_name().trim().to_string()) },
                                        description: if create_set_description().trim().is_empty() { None } else { Some(create_set_description().trim().to_string()) },
                                        is_default: create_is_default(),
                                    };
                                    generating.set(true);
                                    gen_error.set(None);
                                    spawn(async move {
                                        match api::ai_preview_criterion_set(&tok, &req).await {
                                            Ok(preview) => {
                                                review_set_name.set(preview.set_name);
                                                review_description.set(preview.description.unwrap_or_default());
                                                review_criteria.set(preview.criteria);
                                                phase.set(CreatePhase::Review);
                                            }
                                            Err(e) => gen_error.set(Some(e)),
                                        }
                                        generating.set(false);
                                    });
                                },
                                if generating() {
                                    span { class: "ai-generating-label",
                                        span { class: "ai-generating-dot" }
                                        span { class: "ai-generating-dot" }
                                        span { class: "ai-generating-dot" }
                                        "Генерирую..."
                                    }
                                } else {
                                    "Сгенерировать через AI ✨"
                                }
                            }
                        }
                    }
                }

                // ── Create-set tab — REVIEW phase ────────────────────────────
                if phase() == CreatePhase::Review {
                    div { class: "pad", style: "margin-top: 12px;",
                        // Set metadata
                        div { class: "card config-form-card", style: "margin-bottom: 12px;",
                            div { class: "form-field", style: "margin-bottom: 10px;",
                                label { class: "field-label", "Название набора" }
                                input {
                                    class: "field-input",
                                    r#type: "text",
                                    value: "{review_set_name()}",
                                    oninput: move |e| review_set_name.set(e.value()),
                                }
                            }
                            div { class: "form-field",
                                label { class: "field-label", "Описание" }
                                textarea {
                                    class: "field-input field-textarea",
                                    value: "{review_description()}",
                                    placeholder: "Описание набора (опционально)",
                                    oninput: move |e| review_description.set(e.value()),
                                }
                            }
                        }

                        // Criteria list header
                        div { class: "ai-review-list-header",
                            span { class: "section-overline", "Критерии · {review_criteria().len()}" }
                            button {
                                class: "chip",
                                style: "font-size: 12px; padding: 5px 10px;",
                                onclick: move |_| {
                                    review_criteria.with_mut(|v| v.push(AiPreviewCriterion::default()));
                                },
                                "+ Добавить"
                            }
                        }

                        // Criteria cards
                        div { class: "card ai-review-criteria-card",
                            for idx in 0..review_criteria().len() {
                                {
                                    let c = review_criteria.with(|v| v.get(idx).cloned()).unwrap_or_default();
                                    let vtype = c.value_type.clone();
                                    rsx! {
                                        div {
                                            class: "ai-review-criterion",
                                            key: "{idx}",
                                            // Name input — no field-input class (it forces width:100% which breaks flex)
                                            input {
                                                class: "ai-review-criterion-name",
                                                r#type: "text",
                                                value: "{c.name}",
                                                placeholder: "Название критерия",
                                                oninput: move |e| {
                                                    review_criteria.with_mut(|v| {
                                                        if let Some(item) = v.get_mut(idx) {
                                                            item.name = e.value();
                                                        }
                                                    });
                                                },
                                            }
                                            // Type selector
                                            div { class: "ai-review-type-row",
                                                button {
                                                    class: if vtype == "boolean" { "ai-review-type-btn active" } else { "ai-review-type-btn" },
                                                    onclick: move |_| {
                                                        review_criteria.with_mut(|v| {
                                                            if let Some(item) = v.get_mut(idx) {
                                                                item.value_type = "boolean".to_string();
                                                            }
                                                        });
                                                    },
                                                    "☑"
                                                }
                                                button {
                                                    class: if vtype == "number" { "ai-review-type-btn active" } else { "ai-review-type-btn" },
                                                    onclick: move |_| {
                                                        review_criteria.with_mut(|v| {
                                                            if let Some(item) = v.get_mut(idx) {
                                                                item.value_type = "number".to_string();
                                                            }
                                                        });
                                                    },
                                                    "#"
                                                }
                                                button {
                                                    class: if vtype == "string" { "ai-review-type-btn active" } else { "ai-review-type-btn" },
                                                    onclick: move |_| {
                                                        review_criteria.with_mut(|v| {
                                                            if let Some(item) = v.get_mut(idx) {
                                                                item.value_type = "string".to_string();
                                                            }
                                                        });
                                                    },
                                                    "T"
                                                }
                                            }
                                            // Delete button
                                            button {
                                                class: "ai-review-del-btn",
                                                onclick: move |_| {
                                                    review_criteria.with_mut(|v| {
                                                        if idx < v.len() { v.remove(idx); }
                                                    });
                                                },
                                                "×"
                                            }
                                        }
                                    }
                                }
                            }
                            // Empty state
                            if review_criteria().is_empty() {
                                div { class: "ai-review-empty",
                                    p { "Список пуст. Нажмите «+ Добавить», чтобы добавить критерий." }
                                }
                            }
                        }

                        // Error
                        {save_error().as_ref().map(|e| rsx! {
                            div { class: "error-card", style: "margin-top: 12px; padding: 12px;",
                                p { class: "error-text", "{e}" }
                            }
                        })}

                        // Action row
                        div { class: "ai-review-actions",
                            button {
                                class: "btn-ghost",
                                style: "flex: 1;",
                                onclick: move |_| phase.set(CreatePhase::Prompt),
                                "← Изменить запрос"
                            }
                            button {
                                class: "btn-primary",
                                style: "flex: 2;",
                                disabled: saving() || review_criteria().is_empty() || review_set_name().trim().is_empty(),
                                onclick: move |_| {
                                    let tok = token();
                                    let req = AiConfirmRequest {
                                        set_name: review_set_name().trim().to_string(),
                                        description: {
                                            let d = review_description().trim().to_string();
                                            if d.is_empty() { None } else { Some(d) }
                                        },
                                        is_default: create_is_default(),
                                        criteria: review_criteria(),
                                    };
                                    saving.set(true);
                                    save_error.set(None);
                                    spawn(async move {
                                        match api::ai_confirm_criterion_set(&tok, &req).await {
                                            Ok(resp) => {
                                                let msg = format!(
                                                    "Набор «{}» создан (ID {}): {} критериев.",
                                                    resp.set_name, resp.set_id, resp.criteria_created
                                                );
                                                phase.set(CreatePhase::Done(msg));
                                                on_created_set.call(());
                                            }
                                            Err(e) => save_error.set(Some(e)),
                                        }
                                        saving.set(false);
                                    });
                                },
                                if saving() { "Сохранение..." } else { "Сохранить набор" }
                            }
                        }
                    }
                }

                // ── Create-set tab — DONE phase ──────────────────────────────
                if tab() == Tab::CreateSet {
                    if let CreatePhase::Done(msg) = phase() {
                        div { class: "pad", style: "margin-top: 12px;",
                            div { class: "card card-green", style: "padding: 14px; margin-bottom: 14px;",
                                div { style: "font-size: 20px; margin-bottom: 6px;", "✅" }
                                p { class: "body-text", style: "color: var(--text);", "{msg}" }
                            }
                            button {
                                class: "btn-ghost",
                                style: "width: 100%;",
                                onclick: move |_| {
                                    create_prompt.set(String::new());
                                    create_set_name.set(String::new());
                                    create_set_description.set(String::new());
                                    create_is_default.set(false);
                                    gen_error.set(None);
                                    save_error.set(None);
                                    phase.set(CreatePhase::Prompt);
                                },
                                "Создать ещё"
                            }
                        }
                    }
                }

                div { style: "height: 20px;" }
            }
        }
    }
}
