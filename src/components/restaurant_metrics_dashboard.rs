//! Aggregate-only restaurant indicators; no unapproved composite formula.

use dioxus::prelude::*;
use uuid::Uuid;

use super::ProductMeasurementPanel;
use crate::{
    account_session::{AccountSessionAdapter, AccountSessionState},
    operational_walkthrough_api::{
        OperationalWalkthroughApiClient, OperationalWalkthroughApiError,
        OperationalWalkthroughTemplate,
    },
    presentation_percent::{format_percent, format_percentage_points},
    restaurant_metrics_api::{
        LowIndicatorRanking, MetricSourceDrilldown, RestaurantMetric, RestaurantMetricsApiClient,
        RestaurantMetricsApiError,
    },
    workforce_api::{WorkforceApiClient, WorkforceVenue},
};

fn safe_error(error: &RestaurantMetricsApiError) -> &'static str {
    match error {
        RestaurantMetricsApiError::AuthenticationRequired => "Сессия недоступна. Войдите снова.",
        RestaurantMetricsApiError::PermissionDenied => {
            "Доступ к показателям этой компании отозван."
        }
        RestaurantMetricsApiError::NetworkUnavailable => "Нет связи с сервером. Повторите вручную.",
        _ => "Не удалось загрузить показатели. Попробуйте позже.",
    }
}

fn source_label(value: &str) -> &'static str {
    match value {
        "evaluation" => "Оценка",
        "measurement" => "Замер",
        "walkthrough" => "Обход",
        "checklist" => "Чек-лист",
        "test" => "Тест",
        "survey" => "Опрос",
        "attestation" => "Аттестация",
        _ => "Источник",
    }
}

fn score_width(value: &str) -> f64 {
    value.parse::<f64>().unwrap_or(0.0).clamp(0.0, 100.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricsNavigationView {
    Builder,
    Results,
    Restaurant,
    Sources,
}

impl MetricsNavigationView {
    const fn title(self) -> &'static str {
        match self {
            Self::Builder => "Сформировать отчёт",
            Self::Results => "Результаты",
            Self::Restaurant => "Показатели ресторана",
            Self::Sources => "Источники показателей",
        }
    }
}

#[component]
pub fn RestaurantMetricsDashboardPage(
    view: MetricsNavigationView,
    on_walkthrough_started: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<RestaurantMetricsApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let workforce = use_context::<WorkforceApiClient>();
    let mut reload = use_signal(|| 0_u64);
    let mut selected_metric = use_signal(|| None::<String>);
    let mut selected_venue = use_signal(|| None::<Uuid>);
    let mut compare_previous = use_signal(|| true);
    let mut selected_template = use_signal(|| None::<Uuid>);
    let mut selected_source = use_signal(|| None::<String>);
    let mut section_code = use_signal(String::new);
    let venue_session = session.clone();
    let venues = use_resource(move || {
        let state = venue_session.state();
        let workforce = workforce.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Vec::<WorkforceVenue>::new();
            };
            let Some(company_id) = account.selected_company.map(|value| value.0) else {
                return Vec::new();
            };
            workforce
                .venues(&account.access_token, company_id)
                .await
                .unwrap_or_default()
        }
    });
    let load_session = session.clone();
    let data = use_resource(move || {
        let _reload = reload();
        let venue_id = selected_venue();
        let compare = compare_previous();
        let source_type = selected_source();
        let section = section_code();
        let epoch = lifecycle_epoch();
        let api = api.clone();
        let state = load_session.state();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Err(RestaurantMetricsApiError::AuthenticationRequired);
            };
            let company_id = account
                .selected_company
                .map(|value| value.0)
                .ok_or(RestaurantMetricsApiError::InvalidRequest)?;
            let result = api
                .dashboard(
                    &account.access_token,
                    company_id,
                    venue_id,
                    true,
                    compare,
                    source_type.as_deref(),
                    (!section.is_empty()).then_some(section.as_str()),
                )
                .await;
            if lifecycle_epoch() != epoch {
                return Err(RestaurantMetricsApiError::AuthenticationRequired);
            }
            result
        }
    });
    let option_session = session.clone();
    let option_api = use_context::<RestaurantMetricsApiClient>();
    let template_options = use_resource(move || {
        let venue_id = selected_venue();
        let state = option_session.state();
        let api = option_api.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Err(RestaurantMetricsApiError::AuthenticationRequired);
            };
            let company_id = account
                .selected_company
                .map(|value| value.0)
                .ok_or(RestaurantMetricsApiError::InvalidRequest)?;
            api.template_options(&account.access_token, company_id, venue_id)
                .await
        }
    });
    let ranking_session = session.clone();
    let ranking_api = use_context::<RestaurantMetricsApiClient>();
    let ranking = use_resource(move || {
        let template_version_id = selected_template();
        let venue_id = selected_venue();
        let state = ranking_session.state();
        let api = ranking_api.clone();
        async move {
            let Some(template_version_id) = template_version_id else {
                return Ok(None::<LowIndicatorRanking>);
            };
            let AccountSessionState::Authenticated(account) = state else {
                return Err(RestaurantMetricsApiError::AuthenticationRequired);
            };
            let company_id = account
                .selected_company
                .map(|value| value.0)
                .ok_or(RestaurantMetricsApiError::InvalidRequest)?;
            api.low_indicators(
                &account.access_token,
                company_id,
                template_version_id,
                venue_id,
            )
            .await
            .map(Some)
        }
    });
    let source_session = session.clone();
    let source_api = use_context::<RestaurantMetricsApiClient>();
    let sources = use_resource(move || {
        let code = selected_metric();
        let venue_id = selected_venue();
        let source_type = selected_source();
        let section = section_code();
        let state = source_session.state();
        let api = source_api.clone();
        async move {
            let Some(code) = code else {
                return Ok(Vec::<MetricSourceDrilldown>::new());
            };
            let AccountSessionState::Authenticated(account) = state else {
                return Err(RestaurantMetricsApiError::AuthenticationRequired);
            };
            let company_id = account
                .selected_company
                .map(|value| value.0)
                .ok_or(RestaurantMetricsApiError::InvalidRequest)?;
            api.sources(
                &account.access_token,
                company_id,
                &code,
                venue_id,
                source_type.as_deref(),
                (!section.is_empty()).then_some(section.as_str()),
            )
            .await
        }
    });

    rsx! {
        section { class: "metrics-page", aria_labelledby: "restaurant-metrics-title",
            header { class: "metrics-hero",
                div {
                    p { class: "management-eyebrow", "АНАЛИТИКА" }
                    h1 { id: "restaurant-metrics-title", "{view.title()}" }
                    p { "Выберите доступный ресторан, период и источник. RestOS покажет только server-calculated components." }
                }
            }
            div { class: "metrics-notice",
                "Итоговые сводные показатели появятся после утверждения формулы агрегации. Сейчас источники показаны отдельно."
            }
            label { class: "management-company",
                span { "Ресторан" }
                select {
                    value: selected_venue().map(|value| value.to_string()).unwrap_or_default(),
                    onchange: move |event| {
                        selected_metric.set(None);
                        selected_venue.set(Uuid::parse_str(&event.value()).ok());
                    },
                    option { value: "", "Все рестораны" }
                    if let Some(values) = venues() {
                        for venue in values.iter() {
                            option { key: "metric-venue-{venue.venue_id}", value: "{venue.venue_id}", "{venue.name}" }
                        }
                    }
                }
            }
            div { class: "metrics-filters",
                label {
                    span { "Период" }
                    select {
                        value: "today",
                        option { value: "today", "Сегодня" }
                    }
                }
                label {
                    span { "Код зоны/раздела" }
                    input {
                        value: section_code(),
                        maxlength: "100",
                        placeholder: "Например, kitchen",
                        onchange: move |event| section_code.set(event.value().trim().to_string()),
                    }
                }
                label {
                    span { "Тип источника" }
                    select {
                        value: selected_source().unwrap_or_default(),
                        onchange: move |event| {
                            let value = event.value();
                            selected_source.set((!value.is_empty()).then_some(value));
                        },
                        option { value: "", "Все источники" }
                        option { value: "evaluation", "Оценки" }
                        option { value: "walkthrough", "Обходы" }
                        option { value: "measurement", "Замеры" }
                        option { value: "checklist", "Чек-листы" }
                    }
                }
                label {
                    span { "Вид оценки" }
                    select {
                        value: selected_template().map(|value| value.to_string()).unwrap_or_default(),
                        onchange: move |event| selected_template.set(Uuid::parse_str(&event.value()).ok()),
                        option { value: "", "Выберите вид оценки" }
                        if let Some(Ok(values)) = template_options() {
                            for option in values.iter() {
                                option { key: "metric-template-{option.template_version_id}", value: "{option.template_version_id}", "{option.name} ({option.completed_observation_count})" }
                            }
                        }
                    }
                }
                label {
                    span { "Сравнение" }
                    input { r#type: "checkbox", checked: compare_previous(), onchange: move |event| compare_previous.set(event.checked()) }
                    small { "С предыдущим операционным днём" }
                }
            }
            button { class: "btn-primary metrics-generate", r#type: "button", onclick: move |_| { selected_metric.set(None); reload += 1; }, "Сформировать отчёт" }
            ProductMeasurementPanel {}
            OperationalWalkthroughPanel { on_started: on_walkthrough_started }
            match data() {
                None => rsx! { div { class: "management-state", "Загрузка показателей..." } },
                Some(Err(problem)) => rsx! {
                    div { class: "management-state management-error",
                        p { "{safe_error(&problem)}" }
                        button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" }
                    }
                },
                Some(Ok(dashboard)) => rsx! {
                    div { class: "metrics-report-context", role: "status",
                        strong { "Отчёт по компонентам" }
                        span { "Период и scope проверены backend. Composite average и рейтинг худших показателей не рассчитываются без методики." }
                    }
                    div { class: "metrics-grid",
                        for metric in dashboard.metrics.iter() {
                            MetricCard { key: "{metric.code}", metric: metric.clone(), selected_metric }
                        }
                    }
                    if let Some(template_version_id) = selected_template() {
                        section { class: "metric-drilldown", aria_labelledby: "low-indicators-title",
                            h2 { id: "low-indicators-title", "Низкие показатели" }
                            p { "Рейтинг внутри выбранного вида оценки: минимум 3 завершённых наблюдения, максимум 10 позиций." }
                            match ranking() {
                                None => rsx! { p { "Загрузка рейтинга..." } },
                                Some(Err(problem)) => rsx! { p { class: "management-error", "{safe_error(&problem)}" } },
                                Some(Ok(None)) => rsx! { p { "Выберите вид оценки." } },
                                Some(Ok(Some(value))) if value.items.is_empty() => rsx! { p { class: "management-muted", "Недостаточно сопоставимых наблюдений." } },
                                Some(Ok(Some(value))) => rsx! { ol { class: "metric-components",
                                    for item in value.items.iter() {
                                        {
                                            let score = format_percent(&item.score_percent).unwrap_or_else(|| "—".into());
                                            rsx! { li { class: "metric-source-row", key: "low-{template_version_id}-{item.item_code}",
                                                strong { "#{item.rank} · {item.label}" }
                                                small { "{score} · наблюдений {item.sample_count} · покрытие {item.coverage}" }
                                                if item.critical_failure_count > 0 { span { class: "management-error", "Критические отклонения: {item.critical_failure_count}" } }
                                                if item.stop_factor_count > 0 { span { class: "management-error", "Стоп-факторы: {item.stop_factor_count}" } }
                                            } }
                                        }
                                    }
                                } },
                            }
                        }
                    }
                    if let Some(code) = selected_metric() {
                        section { class: "metric-drilldown", aria_label: "Источники показателя",
                            div { class: "metric-card-head", h2 { "Источники: {code}" } button { class: "btn-ghost", r#type:"button", onclick:move |_| selected_metric.set(None), "Закрыть" } }
                            match sources() {
                                None => rsx! { p { "Загрузка источников..." } },
                                Some(Err(problem)) => rsx! { p { class:"management-error", "{safe_error(&problem)}" } },
                                Some(Ok(values)) if values.is_empty() => rsx! { p { class:"management-muted", "Источников пока нет." } },
                                Some(Ok(values)) => rsx! {
                                    div { class: "metric-components",
                                        for value in values.iter() {
                                            {
                                                let score = format_percent(&value.score_percent).unwrap_or_else(|| "—".into());
                                                rsx! { article { class: "metric-source-row",
                                                    strong { "{source_label(&value.source_type)} · {score}" }
                                                    small { "{value.observed_at} · критериев {value.items.len()} · покрытие {value.coverage}" }
                                                    if !value.items.is_empty() {
                                                        ul { class: "metric-criteria",
                                                            for item in value.items.iter() {
                                                                li {
                                                                if let Some(section) = item.section_code.as_deref() {
                                                                    span { "{section} / " }
                                                                }
                                                                if let Some(code) = item.item_code.as_deref() {
                                                                    strong { "{code}" }
                                                                }
                                                                if let Some(normalized) = item.normalized.as_deref() {
                                                                    span { " · {normalized}" }
                                                                }
                                                                if item.critical_failure == Some(true) {
                                                                    span { class: "management-error", " · критическое отклонение" }
                                                                }
                                                                }
                                                            }
                                                        }
                                                    }
                                                } }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                },
            }
        }
    }
}

fn walkthrough_error(error: &OperationalWalkthroughApiError) -> &'static str {
    match error {
        OperationalWalkthroughApiError::AuthenticationRequired => {
            "Сессия недоступна. Войдите снова."
        }
        OperationalWalkthroughApiError::PermissionDenied => "Недостаточно прав для запуска обхода.",
        OperationalWalkthroughApiError::NotFound => "Шаблон или ресторан больше недоступен.",
        OperationalWalkthroughApiError::Conflict => "Для этого шаблона уже есть активный обход.",
        OperationalWalkthroughApiError::NetworkUnavailable => {
            "Нет связи с сервером. Повторите вручную."
        }
        OperationalWalkthroughApiError::InternalError => "Не удалось запустить обход.",
    }
}

#[component]
fn OperationalWalkthroughPanel(on_started: EventHandler<()>) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OperationalWalkthroughApiClient>();
    let workforce = use_context::<WorkforceApiClient>();
    let lifecycle_epoch = use_context::<Signal<u64>>();
    let mut reload = use_signal(|| 0_u64);
    let mut venue_id = use_signal(|| None::<Uuid>);
    let starting = use_signal(|| None::<Uuid>);
    let message = use_signal(|| None::<String>);

    let load_session = session.clone();
    let templates = use_resource(move || {
        let _reload = reload();
        let state = load_session.state();
        let api = api.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Err(OperationalWalkthroughApiError::AuthenticationRequired);
            };
            let company_id = account
                .selected_company
                .map(|value| value.0)
                .ok_or(OperationalWalkthroughApiError::NotFound)?;
            api.templates(&account.access_token, company_id).await
        }
    });
    let venue_session = session.clone();
    let venues = use_resource(move || {
        let state = venue_session.state();
        let workforce = workforce.clone();
        async move {
            let AccountSessionState::Authenticated(account) = state else {
                return Vec::<WorkforceVenue>::new();
            };
            let Some(company_id) = account.selected_company.map(|value| value.0) else {
                return Vec::new();
            };
            workforce
                .venues(&account.access_token, company_id)
                .await
                .unwrap_or_default()
        }
    });

    rsx! {
        section { class: "measurement-panel walkthrough-panel", aria_labelledby: "walkthrough-panel-title",
            div { class: "measurement-panel-head",
                div {
                    p { class: "management-eyebrow", "ОПЕРАЦИОННЫЕ ОБХОДЫ" }
                    h2 { id: "walkthrough-panel-title", "Провести обход ресторана" }
                    p { "Выберите опубликованный шаблон и ресторан. Результаты попадут в показатели после завершения." }
                }
            }
            label { class: "management-company",
                span { "Ресторан" }
                select {
                    value: venue_id().map(|value| value.to_string()).unwrap_or_default(),
                    onchange: move |event| venue_id.set(Uuid::parse_str(&event.value()).ok()),
                    option { value: "", "Выберите ресторан" }
                    if let Some(values) = venues() {
                        for venue in values.iter() {
                            option { key: "walkthrough-venue-{venue.venue_id}", value: "{venue.venue_id}", "{venue.name}" }
                        }
                    }
                }
            }
            if let Some(text) = message() {
                p { class: "management-state", role: "status", "{text}" }
            }
            match templates() {
                None => rsx! { p { class: "management-state", "Загрузка шаблонов обхода..." } },
                Some(Err(problem)) => rsx! {
                    div { class: "management-state management-error",
                        p { "{walkthrough_error(&problem)}" }
                        button { class: "btn-secondary", r#type: "button", onclick: move |_| reload += 1, "Повторить" }
                    }
                },
                Some(Ok(values)) if values.is_empty() => rsx! {
                    p { class: "management-state", "Опубликованных шаблонов обхода пока нет." }
                },
                Some(Ok(values)) => rsx! {
                    div { class: "measurement-list",
                        for template in values.iter() {
                            WalkthroughTemplateCard {
                                key: "walkthrough-template-{template.template_version_id}",
                                template: template.clone(),
                                venue_id,
                                starting,
                                message,
                                lifecycle_epoch,
                                on_started: on_started.clone(),
                            }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn WalkthroughTemplateCard(
    template: OperationalWalkthroughTemplate,
    venue_id: Signal<Option<Uuid>>,
    starting: Signal<Option<Uuid>>,
    message: Signal<Option<String>>,
    lifecycle_epoch: Signal<u64>,
    on_started: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSessionAdapter>();
    let api = use_context::<OperationalWalkthroughApiClient>();
    let version_id = template.template_version_id;
    rsx! {
        article { class: "measurement-row walkthrough-template-card",
            div {
                strong { "{template.name}" }
                p { "Версия {template.version} · разделов {template.section_count} · вопросов {template.item_count}" }
                small { "Расчёт: взвешенная методика RestOS" }
            }
            button {
                class: "btn-primary",
                r#type: "button",
                disabled: venue_id().is_none() || starting().is_some() || !template.scoring_ready,
                onclick: move |_| {
                    let Some(selected_venue) = venue_id() else { return; };
                    let AccountSessionState::Authenticated(account) = session.state() else {
                        message.set(Some("Сессия недоступна. Войдите снова.".to_string()));
                        return;
                    };
                    let Some(company_id) = account.selected_company.map(|value| value.0) else { return; };
                    starting.set(Some(version_id));
                    message.set(None);
                    let api = api.clone();
                    let token = account.access_token.clone();
                    let epoch = lifecycle_epoch();
                    spawn(async move {
                        let result = api.start(&token, company_id, selected_venue, version_id).await;
                        if lifecycle_epoch() != epoch { return; }
                        starting.set(None);
                        match result {
                            Ok(_) => {
                                message.set(Some("Обход создан. Открываем рабочий бланк...".to_string()));
                                on_started.call(());
                            }
                            Err(problem) => message.set(Some(walkthrough_error(&problem).to_string())),
                        }
                    });
                },
                if starting() == Some(version_id) { "Запуск..." } else { "Начать обход" }
            }
        }
    }
}

#[component]
fn MetricCard(metric: RestaurantMetric, mut selected_metric: Signal<Option<String>>) -> Element {
    rsx! {
        article { class: "metric-card",
            div { class: "metric-card-head",
                h2 { "{metric.title}" }
                span { class: "metric-code", "{metric.code}" }
            }
            if metric.components.is_empty() {
                p { class: "management-muted", "Пока нет завершённых источников." }
            } else {
                div { class: "metric-components",
                    for component in metric.components.iter() {
                        {
                            let width = score_width(&component.score_percent);
                            let score_label = format_percent(&component.score_percent).unwrap_or_else(|| "—".into());
                            rsx! {
                                div { class: "metric-component",
                                    div { class: "metric-component-line",
                                        strong { "{source_label(&component.source_type)}" }
                                        span { "{score_label}" }
                                    }
                                    div { class: "metric-track", aria_hidden: "true",
                                        div { class: "metric-fill", style: "width:{width:.2}%;" }
                                    }
                                    small {
                                        "Покрытие {component.coverage} · источников {component.observation_count}"
                                    }
                                    if component.comparison_status == "comparable" {
                                        if let Some(delta) = component.delta.as_deref() {
                                            {
                                                let delta_label = format_percentage_points(delta).unwrap_or_else(|| "—".into());
                                                rsx! { small { "К предыдущему операционному дню: {delta_label}" } }
                                            }
                                        }
                                    } else if component.comparison_status == "not_comparable" {
                                        small { "Сравнение недоступно: набор компонентов отличается." }
                                    }
                                    if component.critical_failure_count > 0 {
                                        small { class: "management-error", "Критических отклонений: {component.critical_failure_count}" }
                                    }
                                    if component.stop_factor_count > 0 {
                                        small { class: "management-error", "Стоп-факторов: {component.stop_factor_count}" }
                                    }
                                }
                            }
                        }
                    }
                }
                button { class:"btn-secondary", r#type:"button", onclick:move |_| selected_metric.set(Some(metric.code.clone())), "Показать источники" }
            }
        }
    }
}
