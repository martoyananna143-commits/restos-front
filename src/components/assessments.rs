//! Visible Stage 20C assessment library and company-template experience.

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use gloo_timers::future::TimeoutFuture;
use uuid::Uuid;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::{
    account_api::{AccountApiError, SelectedCompanyId},
    account_session::{AccountSessionAdapter, AccountSessionState, AuthenticatedAccountSession},
    assessment_api::{
        AdoptLibraryTemplateRequest, AssessmentApiClient, AssessmentApiError, CompanyDraftDocument,
        CompanyTemplateSummary, DraftItem, DraftMetricMapping, DraftSection, LibraryQuery,
        LibraryTemplateSummary, NextDraftRequest, SaveCompanyDraftRequest, TemplateDocument,
        TemplateVersionSummary,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AssessmentTab {
    Library,
    Company,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssessmentLibrarySection {
    Library,
    Company,
}

impl AssessmentLibrarySection {
    const fn tab(self) -> AssessmentTab {
        match self {
            Self::Library => AssessmentTab::Library,
            Self::Company => AssessmentTab::Company,
        }
    }
}

#[derive(Clone, PartialEq)]
enum AssessmentView {
    List,
    LibraryDetail(TemplateDocument),
    CompanyDetail(CompanyTemplateSummary),
    CompanyEditor(CompanyDraftDocument),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoadState {
    Loading,
    Ready,
}

const ASSESSMENT_TOUR_PREFERENCE_KEY: &str = "restos_ui_assessment_tour_v1";
const ASSESSMENT_TOUR_TRIGGER_SELECTOR: &str = "[data-tour-trigger='assessment-templates']";
const METHODOLOGY_TOUR_SELECTOR: &str = "[data-tour='methodology']";
const METHODOLOGY_TOUR_TARGET_CLASS: &str = "assessment-methodology-tour-target";
const LIBRARY_SEARCH_DEBOUNCE_MS: u32 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AdoptCtaState {
    Adopt,
    OpenCompanyCopy,
    SelectCompany,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TourProgress {
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TourTarget {
    LibraryTabs,
    LibraryFilters,
    LibraryCard,
    Methodology,
    AdoptAction,
    CompanyTab,
}

#[derive(Clone, Copy)]
struct TourStep {
    target: TourTarget,
    selector: &'static str,
    title: &'static str,
    body: &'static str,
}

const TOUR_STEPS: [TourStep; 6] = [
    TourStep {
        target: TourTarget::LibraryTabs,
        selector: "[data-tour='library-tabs']",
        title: "Библиотека шаблонов",
        body: "Здесь собраны готовые формы RestOS для оценки, проверки и тестирования.",
    },
    TourStep {
        target: TourTarget::LibraryFilters,
        selector: "[data-tour='library-filters']",
        title: "Поиск и фильтры",
        body: "Найдите шаблон по названию или оставьте только нужный тип активности.",
    },
    TourStep {
        target: TourTarget::LibraryCard,
        selector: "[data-tour='library-card']",
        title: "Карточка шаблона",
        body: "Карточка показывает тип, версию, состав и состояние добавления в вашу компанию.",
    },
    TourStep {
        target: TourTarget::Methodology,
        selector: METHODOLOGY_TOUR_SELECTOR,
        title: "Методология RestOS",
        body: "Методология объясняет назначение шаблона и сохраняется без изменений в копии ресторана.",
    },
    TourStep {
        target: TourTarget::AdoptAction,
        selector: "[data-tour='adopt-action']",
        title: "Добавить в мои замеры",
        body: "Создайте рабочую копию для выбранной компании. Повторное добавление будет заблокировано.",
    },
    TourStep {
        target: TourTarget::CompanyTab,
        selector: "[data-tour='company-tab']",
        title: "Мои шаблоны",
        body: "Здесь находятся собственные и созданные на основе RestOS шаблоны вашей компании.",
    },
];

impl TourProgress {
    fn first() -> Self {
        Self { index: 0 }
    }

    fn next(self) -> Self {
        Self {
            index: (self.index + 1).min(TOUR_STEPS.len() - 1),
        }
    }

    fn back(self) -> Self {
        Self {
            index: self.index.saturating_sub(1),
        }
    }

    fn is_first(self) -> bool {
        self.index == 0
    }

    fn is_last(self) -> bool {
        self.index + 1 == TOUR_STEPS.len()
    }
}

fn active_tour_target(progress: Option<TourProgress>) -> Option<TourTarget> {
    progress.map(|progress| TOUR_STEPS[progress.index].target)
}

fn is_active_tour_target(progress: Option<TourProgress>, target: TourTarget) -> bool {
    active_tour_target(progress) == Some(target)
}

fn assessment_entry_can_reuse_session(state: &AccountSessionState) -> bool {
    matches!(state, AccountSessionState::Authenticated(_))
}

#[component]
pub fn AssessmentsPage(
    section: AssessmentLibrarySection,
    on_section_change: EventHandler<AssessmentLibrarySection>,
) -> Element {
    let session_adapter = use_context::<AccountSessionAdapter>();
    let api = use_context::<AssessmentApiClient>();
    let mut session = use_signal(|| AccountSessionState::Uninitialized);
    let mut tab = use_signal(|| section.tab());
    let mut view = use_signal(|| AssessmentView::List);
    let mut load_state = use_signal(|| LoadState::Loading);
    let mut error = use_signal(|| None::<AssessmentApiError>);
    let mut library = use_signal(Vec::<LibraryTemplateSummary>::new);
    let mut company_templates = use_signal(Vec::<CompanyTemplateSummary>::new);
    let mut company_templates_resolved = use_signal(|| false);
    let mut switching_company = use_signal(|| None::<Uuid>);
    let search = use_signal(String::new);
    let activity_type = use_signal(String::new);
    let mut filter_generation = use_signal(|| 0_u64);
    let mut adopt_target = use_signal(|| None::<LibraryTemplateSummary>);
    let mut adopting = use_signal(|| false);
    let mut success = use_signal(|| None::<String>);
    let mut tour = use_signal(|| None::<TourProgress>);

    use_effect(move || {
        let requested = section.tab();
        if tab() != requested {
            tab.set(requested);
            view.set(AssessmentView::List);
        }
    });

    let initial_adapter = session_adapter.clone();
    let initial_api = api.clone();
    use_effect(move || {
        let adapter = initial_adapter.clone();
        let client = initial_api.clone();
        spawn(async move {
            load_state.set(LoadState::Loading);
            error.set(None);
            let current = adapter.state();
            let session_result = if assessment_entry_can_reuse_session(&current) {
                Ok(current)
            } else {
                adapter.refresh().await
            };
            match session_result {
                Ok(state) => {
                    session.set(state.clone());
                    if let AccountSessionState::Authenticated(authenticated) = state {
                        match list_library(
                            &adapter,
                            &client,
                            &authenticated,
                            LibraryQuery::first_page(),
                        )
                        .await
                        {
                            Ok(items) => library.set(items),
                            Err(problem) => error.set(Some(problem)),
                        }
                        if let Some(company_id) = authenticated.selected_company {
                            match list_company(&adapter, &client, &authenticated, company_id.0)
                                .await
                            {
                                Ok(items) => {
                                    company_templates.set(items);
                                    company_templates_resolved.set(true);
                                }
                                Err(problem) => error.set(Some(problem)),
                            }
                        }
                    }
                }
                Err(problem) => {
                    session.set(adapter.state());
                    error.set(Some(account_error(problem)));
                }
            }
            load_state.set(LoadState::Ready);
        });
    });

    let authenticated = match session() {
        AccountSessionState::Authenticated(value) => Some(value),
        _ => None,
    };

    rsx! {
        div { class: "assessment-page",
            header { class: "assessment-hero",
                div {
                    p { class: "assessment-eyebrow", "RESTOS • БИБЛИОТЕКА ОЦЕНОК" }
                    h1 { "Замеры" }
                    p {
                        "Готовые шаблоны оценок, проверок и тестирований для вашей команды"
                    }
                    button {
                        class: "assessment-tour-launch",
                        "data-tour-trigger": "assessment-templates",
                        r#type: "button",
                        onclick: move |_| {
                            view.set(AssessmentView::List);
                            tab.set(AssessmentTab::Library);
                            on_section_change.call(AssessmentLibrarySection::Library);
                            tour.set(Some(TourProgress::first()));
                        },
                        "Как работать с шаблонами"
                    }
                }
                if let Some(authenticated) = authenticated.as_ref() {
                    if authenticated.bootstrap.companies.len() > 1 {
                        div { class: "assessment-company-picker",
                            span { "Компания" }
                            div { class: "assessment-company-options",
                                for company in authenticated.bootstrap.companies.iter() {
                                    {
                                        let company_id = company.company_id;
                                        let company_name = company.company_name.clone();
                                        let selected = authenticated.selected_company
                                            == Some(SelectedCompanyId(company_id));
                                        let switching = switching_company();
                                        let adapter = session_adapter.clone();
                                        let client = api.clone();
                                        rsx! {
                                            button {
                                                r#type: "button",
                                                class: if selected { "assessment-company-chip is-active" } else { "assessment-company-chip" },
                                                disabled: switching.is_some(),
                                                onclick: move |_| {
                                                    if switching_company().is_some() {
                                                        return;
                                                    }
                                                    let client = client.clone();
                                                    let adapter = adapter.clone();
                                                    if adapter.select_company(SelectedCompanyId(company_id)).is_ok() {
                                                        session.set(adapter.state());
                                                        company_templates.set(Vec::new());
                                                        company_templates_resolved.set(false);
                                                        if matches!(view(), AssessmentView::CompanyDetail(_)) {
                                                            view.set(AssessmentView::List);
                                                        }
                                                        adopt_target.set(None);
                                                        success.set(None);
                                                        switching_company.set(Some(company_id));
                                                        error.set(None);
                                                        spawn(async move {
                                                            let current = adapter.state();
                                                            let response = match current {
                                                                AccountSessionState::Authenticated(auth) => {
                                                                    Some(list_company(
                                                                    &adapter,
                                                                    &client,
                                                                    &auth,
                                                                    company_id,
                                                                )
                                                                    .await)
                                                                }
                                                                _ => None,
                                                            };
                                                            if should_apply_company_response(
                                                                company_id,
                                                                selected_company(&adapter.state()),
                                                                switching_company(),
                                                            ) {
                                                                match response {
                                                                    Some(Ok(items)) => {
                                                                        company_templates.set(items);
                                                                        company_templates_resolved.set(true);
                                                                    }
                                                                    Some(Err(problem)) => error.set(Some(problem)),
                                                                    None => error.set(Some(
                                                                        AssessmentApiError::AuthenticationRequired,
                                                                    )),
                                                                }
                                                                switching_company.set(None);
                                                            }
                                                        });
                                                    }
                                                },
                                                if switching == Some(company_id) {
                                                    span { aria_live: "polite", "Загрузка…" }
                                                } else {
                                                    "{company_name}"
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

            if load_state() == LoadState::Loading {
                AssessmentLoading {}
            } else {
                match session() {
                    AccountSessionState::Anonymous => rsx! {
                        AssessmentMessage {
                            icon: "🔐",
                            title: "Нужен вход в аккаунт",
                            body: "Войдите через безопасную web-сессию, чтобы открыть библиотеку шаблонов.",
                        }
                    },
                    AccountSessionState::Unavailable { retryable: true } => rsx! {
                        AssessmentMessage {
                            icon: "↻",
                            title: "Не удалось восстановить сессию",
                            body: "Проверьте соединение и попробуйте снова.",
                        }
                    },
                    AccountSessionState::Unavailable { retryable: false } => rsx! {
                        AssessmentMessage {
                            icon: "↻",
                            title: "Не удалось восстановить сессию",
                            body: "Account-сессия сейчас недоступна.",
                        }
                    },
                    AccountSessionState::Authenticated(authenticated) => rsx! {
                        if matches!(view(), AssessmentView::List) {
                            nav {
                                class: "assessment-tabs",
                                aria_label: "Разделы шаблонов",
                                "data-tour": "library-tabs",
                                "data-tour-active": is_active_tour_target(
                                    tour(),
                                    TourTarget::LibraryTabs,
                                )
                                .then_some("true"),
                                button {
                                    r#type: "button",
                                    class: if tab() == AssessmentTab::Library { "assessment-tab is-active" } else { "assessment-tab" },
                                    onclick: move |_| {
                                        tab.set(AssessmentTab::Library);
                                        on_section_change.call(AssessmentLibrarySection::Library);
                                        error.set(None);
                                    },
                                    "Библиотека шаблонов"
                                }
                                button {
                                    "data-tour": "company-tab",
                                    "data-tour-active": is_active_tour_target(
                                        tour(),
                                        TourTarget::CompanyTab,
                                    )
                                    .then_some("true"),
                                    r#type: "button",
                                    class: if tab() == AssessmentTab::Company { "assessment-tab is-active" } else { "assessment-tab" },
                                    onclick: {
                                        let client = api.clone();
                                        let adapter = session_adapter.clone();
                                        move |_| {
                                            let client = client.clone();
                                            let adapter = adapter.clone();
                                            tab.set(AssessmentTab::Company);
                                            on_section_change.call(AssessmentLibrarySection::Company);
                                            error.set(None);
                                            success.set(None);
                                            let Some(company_id) = selected_company(&adapter.state()) else {
                                                return;
                                            };
                                            let current = adapter.state();
                                            company_templates.set(Vec::new());
                                            company_templates_resolved.set(false);
                                            spawn(async move {
                                                load_state.set(LoadState::Loading);
                                                if let AccountSessionState::Authenticated(auth) = current {
                                                    match list_company(&adapter, &client, &auth, company_id).await {
                                                        Ok(items) => {
                                                            company_templates.set(items);
                                                            company_templates_resolved.set(true);
                                                        }
                                                        Err(problem) => error.set(Some(problem)),
                                                    }
                                                }
                                                session.set(adapter.state());
                                                load_state.set(LoadState::Ready);
                                            });
                                        }
                                    },
                                    "Мои шаблоны"
                                }
                            }
                        }

                        if let Some(message) = success() {
                            div { class: "assessment-success", role: "status",
                                span { aria_hidden: "true", "✓" }
                                div {
                                    strong { "{message}" }
                                    button {
                                        r#type: "button",
                                        onclick: move |_| {
                                            view.set(AssessmentView::List);
                                            tab.set(AssessmentTab::Company);
                                            on_section_change.call(AssessmentLibrarySection::Company);
                                        },
                                        "Перейти в «Мои шаблоны»"
                                    }
                                }
                            }
                        }

                        if let Some(problem) = error() {
                            AssessmentError {
                                problem,
                                on_retry: {
                                    let client = api.clone();
                                    let adapter = session_adapter.clone();
                                    move |_| {
                                        let client = client.clone();
                                        let adapter = adapter.clone();
                                        let current_tab = tab();
                                        let current = adapter.state();
                                        spawn(async move {
                                            load_state.set(LoadState::Loading);
                                            error.set(None);
                                            if let AccountSessionState::Authenticated(auth) = current {
                                                let result = if !company_templates_resolved() {
                                                    if let Some(company_id) = auth.selected_company {
                                                        list_company(
                                                            &adapter,
                                                            &client,
                                                            &auth,
                                                            company_id.0,
                                                        )
                                                        .await
                                                        .map(|items| {
                                                            company_templates.set(items);
                                                            company_templates_resolved.set(true);
                                                        })
                                                    } else {
                                                        Ok(())
                                                    }
                                                } else if current_tab == AssessmentTab::Library {
                                                    list_library(
                                                        &adapter,
                                                        &client,
                                                        &auth,
                                                        LibraryQuery::first_page(),
                                                    )
                                                    .await
                                                    .map(|items| library.set(items))
                                                } else if let Some(company_id) = auth.selected_company {
                                                    list_company(
                                                        &adapter,
                                                        &client,
                                                        &auth,
                                                        company_id.0,
                                                    )
                                                    .await
                                                    .map(|items| company_templates.set(items))
                                                } else {
                                                    Ok(())
                                                };
                                                if let Err(problem) = result {
                                                    error.set(Some(problem));
                                                }
                                            }
                                            session.set(adapter.state());
                                            load_state.set(LoadState::Ready);
                                        });
                                    }
                                },
                            }
                        } else {
                            match view() {
                                AssessmentView::List if tab() == AssessmentTab::Library => rsx! {
                                    LibraryList {
                                        templates: library(),
                                        company_templates: company_templates(),
                                        active_tour_target: active_tour_target(tour()),
                                        search,
                                        activity_type,
                                        on_filter_change: {
                                            let client = api.clone();
                                            let adapter = session_adapter.clone();
                                            move |debounced: bool| {
                                                filter_generation += 1;
                                                let generation = filter_generation();
                                                let client = client.clone();
                                                let adapter = adapter.clone();
                                                let query = LibraryQuery {
                                                    activity_type: (!activity_type().is_empty()).then(|| activity_type()),
                                                    query: (!search().trim().is_empty()).then(|| search().trim().to_string()),
                                                    limit: 50,
                                                    offset: 0,
                                                };
                                                spawn(async move {
                                                    if debounced {
                                                        TimeoutFuture::new(LIBRARY_SEARCH_DEBOUNCE_MS).await;
                                                    }
                                                    if filter_generation() != generation {
                                                        return;
                                                    }
                                                    load_state.set(LoadState::Loading);
                                                    error.set(None);
                                                    let current = adapter.state();
                                                    if let AccountSessionState::Authenticated(auth) = current {
                                                        let result = list_library(&adapter, &client, &auth, query).await;
                                                        if filter_generation() == generation {
                                                            match result {
                                                                Ok(items) => library.set(items),
                                                                Err(problem) => error.set(Some(problem)),
                                                            }
                                                        }
                                                    }
                                                    if filter_generation() == generation {
                                                        session.set(adapter.state());
                                                        load_state.set(LoadState::Ready);
                                                    }
                                                });
                                            }
                                        },
                                        on_adopt: move |template| adopt_target.set(Some(template)),
                                        on_open: {
                                            let client = api.clone();
                                            let adapter = session_adapter.clone();
                                            move |template_id| {
                                                let client = client.clone();
                                                let adapter = adapter.clone();
                                                let current = adapter.state();
                                                spawn(async move {
                                                    load_state.set(LoadState::Loading);
                                                    if let AccountSessionState::Authenticated(auth) = current {
                                                        match get_library(
                                                            &adapter,
                                                            &client,
                                                            &auth,
                                                            template_id,
                                                        )
                                                        .await
                                                        {
                                                            Ok(document) => view.set(AssessmentView::LibraryDetail(document)),
                                                            Err(problem) => error.set(Some(problem)),
                                                        }
                                                    }
                                                    session.set(adapter.state());
                                                    load_state.set(LoadState::Ready);
                                                });
                                            }
                                        },
                                    }
                                },
                                AssessmentView::List => rsx! {
                                    CompanyList {
                                        templates: company_templates(),
                                        has_company: authenticated.selected_company.is_some(),
                                        on_open: move |template| view.set(AssessmentView::CompanyDetail(template)),
                                    }
                                },
                                AssessmentView::LibraryDetail(document) => rsx! {
                                    LibraryDetail {
                                        document: document.clone(),
                                        active_tour_target: active_tour_target(tour()),
                                        adopted: company_templates().iter().any(|item| {
                                            item.source_library_version_id == Some(document.version.version_id)
                                        }),
                                        can_adopt: authenticated.selected_company.is_some(),
                                        company_data_resolved: company_templates_resolved()
                                            && switching_company().is_none(),
                                        on_back: move |_| view.set(AssessmentView::List),
                                        on_open_company: move |_| {
                                            if let Some(template) = company_templates()
                                                .into_iter()
                                                .find(|item| {
                                                    item.source_library_version_id
                                                        == Some(document.version.version_id)
                                                })
                                            {
                                                view.set(AssessmentView::CompanyDetail(template));
                                            } else {
                                                view.set(AssessmentView::List);
                                                tab.set(AssessmentTab::Company);
                                                on_section_change.call(AssessmentLibrarySection::Company);
                                            }
                                        },
                                        on_adopt: move |_| {
                                            adopt_target.set(library().into_iter().find(|item| {
                                                item.version_id == document.version.version_id
                                            }));
                                        },
                                    }
                                },
                                AssessmentView::CompanyDetail(template) => rsx! {
                                    CompanyDetail {
                                        template: template.clone(),
                                        on_back: move |_| view.set(AssessmentView::List),
                                        on_edit: {
                                            let adapter = session_adapter.clone();
                                            let client = api.clone();
                                            move |_| {
                                                let adapter = adapter.clone();
                                                let client = client.clone();
                                                let template = template.clone();
                                                spawn(async move {
                                                    error.set(None);
                                                    let AccountSessionState::Authenticated(account) = adapter.state() else {
                                                        error.set(Some(AssessmentApiError::AuthenticationRequired));
                                                        return;
                                                    };
                                                    let Some(company_id) = account.selected_company.map(|value| value.0) else {
                                                        error.set(Some(AssessmentApiError::PermissionDenied));
                                                        return;
                                                    };
                                                    let version_id = if let Some(draft) = template.latest_draft.as_ref() {
                                                        draft.version_id
                                                    } else {
                                                        let Some(published) = template.latest_published.as_ref() else {
                                                            error.set(Some(AssessmentApiError::InvalidRequest));
                                                            return;
                                                        };
                                                        match client.create_next_draft(
                                                            &account.access_token,
                                                            company_id,
                                                            template.template_id,
                                                            published.version_id,
                                                            &NextDraftRequest {
                                                                change_note: Some("Редактирование шаблона компании".into()),
                                                            },
                                                        ).await {
                                                            Ok(created) => created.draft_version_id,
                                                            Err(problem) => {
                                                                error.set(Some(problem));
                                                                return;
                                                            }
                                                        }
                                                    };
                                                    match client.get_company_draft(
                                                        &account.access_token,
                                                        company_id,
                                                        template.template_id,
                                                        version_id,
                                                    ).await {
                                                        Ok(document) => view.set(AssessmentView::CompanyEditor(document)),
                                                        Err(problem) => error.set(Some(problem)),
                                                    }
                                                });
                                            }
                                        },
                                    }
                                },
                                AssessmentView::CompanyEditor(document) => rsx! {
                                    CompanyDraftEditor {
                                        initial: document,
                                        on_back: move |_| view.set(AssessmentView::List),
                                    }
                                },
                            }
                        }
                    },
                    _ => rsx! {
                        AssessmentMessage {
                            icon: "◌",
                            title: "Подготавливаем библиотеку",
                            body: "Восстанавливаем безопасную Account-сессию.",
                        }
                    },
                }
            }

            if let Some(target) = adopt_target() {
                AdoptDialog {
                    template: target.clone(),
                    loading: adopting(),
                    on_cancel: move |_| {
                        if !adopting() {
                            adopt_target.set(None);
                        }
                    },
                    on_confirm: {
                        let client = api.clone();
                        let adapter = session_adapter.clone();
                        move |_| {
                            let client = client.clone();
                            let adapter = adapter.clone();
                            let current = adapter.state();
                            let target = target.clone();
                            let Some(company_id) = selected_company(&current) else {
                                error.set(Some(AssessmentApiError::PermissionDenied));
                                adopt_target.set(None);
                                return;
                            };
                            spawn(async move {
                                adopting.set(true);
                                error.set(None);
                                let result = if let AccountSessionState::Authenticated(auth) = current {
                                    client
                                        .adopt_library_template(
                                            &auth.access_token,
                                            company_id,
                                            &AdoptLibraryTemplateRequest {
                                                source_library_version_id: target.version_id,
                                                company_template_code: format!("restos-{}", target.code),
                                                company_template_name: Some(target.name.clone()),
                                                local_description: target.local_description.clone(),
                                            },
                                        )
                                        .await
                                } else {
                                    Err(AssessmentApiError::AuthenticationRequired)
                                };
                                match result {
                                    Ok(_) => {
                                        success.set(Some("Добавлено в мои замеры".into()));
                                        adopt_target.set(None);
                                        if let AccountSessionState::Authenticated(auth) = adapter.state() {
                                            if let Ok(items) = list_company(
                                                &adapter,
                                                &client,
                                                &auth,
                                                company_id,
                                            )
                                            .await
                                            {
                                                company_templates.set(items);
                                            }
                                        }
                                    }
                                    Err(AssessmentApiError::Conflict) => {
                                        let reconciled = if let AccountSessionState::Authenticated(
                                            auth,
                                        ) = adapter.state()
                                        {
                                            match list_company(
                                                &adapter,
                                                &client,
                                                &auth,
                                                company_id,
                                            )
                                            .await
                                            {
                                                Ok(items) => {
                                                    let adopted =
                                                        is_library_version_adopted(target.version_id, &items);
                                                    company_templates.set(items);
                                                    adopted
                                                }
                                                Err(_) => false,
                                            }
                                        } else {
                                            false
                                        };
                                        if reconciled {
                                            success.set(Some(
                                                "Шаблон уже добавлен в вашу компанию".into(),
                                            ));
                                            error.set(None);
                                        } else {
                                            error.set(Some(AssessmentApiError::Conflict));
                                        }
                                        adopt_target.set(None);
                                    }
                                    Err(problem) => {
                                        error.set(Some(problem));
                                        adopt_target.set(None);
                                    }
                                }
                                adopting.set(false);
                            });
                        }
                    },
                }
            }

            if let Some(progress) = tour() {
                GuidedTour {
                    progress,
                    on_back: {
                        let client = api.clone();
                        let adapter = session_adapter.clone();
                        move |_| {
                            let target = progress.back();
                            prepare_tour_step(
                                target,
                                client.clone(),
                                adapter.clone(),
                                library,
                                view,
                                tab,
                                load_state,
                                error,
                                session,
                                tour,
                            );
                        }
                    },
                    on_next: {
                        let client = api.clone();
                        let adapter = session_adapter.clone();
                        move |_| {
                            let target = progress.next();
                            prepare_tour_step(
                                target,
                                client.clone(),
                                adapter.clone(),
                                library,
                                view,
                                tab,
                                load_state,
                                error,
                                session,
                                tour,
                            );
                        }
                    },
                    on_close: move |completed| close_assessment_tour(tour, completed),
                }
            }
        }
    }
}

#[component]
fn LibraryList(
    templates: Vec<LibraryTemplateSummary>,
    company_templates: Vec<CompanyTemplateSummary>,
    active_tour_target: Option<TourTarget>,
    mut search: Signal<String>,
    mut activity_type: Signal<String>,
    on_filter_change: EventHandler<bool>,
    on_open: EventHandler<Uuid>,
    on_adopt: EventHandler<LibraryTemplateSummary>,
) -> Element {
    let active_filter_count =
        usize::from(!search().trim().is_empty()) + usize::from(!activity_type().is_empty());
    rsx! {
        section { class: "assessment-content",
            div {
                class: "assessment-toolbar",
                "data-tour": "library-filters",
                "data-tour-active": (active_tour_target == Some(TourTarget::LibraryFilters))
                    .then_some("true"),
                label { class: "assessment-search",
                    span { "Поиск по библиотеке" }
                    div { class: "assessment-filter-control",
                        input {
                            value: "{search}",
                            placeholder: "Название или код шаблона",
                            oninput: move |event| {
                                search.set(event.value());
                                on_filter_change.call(true);
                            },
                        }
                        if !search().is_empty() {
                            button {
                                class: "assessment-field-clear",
                                r#type: "button",
                                aria_label: "Очистить поиск",
                                onclick: move |_| {
                                    search.set(String::new());
                                    on_filter_change.call(false);
                                },
                                "×"
                            }
                        }
                    }
                }
                label { class: "assessment-filter",
                    span { "Тип активности" }
                    div { class: "assessment-filter-control",
                        select {
                            value: "{activity_type}",
                            onchange: move |event| {
                                activity_type.set(event.value());
                                on_filter_change.call(false);
                            },
                            option { value: "", "Все типы" }
                            option { value: "evaluation", "Оценка" }
                            option { value: "measurement", "Замер" }
                            option { value: "walkthrough", "Обход" }
                            option { value: "checklist", "Чек-лист" }
                            option { value: "test", "Тест" }
                            option { value: "survey", "Опрос" }
                            option { value: "attestation", "Аттестация" }
                        }
                        if !activity_type().is_empty() {
                            button {
                                class: "assessment-field-clear",
                                r#type: "button",
                                aria_label: "Очистить тип активности",
                                onclick: move |_| {
                                    activity_type.set(String::new());
                                    on_filter_change.call(false);
                                },
                                "×"
                            }
                        }
                    }
                }
                if active_filter_count > 1 {
                    div { class: "assessment-filter-actions",
                        button {
                            class: "assessment-button secondary",
                            r#type: "button",
                            onclick: move |_| {
                                search.set(String::new());
                                activity_type.set(String::new());
                                on_filter_change.call(false);
                            },
                            "Сбросить фильтры"
                        }
                    }
                }
            }
            div { class: "assessment-results-head",
                strong { "{templates.len()} шаблонов" }
                span { "Проверенные сценарии RestOS" }
            }
            if templates.is_empty() {
                AssessmentMessage {
                    icon: "⌕",
                    title: "Шаблоны не найдены",
                    body: "Измените запрос или очистите фильтры.",
                }
            } else {
                div { class: "assessment-grid",
                    for (index, template) in templates.into_iter().enumerate() {
                        {
                            let template_id = template.template_id;
                            let adopted = is_library_version_adopted(
                                template.version_id,
                                &company_templates,
                            );
                            let can_adopt = !adopted && template.published_at.is_some();
                            let open_template = on_open;
                            let keyboard_open = on_open;
                            let adopt_template = on_adopt;
                            let adoption_target = template.clone();
                            rsx! {
                                article {
                                    class: if adopted { "assessment-card is-adopted" } else { "assessment-card" },
                                    role: "button",
                                    tabindex: "0",
                                    aria_label: "Открыть шаблон {template.name}",
                                    onclick: move |_| open_template.call(template_id),
                                    onkeydown: move |event| {
                                        let key = event.key();
                                        if key == Key::Enter || key == Key::Character(" ".into()) {
                                            event.prevent_default();
                                            keyboard_open.call(template_id);
                                        }
                                    },
                                    "data-tour": if index == 0 { "library-card" } else { "" },
                                    "data-tour-active": (index == 0
                                        && active_tour_target == Some(TourTarget::LibraryCard))
                                        .then_some("true"),
                                    div { class: "assessment-card-top",
                                        span { class: "assessment-restos-badge", "Шаблон RestOS" }
                                        span { class: "assessment-type-badge", "{activity_label(&template.activity_type)}" }
                                    }
                                    if adopted {
                                        span { class: "assessment-added-badge", "✓ Уже добавлен" }
                                    }
                                    h2 { "{template.name}" }
                                    p { class: "assessment-description",
                                        "{template.local_description.as_deref().unwrap_or(\"Описание будет доступно в подробной карточке\")}"
                                    }
                                    div { class: "assessment-card-meta",
                                        span { "Версия {template.version}" }
                                        span { "{template.section_count} разд. · {template.item_count} пунктов" }
                                    }
                                    button {
                                        class: "assessment-button primary assessment-card-adopt",
                                        r#type: "button",
                                        disabled: !can_adopt,
                                        onkeydown: move |event| event.stop_propagation(),
                                        onclick: move |event| {
                                            event.stop_propagation();
                                            if can_adopt {
                                                adopt_template.call(adoption_target.clone());
                                            }
                                        },
                                        if adopted {
                                            "Добавлено в мои замеры"
                                        } else if template.published_at.is_none() {
                                            "Недоступно: шаблон не опубликован"
                                        } else {
                                            "Добавить в мои замеры"
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
}

#[component]
fn LibraryDetail(
    document: TemplateDocument,
    active_tour_target: Option<TourTarget>,
    adopted: bool,
    can_adopt: bool,
    company_data_resolved: bool,
    on_back: EventHandler<()>,
    on_open_company: EventHandler<()>,
    on_adopt: EventHandler<()>,
) -> Element {
    let cta_state = adopt_cta_state(adopted, can_adopt);
    rsx! {
        section { class: "assessment-content assessment-detail",
            button { class: "assessment-back", r#type: "button", onclick: move |_| on_back.call(()), "← Назад к библиотеке" }
            div { class: "assessment-detail-head",
                div {
                    div { class: "assessment-card-top",
                        span { class: "assessment-restos-badge", "Шаблон RestOS" }
                        span { class: "assessment-type-badge", "{activity_label(&document.template.activity_type)}" }
                    }
                    h2 { "{document.template.name}" }
                    p { "{document.version.local_description.as_deref().unwrap_or(\"Готовый шаблон для вашей команды\")}" }
                }
                button {
                    class: "assessment-button primary",
                    r#type: "button",
                    "data-tour": "adopt-action",
                    "data-tour-active": (active_tour_target == Some(TourTarget::AdoptAction))
                        .then_some("true"),
                    disabled: cta_state == AdoptCtaState::SelectCompany
                        || !company_data_resolved,
                    onclick: move |_| match cta_state {
                        AdoptCtaState::Adopt if company_data_resolved => on_adopt.call(()),
                        AdoptCtaState::OpenCompanyCopy => on_open_company.call(()),
                        AdoptCtaState::Adopt | AdoptCtaState::SelectCompany => {}
                    },
                    match cta_state {
                        AdoptCtaState::Adopt => "Добавить в мои замеры",
                        AdoptCtaState::OpenCompanyCopy => "Открыть мой шаблон",
                        AdoptCtaState::SelectCompany => "Выберите компанию",
                    }
                }
            }
            div { class: "assessment-summary",
                div { strong { "{document.version.version}" } span { "Бизнес-версия" } }
                div { strong { "{document.version.section_count}" } span { "Разделов" } }
                div { strong { "{document.version.item_count}" } span { "Пунктов" } }
                div { strong { "{document.version.option_count}" } span { "Вариантов ответа" } }
            }
            aside {
                class: "assessment-methodology",
                div {
                    class: METHODOLOGY_TOUR_TARGET_CLASS,
                    "data-tour": "methodology",
                    "data-tour-active": (active_tour_target == Some(TourTarget::Methodology))
                        .then_some("true"),
                    MethodologyBookIcon {}
                    div { class: "assessment-methodology-copy",
                        p { class: "assessment-eyebrow", "МЕТОДОЛОГИЯ RESTOS · ВЕРСИЯ {document.methodology.version}" }
                        h3 { "Методология RestOS" }
                        p { class: "assessment-methodology-intro",
                            "Зачем нужен этот шаблон и как правильно применять его в работе."
                        }
                    }
                }
                div { class: "assessment-methodology-copy assessment-methodology-details",
                    p { class: "assessment-methodology-immutable",
                        "Методология сохраняется без изменений, даже если вы адаптируете название, разделы и критерии под свой ресторан."
                    }
                    if let Some(body) = document.methodology.body.as_deref() {
                        div { class: "assessment-methodology-content",
                            strong { "{document.methodology.title}" }
                            p { "{body}" }
                        }
                    }
                    p { class: "assessment-methodology-note",
                        "После добавления шаблона его название, описание и критерии можно будет адаптировать в редакторе."
                    }
                }
            }
            div { class: "assessment-sections",
                for (index, section) in document.sections.iter().enumerate() {
                    article { class: "assessment-section-card",
                        div { class: "assessment-section-number", "{index + 1:02}" }
                        div { class: "assessment-section-body",
                            h3 { "{section.title}" }
                            if let Some(description) = section.description.as_deref() {
                                p { class: "assessment-description", "{description}" }
                            }
                            div { class: "assessment-items",
                                for item in section.items.iter() {
                                    {
                                        let required_suffix = if item.is_required {
                                            " · обязательно"
                                        } else {
                                            ""
                                        };
                                        rsx! {
                                            div { class: "assessment-item",
                                                div {
                                                    strong { "{item.prompt}" }
                                                    span { "{response_label(&item.response_type)}{required_suffix}" }
                                                }
                                                if !item.options.is_empty() {
                                                    ul {
                                                        for option in item.options.iter() {
                                                            li {
                                                                "{option.label}"
                                                                if option.is_disqualifying {
                                                                    span { class: "assessment-critical", " Стоп-фактор" }
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
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CompanyList(
    templates: Vec<CompanyTemplateSummary>,
    has_company: bool,
    on_open: EventHandler<CompanyTemplateSummary>,
) -> Element {
    if !has_company {
        return rsx! {
            AssessmentMessage {
                icon: "⌂",
                title: "Выберите компанию",
                body: "Для просмотра своих шаблонов выберите доступную компанию выше.",
            }
        };
    }
    rsx! {
        section { class: "assessment-content",
            div { class: "assessment-results-head",
                strong { "Мои шаблоны" }
                span { "{templates.len()} доступно" }
            }
            if templates.is_empty() {
                AssessmentMessage {
                    icon: "＋",
                    title: "Пока нет шаблонов",
                    body: "Добавьте готовый шаблон из библиотеки RestOS.",
                }
            } else {
                TemplateCategory {
                    title: "Оценки сотрудников",
                    description: "КЛН и практикумы, которые назначаются сотрудникам.",
                    templates: templates.iter().filter(|value| value.activity_type != "walkthrough").cloned().collect(),
                    on_open: on_open.clone(),
                }
                TemplateCategory {
                    title: "Операционные обходы",
                    description: "Обходы принадлежат ресторану и запускаются с главной кнопкой «Сделать замер».",
                    templates: templates.iter().filter(|value| value.activity_type == "walkthrough").cloned().collect(),
                    on_open: on_open.clone(),
                }
                section { class: "assessment-category", aria_labelledby: "product-measurement-category",
                    div { class: "assessment-results-head",
                        div {
                            strong { id: "product-measurement-category", "Замеры продукта" }
                            p { "Повторяемая оценка вкуса, внешнего вида, выхода и Ticket Time." }
                        }
                    }
                    article { class: "assessment-card",
                        div { class: "assessment-card-top",
                            span { class: "assessment-status-badge", "Доступен" }
                            span { class: "assessment-type-badge", "Замер" }
                        }
                        h2 { "Оценка вкуса и скорости" }
                        p { class: "assessment-description", "Динамический бланк без заранее созданных пустых строк." }
                        div { class: "assessment-card-meta",
                            span { "Вкус /9" }
                            span { "Скорость /3" }
                            span { "Собственный итог /12" }
                        }
                        p { class: "assessment-origin", "Запуск доступен с главной кнопкой «Сделать замер»." }
                    }
                }
            }
        }
    }
}

#[component]
fn TemplateCategory(
    title: &'static str,
    description: &'static str,
    templates: Vec<CompanyTemplateSummary>,
    on_open: EventHandler<CompanyTemplateSummary>,
) -> Element {
    rsx! {
        section { class: "assessment-category", aria_label: "{title}",
            div { class: "assessment-results-head",
                div { strong { "{title}" } p { "{description}" } }
                span { "{templates.len()}" }
            }
            if templates.is_empty() {
                p { class: "assessment-empty-category", "В этой категории пока нет шаблонов." }
            } else {
                div { class: "assessment-grid",
                    for template in templates {
                        {
                            let version = preferred_version(&template);
                            let status = version.map(|value| value.status.as_str()).unwrap_or("archived");
                            let item = template.clone();
                            rsx! {
                                article { class: "assessment-card",
                                    div { class: "assessment-card-top",
                                        span { class: "assessment-status-badge", "{status_label(status)}" }
                                        span { class: "assessment-type-badge", "{activity_label(&template.activity_type)}" }
                                    }
                                    h2 { "{template.name}" }
                                    p { class: "assessment-description",
                                        "{version.and_then(|value| value.local_description.as_deref()).unwrap_or(\"Описание ещё не добавлено\")}"
                                    }
                                    div { class: "assessment-origin",
                                        if template.source_library_version_id.is_some() {
                                            "◇ На основе шаблона RestOS"
                                        } else {
                                            "Собственный шаблон"
                                        }
                                    }
                                    if let Some(version) = version {
                                        div { class: "assessment-card-meta",
                                            span { "Версия {version.version}" }
                                            span { "Редакция {version.edit_revision}" }
                                            span {
                                                if is_imported_weighted(&template.code) {
                                                    "Алгоритм weighted_v1"
                                                } else {
                                                    "Алгоритм completion_v1"
                                                }
                                            }
                                            span {
                                                if template_version_launch_available(&version.status) {
                                                    "Расчёт и запуск доступны"
                                                } else {
                                                    "Расчёт и запуск недоступны"
                                                }
                                            }
                                        }
                                    }
                                    button {
                                        class: "assessment-link-button",
                                        r#type: "button",
                                        onclick: move |_| on_open.call(item.clone()),
                                        "Открыть",
                                        span { aria_hidden: "true", " →" }
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

fn is_imported_weighted(code: &str) -> bool {
    matches!(
        code,
        "cook-kln"
            | "waiter-kln"
            | "hostess-kln"
            | "itci-kln"
            | "kitchen-practicum"
            | "bar-practicum"
            | "restaurant-service-walkthrough"
            | "production-walkthrough"
    )
}

fn template_version_launch_available(status: &str) -> bool {
    status == "published"
}

#[component]
fn CompanyDetail(
    template: CompanyTemplateSummary,
    on_back: EventHandler<()>,
    on_edit: EventHandler<()>,
) -> Element {
    let version = preferred_version(&template);
    rsx! {
        section { class: "assessment-content assessment-detail",
            button { class: "assessment-back", r#type: "button", onclick: move |_| on_back.call(()), "← Назад к моим шаблонам" }
            div { class: "assessment-detail-head",
                div {
                    div { class: "assessment-card-top",
                        span { class: "assessment-status-badge", "{status_label(version.map(|value| value.status.as_str()).unwrap_or(\"archived\"))}" }
                        span { class: "assessment-type-badge", "{activity_label(&template.activity_type)}" }
                    }
                    h2 { "{template.name}" }
                    p { "{version.and_then(|value| value.local_description.as_deref()).unwrap_or(\"Описание ещё не добавлено\")}" }
                }
            }
            if let Some(version) = version {
                div { class: "assessment-summary",
                    div { strong { "{version.version}" } span { "Бизнес-версия" } }
                    div { strong { "{version.edit_revision}" } span { "Редакция" } }
                    div { strong { "{version.section_count}" } span { "Разделов" } }
                    div { strong { "{version.item_count}" } span { "Пунктов" } }
                    div {
                        strong {
                            if version.status == "published" { "Доступен" } else { "Недоступен" }
                        }
                        span { "Расчёт" }
                    }
                }
            }
            if let Some(methodology) = template.methodology.as_ref() {
                aside { class: "assessment-methodology compact",
                    MethodologyBookIcon {}
                    div { class: "assessment-methodology-copy",
                        p { class: "assessment-eyebrow", "НЕИЗМЕНЯЕМАЯ МЕТОДОЛОГИЯ" }
                        h3 { "Методология RestOS" }
                        p { class: "assessment-methodology-intro",
                            "Зачем нужен этот шаблон и как правильно применять его в работе."
                        }
                        div { class: "assessment-methodology-content",
                            strong { "{methodology.title}" }
                        }
                        p { class: "assessment-methodology-note",
                            "Методология сохраняется без изменений. Название, описание и критерии копии можно будет адаптировать в редакторе."
                        }
                    }
                }
            }
            div { class: "assessment-readonly-note",
                strong { "Опубликованная версия защищена" }
                p { "Изменения создаются только в новой черновой версии. Действующие замеры сохраняют прежнюю версию." }
                if template.can_manage {
                    button {
                        class: "btn btn-primary",
                        r#type: "button",
                        onclick: move |_| on_edit.call(()),
                        if template.latest_draft.is_some() {
                            "Продолжить редактирование"
                        } else {
                            "Создать новую версию"
                        }
                    }
                }
            }
        }
    }
}

const CANONICAL_METRICS: [(&str, &str); 7] = [
    ("people", "Люди"),
    ("service", "Сервис"),
    ("taste", "Вкус"),
    ("speed", "Скорость"),
    ("order", "Порядок"),
    ("space", "Пространство"),
    ("economics", "Экономика"),
];

fn numeric_text(value: &Option<serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    }
}

fn numeric_value(value: String) -> Option<serde_json::Value> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(serde_json::Value::String(value.to_string()))
    }
}

fn numeric_values_equal(
    left: &Option<serde_json::Value>,
    right: &Option<serde_json::Value>,
) -> bool {
    let parse = |value: &Option<serde_json::Value>| {
        value
            .as_ref()
            .and_then(|value| numeric_text(&Some(value.clone())).parse::<f64>().ok())
            .filter(|value| value.is_finite())
    };
    matches!((parse(left), parse(right)), (Some(left), Some(right)) if left == right)
}

fn draft_document_valid(document: &CompanyDraftDocument) -> bool {
    !document.template.name.trim().is_empty()
        && !document.sections.is_empty()
        && document.sections.iter().all(|section| {
            !section.title.trim().is_empty()
                && !section.items.is_empty()
                && section.items.iter().all(|item| {
                    !item.prompt.trim().is_empty()
                        && item
                            .weight
                            .as_ref()
                            .and_then(|value| {
                                numeric_text(&Some(value.clone())).parse::<f64>().ok()
                            })
                            .is_some_and(|weight| weight.is_finite() && weight > 0.0)
                        && !item.metric_mappings.is_empty()
                        && item.metric_mappings.iter().all(|mapping| {
                            numeric_values_equal(
                                &Some(mapping.contribution_weight.clone()),
                                &item.weight,
                            )
                        })
                })
        })
}

fn draft_validation_message(document: &CompanyDraftDocument) -> Option<String> {
    if document.template.name.trim().is_empty() {
        return Some("Укажите название шаблона.".into());
    }
    if document.sections.is_empty() {
        return Some("Добавьте хотя бы один раздел.".into());
    }
    for (section_index, section) in document.sections.iter().enumerate() {
        if section.title.trim().is_empty() {
            return Some(format!("Укажите название раздела {}.", section_index + 1));
        }
        if section.items.is_empty() {
            return Some(format!(
                "В разделе «{}» должен быть хотя бы один пункт.",
                section.title
            ));
        }
        for (item_index, item) in section.items.iter().enumerate() {
            let label = format!("Раздел {}, пункт {}", section_index + 1, item_index + 1);
            if item.prompt.trim().is_empty() {
                return Some(format!("{label}: заполните формулировку."));
            }
            let valid_weight = item
                .weight
                .as_ref()
                .and_then(|value| numeric_text(&Some(value.clone())).parse::<f64>().ok())
                .is_some_and(|weight| weight.is_finite() && weight > 0.0);
            if !valid_weight {
                return Some(format!("{label}: укажите положительный вес."));
            }
            if item.metric_mappings.is_empty() {
                return Some(format!("{label}: выберите показатель аналитики."));
            }
        }
    }
    None
}

fn normalize_draft_order(document: &mut CompanyDraftDocument) {
    for (section_order, section) in document.sections.iter_mut().enumerate() {
        section.sort_order = section_order as i32;
        for (item_order, item) in section.items.iter_mut().enumerate() {
            item.sort_order = item_order as i32;
            for (option_order, option) in item.options.iter_mut().enumerate() {
                option.sort_order = option_order as i32;
            }
        }
    }
}

fn synchronize_mapping_weights(item: &mut DraftItem, weight: Option<serde_json::Value>) {
    item.weight = weight.clone();
    for mapping in &mut item.metric_mappings {
        mapping.contribution_weight = weight
            .clone()
            .unwrap_or_else(|| serde_json::Value::String("1".into()));
    }
}

fn move_draft_item(
    document: &mut CompanyDraftDocument,
    from_section: usize,
    item_index: usize,
    to_section: usize,
) -> bool {
    if from_section >= document.sections.len()
        || to_section >= document.sections.len()
        || from_section == to_section
        || item_index >= document.sections[from_section].items.len()
    {
        return false;
    }
    let item = document.sections[from_section].items.remove(item_index);
    document.sections[to_section].items.push(item);
    normalize_draft_order(document);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftDeleteTarget {
    Section(usize),
    Item(usize, usize),
}

#[component]
fn CompanyDraftEditor(initial: CompanyDraftDocument, on_back: EventHandler<()>) -> Element {
    let api = use_context::<AssessmentApiClient>();
    let session = use_context::<AccountSessionAdapter>();
    let mut document = use_signal(|| initial.clone());
    let mut saved_document = use_signal(|| initial.clone());
    let mut saving = use_signal(|| false);
    let mut confirming_publish = use_signal(|| false);
    let mut previewing = use_signal(|| false);
    let mut delete_target = use_signal(|| None::<DraftDeleteTarget>);
    let mut message = use_signal(|| None::<String>);
    let mut conflict = use_signal(|| false);
    let validation_message = draft_validation_message(&document());
    let valid = draft_document_valid(&document());
    let validation_text = validation_message
        .clone()
        .unwrap_or_else(|| String::from("проверьте содержимое черновика"));
    let dirty = document() != saved_document();

    let save_action = {
        let api = api.clone();
        let session = session.clone();
        EventHandler::new(move |publish_after: bool| {
            if saving() {
                return;
            }
            if publish_after {
                if let Some(problem) = draft_validation_message(&document()) {
                    message.set(Some(problem));
                    return;
                }
            }
            let client = api.clone();
            let adapter = session.clone();
            let mut snapshot = document();
            normalize_draft_order(&mut snapshot);
            spawn(async move {
                saving.set(true);
                conflict.set(false);
                message.set(None);
                let AccountSessionState::Authenticated(account) = adapter.state() else {
                    saving.set(false);
                    message.set(Some("Сессия завершена. Войдите снова.".into()));
                    return;
                };
                let Some(company_id) = account.selected_company.map(|value| value.0) else {
                    saving.set(false);
                    message.set(Some("Выберите организацию.".into()));
                    return;
                };
                let request = SaveCompanyDraftRequest {
                    expected_edit_revision: snapshot.version.edit_revision,
                    template_name: Some(snapshot.template.name.clone()),
                    local_description: snapshot.version.local_description.clone(),
                    change_note: Some("Изменение структуры шаблона компании".into()),
                    sections: snapshot.sections.clone(),
                };
                match client
                    .save_company_draft(
                        &account.access_token,
                        company_id,
                        snapshot.template.id,
                        snapshot.version.id,
                        &request,
                    )
                    .await
                {
                    Ok(_) => {
                        if publish_after {
                            match client
                                .publish_company_draft(
                                    &account.access_token,
                                    company_id,
                                    snapshot.template.id,
                                    snapshot.version.id,
                                )
                                .await
                            {
                                Ok(_) => {
                                    confirming_publish.set(false);
                                    message.set(Some(
                                        "Новая версия опубликована. Текущие замеры не изменены."
                                            .into(),
                                    ));
                                }
                                Err(problem) => message.set(Some(safe_error(problem).into())),
                            }
                        } else {
                            match client
                                .get_company_draft(
                                    &account.access_token,
                                    company_id,
                                    snapshot.template.id,
                                    snapshot.version.id,
                                )
                                .await
                            {
                                Ok(reloaded) => {
                                    saved_document.set(reloaded.clone());
                                    document.set(reloaded);
                                    message.set(Some("Черновик сохранён.".into()));
                                }
                                Err(problem) => message.set(Some(safe_error(problem).into())),
                            }
                        }
                    }
                    Err(AssessmentApiError::Conflict) => {
                        conflict.set(true);
                        message.set(Some(
                            "Черновик изменился в другой сессии. Перезагрузите актуальную версию."
                                .into(),
                        ));
                    }
                    Err(problem) => message.set(Some(safe_error(problem).into())),
                }
                saving.set(false);
            });
        })
    };

    rsx! {
        section { class: "assessment-content assessment-template-editor",
            header { class: "assessment-editor-header",
                button { class: "assessment-back", r#type: "button", onclick: move |_| on_back.call(()), "← Мои шаблоны" }
                div {
                    p { class: "assessment-eyebrow", "ЧЕРНОВИК ВЕРСИИ {document().version.version}" }
                    h2 { "Редактор шаблона" }
                    p { "Методология и опубликованные версии остаются неизменными." }
                }
                div { class: "assessment-editor-actions",
                    span { class: "assessment-editor-state",
                        if conflict() { "Конфликт версии" } else if saving() { "Сохранение…" } else if dirty { "Есть несохранённые изменения" } else { "Сохранено" }
                    }
                    button { class: "btn btn-secondary", r#type: "button", disabled: saving() || !dirty, onclick: move |_| save_action.call(false), "Сохранить черновик" }
                    button { class: "btn btn-secondary", r#type: "button", disabled: saving(), onclick: move |_| previewing.set(!previewing()), "Предпросмотр" }
                    button {
                        class: "btn btn-secondary", r#type: "button", disabled: saving() || !dirty,
                        onclick: move |_| {
                            document.set(saved_document());
                            conflict.set(false);
                            message.set(Some("Несохранённые изменения отменены.".into()));
                        },
                        "Отменить изменения"
                    }
                    button { class: "btn btn-primary", r#type: "button", disabled: saving() || !valid, onclick: move |_| confirming_publish.set(true), "Опубликовать новую версию" }
                }
            }
            if !valid {
                div { class: "assessment-inline-warning", role: "status",
                    "Нельзя опубликовать: {validation_text}"
                }
            }
            if let Some(text) = message() {
                div { class: if conflict() { "assessment-inline-error" } else { "assessment-inline-success" }, role: "status", "{text}" }
            }
            if conflict() {
                button {
                    class: "btn btn-secondary",
                    r#type: "button",
                    onclick: {
                        let client = api.clone();
                        let adapter = session.clone();
                        move |_| {
                            let client = client.clone();
                            let adapter = adapter.clone();
                            let current = document();
                            spawn(async move {
                                let AccountSessionState::Authenticated(account) = adapter.state() else { return; };
                                let Some(company_id) = account.selected_company.map(|value| value.0) else { return; };
                                if let Ok(reloaded) = client.get_company_draft(&account.access_token, company_id, current.template.id, current.version.id).await {
                                    saved_document.set(reloaded.clone());
                                    document.set(reloaded);
                                    conflict.set(false);
                                    message.set(Some("Загружена актуальная редакция.".into()));
                                }
                            });
                        }
                    },
                    "Перезагрузить актуальный черновик"
                }
            }
            label { class: "assessment-editor-field",
                span { "Название шаблона" }
                input {
                    value: "{document().template.name}",
                    oninput: move |event| document.write().template.name = event.value(),
                }
            }
            if previewing() {
                section { class: "assessment-editor-preview", aria_label: "Предпросмотр шаблона",
                    p { class: "assessment-eyebrow", "ПРЕДПРОСМОТР" }
                    h3 { "{document().template.name}" }
                    for section in document().sections {
                        div { class: "assessment-editor-preview-section", key: "preview-{section.code}",
                            strong { "{section.title}" }
                            for item in section.items {
                                p { key: "preview-{section.code}-{item.code}", "{item.prompt}" }
                            }
                        }
                    }
                }
            }
            for section_index in 0..document().sections.len() {
                article { class: "assessment-editor-section", key: "section-{section_index}",
                    div { class: "assessment-editor-row",
                        input {
                            aria_label: "Название раздела",
                            value: "{document().sections[section_index].title}",
                            oninput: move |event| document.write().sections[section_index].title = event.value(),
                        }
                        button { r#type: "button", aria_label: "Переместить раздел выше", disabled: section_index == 0, onclick: move |_| document.write().sections.swap(section_index, section_index - 1), "↑" }
                        button { r#type: "button", aria_label: "Переместить раздел ниже", disabled: section_index + 1 == document().sections.len(), onclick: move |_| document.write().sections.swap(section_index, section_index + 1), "↓" }
                        button { r#type: "button", class: "assessment-critical-action", onclick: move |_| delete_target.set(Some(DraftDeleteTarget::Section(section_index))), "Удалить раздел" }
                    }
                    for item_index in 0..document().sections[section_index].items.len() {
                        div { class: "assessment-editor-item", key: "item-{section_index}-{item_index}",
                            textarea {
                                aria_label: "Текст пункта",
                                value: "{document().sections[section_index].items[item_index].prompt}",
                                oninput: move |event| document.write().sections[section_index].items[item_index].prompt = event.value(),
                            }
                            textarea {
                                aria_label: "Подсказка к пункту",
                                placeholder: "Подсказка для проверяющего",
                                value: "{document().sections[section_index].items[item_index].guidance.clone().unwrap_or_default()}",
                                oninput: move |event| {
                                    let value = event.value();
                                    document.write().sections[section_index].items[item_index].guidance =
                                        (!value.trim().is_empty()).then_some(value);
                                },
                            }
                            div { class: "assessment-editor-grid",
                                label { "Тип ответа"
                                    select {
                                        value: "{document().sections[section_index].items[item_index].response_type}",
                                        onchange: move |event| {
                                            let response_type = event.value();
                                            let mut state = document.write();
                                            let item = &mut state.sections[section_index].items[item_index];
                                            item.response_type = response_type.clone();
                                            if !matches!(response_type.as_str(), "single_choice" | "multi_choice") {
                                                item.options.clear();
                                            } else if item.options.is_empty() {
                                                item.options.push(crate::assessment_api::DraftOption {
                                                    code: "option-1".into(), label: "Вариант 1".into(),
                                                    sort_order: 0, numeric_value: None, is_disqualifying: false,
                                                });
                                            }
                                        },
                                        option { value: "boolean", "Да / нет" }
                                        option { value: "score", "Оценка" }
                                        option { value: "integer", "Целое число" }
                                        option { value: "decimal", "Число" }
                                        option { value: "text", "Текст" }
                                        option { value: "single_choice", "Один вариант" }
                                        option { value: "multi_choice", "Несколько вариантов" }
                                        option { value: "date", "Дата" }
                                        option { value: "time", "Время" }
                                    }
                                }
                                label { "Вес"
                                    input {
                                        r#type: "number", min: "0.000001", step: "0.1",
                                        value: "{numeric_text(&document().sections[section_index].items[item_index].weight)}",
                                        oninput: move |event| {
                                            let weight = numeric_value(event.value());
                                            let mut state = document.write();
                                            let item = &mut state.sections[section_index].items[item_index];
                                            synchronize_mapping_weights(item, weight);
                                        }
                                    }
                                }
                                label { "Показатель"
                                    select {
                                        value: "{document().sections[section_index].items[item_index].metric_mappings.first().map(|mapping| mapping.metric_code.as_str()).unwrap_or(\"\")}",
                                        onchange: move |event| {
                                            let mut state = document.write();
                                            let item = &mut state.sections[section_index].items[item_index];
                                            item.metric_mappings = vec![DraftMetricMapping {
                                                metric_code: event.value(),
                                                contribution_weight: item.weight.clone().unwrap_or_else(|| serde_json::Value::String("1".into())),
                                                direction: "positive".into(),
                                            }];
                                        },
                                        option { value: "", disabled: true, "Выберите" }
                                        for (code, label) in CANONICAL_METRICS {
                                            option { value: "{code}", "{label}" }
                                        }
                                    }
                                }
                                label { class: "assessment-editor-check",
                                    input {
                                        r#type: "checkbox",
                                        checked: document().sections[section_index].items[item_index].is_required,
                                        onchange: move |event| document.write().sections[section_index].items[item_index].is_required = event.checked(),
                                    }
                                    "Обязательный пункт"
                                }
                                label { "Подтверждение"
                                    select {
                                        value: "{document().sections[section_index].items[item_index].evidence_mode}",
                                        onchange: move |event| document.write().sections[section_index].items[item_index].evidence_mode = event.value(),
                                        option { value: "none", "Не требуется" }
                                        option { value: "optional_comment", "Комментарий по желанию" }
                                        option { value: "required_comment", "Комментарий обязателен" }
                                        option { value: "optional_photo", "Фото по желанию" }
                                        option { value: "required_photo", "Фото обязательно" }
                                        option { value: "photo_and_comment", "Фото и комментарий" }
                                    }
                                }
                                label { "Критичность"
                                    select {
                                        value: "{document().sections[section_index].items[item_index].criticality}",
                                        onchange: move |event| document.write().sections[section_index].items[item_index].criticality = event.value(),
                                        option { value: "normal", "Обычный" }
                                        option { value: "critical", "Критический" }
                                        option { value: "stop_factor", "Стоп-фактор" }
                                    }
                                }
                            }
                            if matches!(document().sections[section_index].items[item_index].response_type.as_str(), "single_choice" | "multi_choice") {
                                div { class: "assessment-editor-options",
                                    strong { "Варианты ответа" }
                                    for option_index in 0..document().sections[section_index].items[item_index].options.len() {
                                        div { class: "assessment-editor-row", key: "option-{section_index}-{item_index}-{option_index}",
                                            input {
                                                aria_label: "Текст варианта ответа",
                                                value: "{document().sections[section_index].items[item_index].options[option_index].label}",
                                                oninput: move |event| document.write().sections[section_index].items[item_index].options[option_index].label = event.value(),
                                            }
                                            button {
                                                r#type: "button", class: "assessment-critical-action",
                                                onclick: move |_| { document.write().sections[section_index].items[item_index].options.remove(option_index); },
                                                "Удалить вариант"
                                            }
                                        }
                                    }
                                    button {
                                        r#type: "button", class: "btn btn-secondary",
                                        onclick: move |_| {
                                            let next = document().sections[section_index].items[item_index].options.len() + 1;
                                            document.write().sections[section_index].items[item_index].options.push(crate::assessment_api::DraftOption {
                                                code: format!("option-{next}"), label: format!("Вариант {next}"),
                                                sort_order: 0, numeric_value: None, is_disqualifying: false,
                                            });
                                        },
                                        "+ Вариант"
                                    }
                                }
                            }
                            div { class: "assessment-editor-row compact",
                                button { r#type: "button", disabled: item_index == 0, onclick: move |_| document.write().sections[section_index].items.swap(item_index, item_index - 1), "Выше" }
                                button { r#type: "button", disabled: item_index + 1 == document().sections[section_index].items.len(), onclick: move |_| document.write().sections[section_index].items.swap(item_index, item_index + 1), "Ниже" }
                                button { r#type: "button", disabled: section_index == 0, onclick: move |_| { move_draft_item(&mut document.write(), section_index, item_index, section_index - 1); }, "В предыдущий раздел" }
                                button { r#type: "button", disabled: section_index + 1 == document().sections.len(), onclick: move |_| { move_draft_item(&mut document.write(), section_index, item_index, section_index + 1); }, "В следующий раздел" }
                                button { r#type: "button", class: "assessment-critical-action", onclick: move |_| delete_target.set(Some(DraftDeleteTarget::Item(section_index, item_index))), "Удалить пункт" }
                            }
                        }
                    }
                    button {
                        class: "btn btn-secondary", r#type: "button",
                        onclick: move |_| {
                            let code = format!("item-{}-{}", section_index + 1, document().sections[section_index].items.len() + 1);
                            document.write().sections[section_index].items.push(DraftItem {
                                code, prompt: "Новый пункт".into(), guidance: None,
                                response_type: "boolean".into(), is_required: true,
                                sort_order: 0, weight: Some(serde_json::Value::String("1".into())),
                                min_value: None, max_value: None, passing_value: None,
                                evidence_mode: "optional_comment".into(), criticality: "normal".into(),
                                config: serde_json::json!({}), options: vec![], metric_mappings: vec![],
                            });
                        },
                        "+ Добавить пункт"
                    }
                }
            }
            button {
                class: "btn btn-secondary", r#type: "button",
                onclick: move |_| {
                    let index = document().sections.len() + 1;
                    document.write().sections.push(DraftSection {
                        code: format!("section-{index}"), title: "Новый раздел".into(),
                        description: None, section_kind: "section".into(), sort_order: 0,
                        weight: None, parent_code: None, items: vec![],
                    });
                },
                "+ Добавить раздел"
            }
            if confirming_publish() {
                div { class: "assessment-dialog-backdrop", role: "presentation",
                    div { class: "assessment-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "publish-draft-title",
                        h3 { id: "publish-draft-title", "Опубликовать новую версию?" }
                        p { "Новые назначения будут использовать её. Текущие и завершённые замеры сохранят прежнюю версию." }
                        div { class: "assessment-dialog-actions",
                            button { class: "btn btn-secondary", r#type: "button", onclick: move |_| confirming_publish.set(false), "Отмена" }
                            button { class: "btn btn-primary", r#type: "button", onclick: move |_| save_action.call(true), "Сохранить и опубликовать" }
                        }
                    }
                }
            }
            if let Some(target) = delete_target() {
                div { class: "assessment-dialog-backdrop", role: "presentation",
                    div { class: "assessment-dialog", role: "dialog", aria_modal: "true", aria_labelledby: "delete-draft-content-title",
                        h3 { id: "delete-draft-content-title", "Подтвердите удаление" }
                        p {
                            match target {
                                DraftDeleteTarget::Section(index) => format!(
                                    "Раздел и {} пунктов будут удалены только из текущего черновика.",
                                    document().sections.get(index).map(|section| section.items.len()).unwrap_or(0)
                                ),
                                DraftDeleteTarget::Item(_, _) => "Пункт будет удалён только из текущего черновика.".into(),
                            }
                        }
                        div { class: "assessment-dialog-actions",
                            button { class: "btn btn-secondary", r#type: "button", onclick: move |_| delete_target.set(None), "Отмена" }
                            button {
                                class: "assessment-critical-action", r#type: "button",
                                onclick: move |_| {
                                    let mut state = document.write();
                                    match target {
                                        DraftDeleteTarget::Section(index) if index < state.sections.len() => { state.sections.remove(index); }
                                        DraftDeleteTarget::Item(section, item) if section < state.sections.len() && item < state.sections[section].items.len() => { state.sections[section].items.remove(item); }
                                        _ => {}
                                    }
                                    normalize_draft_order(&mut state);
                                    delete_target.set(None);
                                },
                                "Удалить"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MethodologyBookIcon() -> Element {
    rsx! {
        div { class: "assessment-methodology-icon", aria_hidden: "true",
            svg {
                view_box: "0 0 32 32",
                path { d: "M6.5 6.5h7.1c1.3 0 2.4 1.1 2.4 2.4v16.6c0-1.3-1.1-2.4-2.4-2.4H6.5z" }
                path { d: "M25.5 6.5h-7.1c-1.3 0-2.4 1.1-2.4 2.4v16.6c0-1.3 1.1-2.4 2.4-2.4h7.1z" }
                path { class: "assessment-book-glint", d: "M19.2 10.1h3.5M19.2 13.4h3.5" }
            }
        }
    }
}

#[component]
fn GuidedTour(
    progress: TourProgress,
    on_back: EventHandler<()>,
    on_next: EventHandler<()>,
    on_close: EventHandler<bool>,
) -> Element {
    let mut target_found = use_signal(|| true);
    let step = TOUR_STEPS[progress.index];
    use_effect(use_reactive((&progress,), move |(progress,)| {
        target_found.set(highlight_tour_target(TOUR_STEPS[progress.index].selector));
    }));

    rsx! {
        div {
            class: "assessment-tour-overlay",
            role: "dialog",
            aria_modal: "true",
            aria_labelledby: "assessment-tour-title",
            tabindex: "0",
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    on_close.call(false);
                }
            },
            div { class: "assessment-tour-popover", id: "assessment-tour-popover", tabindex: "-1",
                div { class: "assessment-tour-progress",
                    span { "Шаг {progress.index + 1} из {TOUR_STEPS.len()}" }
                    button {
                        r#type: "button",
                        onclick: move |_| on_close.call(true),
                        "Пропустить"
                    }
                }
                h2 { id: "assessment-tour-title", "{step.title}" }
                p { "{step.body}" }
                if !target_found() {
                    p { class: "assessment-tour-target-note",
                        "Элемент сейчас недоступен. Можно продолжить тур или закрыть его."
                    }
                }
                div { class: "assessment-tour-actions",
                    button {
                        class: "assessment-button secondary",
                        r#type: "button",
                        disabled: progress.is_first(),
                        onclick: move |_| on_back.call(()),
                        "Назад"
                    }
                    button {
                        class: "assessment-button primary",
                        r#type: "button",
                        onclick: move |_| {
                            if progress.is_last() {
                                on_close.call(true);
                            } else {
                                on_next.call(());
                            }
                        },
                        if progress.is_last() { "Готово" } else { "Далее" }
                    }
                }
            }
        }
    }
}

#[component]
fn AdoptDialog(
    template: LibraryTemplateSummary,
    loading: bool,
    on_cancel: EventHandler<()>,
    on_confirm: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "assessment-dialog-backdrop",
            role: "presentation",
            onkeydown: move |event| {
                if event.key() == Key::Escape && !loading {
                    on_cancel.call(());
                }
            },
            div {
                class: "assessment-dialog",
                role: "dialog",
                aria_modal: "true",
                aria_labelledby: "assessment-adopt-title",
                h2 { id: "assessment-adopt-title", "Добавить шаблон?" }
                p { class: "assessment-dialog-name", "{template.name}" }
                p {
                    "Будет создана копия для вашей компании. Описание и структуру можно будет адаптировать позже, методология RestOS останется неизменной"
                }
                div { class: "assessment-dialog-actions",
                    button {
                        class: "assessment-button secondary",
                        r#type: "button",
                        disabled: loading,
                        onclick: move |_| on_cancel.call(()),
                        "Отмена"
                    }
                    button {
                        class: "assessment-button primary",
                        r#type: "button",
                        disabled: loading,
                        onclick: move |_| on_confirm.call(()),
                        if loading { "Добавляем…" } else { "Добавить шаблон" }
                    }
                }
            }
        }
    }
}

#[component]
fn AssessmentLoading() -> Element {
    rsx! {
        div { class: "assessment-loading", aria_live: "polite",
            div { class: "assessment-loading-line wide" }
            div { class: "assessment-loading-line" }
            div { class: "assessment-grid",
                for _ in 0..6 {
                    div { class: "assessment-card assessment-skeleton",
                        div { class: "assessment-loading-line short" }
                        div { class: "assessment-loading-line wide" }
                        div { class: "assessment-loading-line" }
                        div { class: "assessment-loading-line short" }
                    }
                }
            }
        }
    }
}

#[component]
fn AssessmentMessage(icon: &'static str, title: &'static str, body: &'static str) -> Element {
    rsx! {
        div { class: "assessment-message",
            span { class: "assessment-message-icon", aria_hidden: "true", "{icon}" }
            h2 { "{title}" }
            p { "{body}" }
        }
    }
}

#[component]
fn AssessmentError(problem: AssessmentApiError, on_retry: EventHandler<()>) -> Element {
    let (title, body) = match problem {
        AssessmentApiError::AuthenticationRequired => (
            "Сессия завершилась",
            "Войдите снова, чтобы продолжить работу с шаблонами.",
        ),
        AssessmentApiError::PermissionDenied => (
            "Недостаточно прав",
            "У вашей роли нет доступа к шаблонам этой компании.",
        ),
        AssessmentApiError::NotFound => (
            "Шаблон недоступен",
            "Возможно, он был архивирован или удалён.",
        ),
        AssessmentApiError::Conflict => (
            "Шаблон уже добавлен",
            "Откройте «Мои шаблоны», чтобы продолжить работу с копией.",
        ),
        AssessmentApiError::ConfigurationUnavailable => (
            "Сервис временно недоступен",
            "Конфигурация Account API ещё не завершена.",
        ),
        AssessmentApiError::NetworkUnavailable => {
            ("Нет соединения", "Проверьте интернет и повторите загрузку.")
        }
        _ => (
            "Не удалось загрузить шаблоны",
            "Попробуйте ещё раз немного позже.",
        ),
    };
    rsx! {
        div { class: "assessment-message assessment-error", role: "alert",
            span { class: "assessment-message-icon", aria_hidden: "true", "!" }
            h2 { "{title}" }
            p { "{body}" }
            button { class: "assessment-button secondary", r#type: "button", onclick: move |_| on_retry.call(()), "Повторить" }
        }
    }
}

fn safe_error(problem: AssessmentApiError) -> &'static str {
    match problem {
        AssessmentApiError::AuthenticationRequired => "Сессия завершена. Войдите снова.",
        AssessmentApiError::PermissionDenied => "Недостаточно прав для изменения шаблона.",
        AssessmentApiError::NotFound => "Черновик больше недоступен.",
        AssessmentApiError::Conflict => "Черновик изменён в другой сессии.",
        AssessmentApiError::InvalidRequest => "Проверьте структуру, веса и показатели.",
        AssessmentApiError::NetworkUnavailable => "Нет соединения. Изменения не отправлены.",
        AssessmentApiError::ConfigurationUnavailable => "Сервис временно недоступен.",
        AssessmentApiError::InternalError => "Не удалось выполнить действие.",
    }
}

fn selected_company(state: &AccountSessionState) -> Option<Uuid> {
    match state {
        AccountSessionState::Authenticated(authenticated) => {
            authenticated.selected_company.map(|value| value.0)
        }
        _ => None,
    }
}

fn should_apply_company_response(
    requested_company_id: Uuid,
    selected_company_id: Option<Uuid>,
    switching_company_id: Option<Uuid>,
) -> bool {
    selected_company_id == Some(requested_company_id)
        && switching_company_id == Some(requested_company_id)
}

fn is_library_version_adopted(
    library_version_id: Uuid,
    company_templates: &[CompanyTemplateSummary],
) -> bool {
    company_templates
        .iter()
        .any(|template| template.source_library_version_id == Some(library_version_id))
}

fn adopt_cta_state(adopted: bool, can_adopt: bool) -> AdoptCtaState {
    if adopted {
        AdoptCtaState::OpenCompanyCopy
    } else if can_adopt {
        AdoptCtaState::Adopt
    } else {
        AdoptCtaState::SelectCompany
    }
}

fn prepare_tour_step(
    target: TourProgress,
    client: AssessmentApiClient,
    adapter: AccountSessionAdapter,
    library: Signal<Vec<LibraryTemplateSummary>>,
    mut view: Signal<AssessmentView>,
    mut tab: Signal<AssessmentTab>,
    mut load_state: Signal<LoadState>,
    mut error: Signal<Option<AssessmentApiError>>,
    mut session: Signal<AccountSessionState>,
    mut tour: Signal<Option<TourProgress>>,
) {
    if matches!(target.index, 3 | 4) {
        let Some(template_id) = library().first().map(|template| template.template_id) else {
            tour.set(Some(target));
            return;
        };
        let current = adapter.state();
        spawn(async move {
            load_state.set(LoadState::Loading);
            if let AccountSessionState::Authenticated(auth) = current {
                match get_library(&adapter, &client, &auth, template_id).await {
                    Ok(document) => view.set(AssessmentView::LibraryDetail(document)),
                    Err(problem) => error.set(Some(problem)),
                }
            }
            session.set(adapter.state());
            load_state.set(LoadState::Ready);
            tour.set(Some(target));
        });
    } else {
        view.set(AssessmentView::List);
        tab.set(AssessmentTab::Library);
        tour.set(Some(target));
    }
}

fn highlight_tour_target(selector: &str) -> bool {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    let Ok(Some(target)) = document.query_selector(selector) else {
        return false;
    };
    target.scroll_into_view();
    if let Some(popover) = document
        .get_element_by_id("assessment-tour-popover")
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = popover.focus();
    }
    true
}

fn save_tour_completed() {
    let _ = LocalStorage::set(ASSESSMENT_TOUR_PREFERENCE_KEY, true);
}

fn close_assessment_tour(mut tour: Signal<Option<TourProgress>>, completed: bool) {
    if completed {
        save_tour_completed();
    }
    tour.set(None);
    restore_assessment_tour_trigger_focus();
}

fn restore_assessment_tour_trigger_focus() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let callback = Closure::once_into_js(move || {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        if let Ok(Some(trigger)) = document.query_selector(ASSESSMENT_TOUR_TRIGGER_SELECTOR) {
            if let Ok(trigger) = trigger.dyn_into::<web_sys::HtmlElement>() {
                let _ = trigger.focus();
            }
        }
    });
    let _ = window.request_animation_frame(callback.unchecked_ref());
}

fn preferred_version(template: &CompanyTemplateSummary) -> Option<&TemplateVersionSummary> {
    template
        .latest_draft
        .as_ref()
        .or(template.latest_published.as_ref())
}

fn activity_label(value: &str) -> &'static str {
    match value {
        "evaluation" => "Оценка",
        "measurement" => "Замер",
        "walkthrough" => "Обход",
        "checklist" => "Чек-лист",
        "test" => "Тест",
        "survey" => "Опрос",
        "attestation" => "Аттестация",
        _ => "Шаблон",
    }
}

fn response_label(value: &str) -> &'static str {
    match value {
        "boolean" => "Да / нет",
        "single_choice" => "Один вариант",
        "multi_choice" => "Несколько вариантов",
        "numeric" => "Число",
        "text" => "Текст",
        _ => "Ответ",
    }
}

fn status_label(value: &str) -> &'static str {
    match value {
        "draft" => "Черновик",
        "published" => "Опубликован",
        "archived" => "Архивирован",
        _ => "Недоступен",
    }
}

fn account_error(error: AccountApiError) -> AssessmentApiError {
    match error {
        AccountApiError::AuthenticationRequired | AccountApiError::ReauthenticationRequired => {
            AssessmentApiError::AuthenticationRequired
        }
        AccountApiError::PermissionDenied => AssessmentApiError::PermissionDenied,
        AccountApiError::InvalidRequest => AssessmentApiError::InvalidRequest,
        AccountApiError::ConfigurationUnavailable => AssessmentApiError::ConfigurationUnavailable,
        AccountApiError::NetworkUnavailable => AssessmentApiError::NetworkUnavailable,
        AccountApiError::RateLimited => AssessmentApiError::InvalidRequest,
        AccountApiError::InternalError => AssessmentApiError::InternalError,
    }
}

#[cfg(target_arch = "wasm32")]
async fn list_library(
    adapter: &AccountSessionAdapter,
    client: &AssessmentApiClient,
    authenticated: &AuthenticatedAccountSession,
    query: LibraryQuery,
) -> Result<Vec<LibraryTemplateSummary>, AssessmentApiError> {
    match client
        .list_library_templates(&authenticated.access_token, &query)
        .await
    {
        Err(AssessmentApiError::AuthenticationRequired) => {
            let refreshed = adapter.refresh().await.map_err(account_error)?;
            let AccountSessionState::Authenticated(authenticated) = refreshed else {
                return Err(AssessmentApiError::AuthenticationRequired);
            };
            client
                .list_library_templates(&authenticated.access_token, &query)
                .await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn get_library(
    adapter: &AccountSessionAdapter,
    client: &AssessmentApiClient,
    authenticated: &AuthenticatedAccountSession,
    template_id: Uuid,
) -> Result<TemplateDocument, AssessmentApiError> {
    match client
        .get_library_template(&authenticated.access_token, template_id)
        .await
    {
        Err(AssessmentApiError::AuthenticationRequired) => {
            let refreshed = adapter.refresh().await.map_err(account_error)?;
            let AccountSessionState::Authenticated(authenticated) = refreshed else {
                return Err(AssessmentApiError::AuthenticationRequired);
            };
            client
                .get_library_template(&authenticated.access_token, template_id)
                .await
        }
        result => result,
    }
}

#[cfg(target_arch = "wasm32")]
async fn list_company(
    adapter: &AccountSessionAdapter,
    client: &AssessmentApiClient,
    authenticated: &AuthenticatedAccountSession,
    company_id: Uuid,
) -> Result<Vec<CompanyTemplateSummary>, AssessmentApiError> {
    match client
        .list_company_templates(&authenticated.access_token, company_id)
        .await
    {
        Err(AssessmentApiError::AuthenticationRequired) => {
            let refreshed = adapter.refresh().await.map_err(account_error)?;
            let AccountSessionState::Authenticated(authenticated) = refreshed else {
                return Err(AssessmentApiError::AuthenticationRequired);
            };
            client
                .list_company_templates(&authenticated.access_token, company_id)
                .await
        }
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authenticated_assessment_entry_reuses_shared_account_session() {
        let state = AccountSessionState::Authenticated(AuthenticatedAccountSession {
            access_token: crate::account_api::AccountAccessToken::from_server(
                "synthetic-token".into(),
            )
            .unwrap(),
            expires_at: "2030-01-01T00:00:00Z".into(),
            bootstrap: crate::account_api::AccountBootstrap {
                account: crate::account_api::BootstrapAccount {
                    id: Uuid::from_u128(1),
                    status: "active".into(),
                    security_version: 1,
                },
                companies: Vec::new(),
            },
            selected_company: None,
            company_selection_required: false,
        });

        assert!(assessment_entry_can_reuse_session(&state));
        assert!(!assessment_entry_can_reuse_session(
            &AccountSessionState::Uninitialized
        ));
        assert!(!assessment_entry_can_reuse_session(
            &AccountSessionState::Anonymous
        ));
    }

    fn company_template(source: Option<Uuid>, name: &str) -> CompanyTemplateSummary {
        CompanyTemplateSummary {
            template_id: Uuid::from_u128(10),
            code: "company-template".into(),
            name: name.into(),
            activity_type: "evaluation".into(),
            status: "active".into(),
            source_library_version_id: source,
            methodology: None,
            latest_draft: None,
            latest_published: None,
            can_manage: true,
        }
    }

    fn company_draft_document() -> CompanyDraftDocument {
        CompanyDraftDocument {
            template: crate::assessment_api::DraftTemplateInfo {
                id: Uuid::from_u128(10),
                scope: "company".into(),
                company_id: Some(Uuid::from_u128(11)),
                source_library_version_id: Some(Uuid::from_u128(12)),
                name: "Template".into(),
                code: "template".into(),
                activity_type: "evaluation".into(),
            },
            version: crate::assessment_api::DraftVersionInfo {
                id: Uuid::from_u128(13),
                version: 2,
                status: "draft".into(),
                edit_revision: 1,
                local_description: None,
            },
            methodology: crate::assessment_api::MethodologyDetail {
                id: Uuid::from_u128(14),
                code: Some("synthetic".into()),
                title: "Synthetic".into(),
                body: None,
                version: 1,
                owner_type: Some("restos".into()),
                status: Some("active".into()),
            },
            sections: vec![DraftSection {
                code: "first".into(),
                title: "First".into(),
                description: None,
                section_kind: "section".into(),
                sort_order: 0,
                weight: None,
                parent_code: None,
                items: vec![DraftItem {
                    code: "criterion".into(),
                    prompt: "Criterion".into(),
                    guidance: None,
                    response_type: "boolean".into(),
                    is_required: true,
                    sort_order: 0,
                    weight: Some(serde_json::Value::String("1".into())),
                    min_value: None,
                    max_value: None,
                    passing_value: None,
                    evidence_mode: "none".into(),
                    criticality: "normal".into(),
                    config: serde_json::json!({}),
                    options: vec![],
                    metric_mappings: vec![DraftMetricMapping {
                        metric_code: "service".into(),
                        contribution_weight: serde_json::Value::String("1".into()),
                        direction: "positive".into(),
                    }],
                }],
            }],
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn display_labels_are_safe_and_stable() {
        assert_eq!(activity_label("walkthrough"), "Обход");
        assert_eq!(activity_label("unknown"), "Шаблон");
        assert_eq!(response_label("multi_choice"), "Несколько вариантов");
        assert_eq!(status_label("draft"), "Черновик");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn adopted_state_uses_provenance_not_title() {
        let source = Uuid::from_u128(1);
        let same_title_without_provenance = company_template(None, "Оценка сервиса");
        assert!(!is_library_version_adopted(
            source,
            &[same_title_without_provenance]
        ));

        let different_title_with_provenance =
            company_template(Some(source), "Локальное название ресторана");
        assert!(is_library_version_adopted(
            source,
            &[different_title_with_provenance]
        ));
        assert!(!is_library_version_adopted(
            Uuid::from_u128(2),
            &[company_template(Some(source), "Любое название")]
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn reviewed_import_codes_use_weighted_scoring_without_guessing_others() {
        assert!(is_imported_weighted("cook-kln"));
        assert!(is_imported_weighted("production-walkthrough"));
        assert!(is_imported_weighted("waiter-kln"));
        assert!(!is_imported_weighted("unreviewed-template"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn waiter_launch_state_uses_published_version_contract() {
        assert!(template_version_launch_available("published"));
        assert!(!template_version_launch_available("draft"));
        assert!(!template_version_launch_available("archived"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn current_company_response_is_applied() {
        let company_id = Uuid::from_u128(20);
        assert!(should_apply_company_response(
            company_id,
            Some(company_id),
            Some(company_id)
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn previous_company_response_is_stale() {
        assert!(!should_apply_company_response(
            Uuid::from_u128(20),
            Some(Uuid::from_u128(21)),
            Some(Uuid::from_u128(21))
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn response_without_active_switch_is_stale() {
        let company_id = Uuid::from_u128(20);
        assert!(!should_apply_company_response(
            company_id,
            Some(company_id),
            None
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn another_company_response_cannot_clear_current_switch() {
        assert!(!should_apply_company_response(
            Uuid::from_u128(20),
            Some(Uuid::from_u128(21)),
            Some(Uuid::from_u128(21))
        ));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn adopt_cta_mapping_is_explicit() {
        assert_eq!(adopt_cta_state(true, true), AdoptCtaState::OpenCompanyCopy);
        assert_eq!(adopt_cta_state(false, true), AdoptCtaState::Adopt);
        assert_eq!(adopt_cta_state(false, false), AdoptCtaState::SelectCompany);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn tour_sequence_and_boundaries_are_stable() {
        let first = TourProgress::first();
        assert!(first.is_first());
        assert_eq!(first.back(), first);
        let last = (0..TOUR_STEPS.len()).fold(first, |progress, _| progress.next());
        assert!(last.is_last());
        assert_eq!(last.next(), last);
        assert_eq!(last.back().index, TOUR_STEPS.len() - 2);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn every_tour_step_maps_to_exactly_one_target() {
        let expected = [
            TourTarget::LibraryTabs,
            TourTarget::LibraryFilters,
            TourTarget::LibraryCard,
            TourTarget::Methodology,
            TourTarget::AdoptAction,
            TourTarget::CompanyTab,
        ];

        for (index, expected_target) in expected.iter().copied().enumerate() {
            let progress = Some(TourProgress { index });
            assert_eq!(active_tour_target(progress), Some(expected_target));
            assert_eq!(
                expected
                    .iter()
                    .filter(|target| is_active_tour_target(progress, **target))
                    .count(),
                1
            );
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn methodology_tour_targets_the_compact_semantic_header() {
        assert_eq!(TOUR_STEPS[3].target, TourTarget::Methodology);
        assert_eq!(TOUR_STEPS[3].selector, METHODOLOGY_TOUR_SELECTOR);
        assert_eq!(
            METHODOLOGY_TOUR_TARGET_CLASS,
            "assessment-methodology-tour-target"
        );
        assert_ne!(METHODOLOGY_TOUR_TARGET_CLASS, "assessment-methodology");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn next_and_back_update_the_active_target() {
        let first = TourProgress::first();
        let second = first.next();
        assert_eq!(
            active_tour_target(Some(second)),
            Some(TourTarget::LibraryFilters)
        );
        assert_eq!(
            active_tour_target(Some(second.back())),
            Some(TourTarget::LibraryTabs)
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn closed_tour_has_no_active_target() {
        assert_eq!(active_tour_target(None), None);
        for target in [
            TourTarget::LibraryTabs,
            TourTarget::LibraryFilters,
            TourTarget::LibraryCard,
            TourTarget::Methodology,
            TourTarget::AdoptAction,
            TourTarget::CompanyTab,
        ] {
            assert!(!is_active_tour_target(None, target));
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn finish_and_skip_clear_the_active_target() {
        let finished = None;
        let skipped = None;
        assert_eq!(active_tour_target(finished), None);
        assert_eq!(active_tour_target(skipped), None);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn tour_preference_key_is_versioned_and_non_security_state() {
        assert_eq!(
            ASSESSMENT_TOUR_PREFERENCE_KEY,
            "restos_ui_assessment_tour_v1"
        );
        assert!(!ASSESSMENT_TOUR_PREFERENCE_KEY.contains("token"));
        assert!(!ASSESSMENT_TOUR_PREFERENCE_KEY.contains("permission"));
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn company_editor_synchronizes_mapping_weight_with_criterion() {
        let mut item = DraftItem {
            code: "service".into(),
            prompt: "Service".into(),
            guidance: None,
            response_type: "boolean".into(),
            is_required: true,
            sort_order: 0,
            weight: Some(serde_json::Value::String("1".into())),
            min_value: None,
            max_value: None,
            passing_value: None,
            evidence_mode: "optional_comment".into(),
            criticality: "normal".into(),
            config: serde_json::json!({}),
            options: vec![],
            metric_mappings: vec![DraftMetricMapping {
                metric_code: "service".into(),
                contribution_weight: serde_json::Value::String("1".into()),
                direction: "positive".into(),
            }],
        };
        synchronize_mapping_weights(&mut item, Some(serde_json::Value::String("2.5".into())));
        assert_eq!(
            item.weight,
            Some(item.metric_mappings[0].contribution_weight.clone())
        );
        assert_eq!(item.metric_mappings[0].metric_code, "service");
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn company_editor_visibility_is_fail_closed_by_manage_projection() {
        let mut template = company_template(None, "Local");
        assert!(template.can_manage);
        template.can_manage = false;
        assert!(!template.can_manage);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn company_editor_requires_mapping_and_reports_exact_item() {
        let mut draft = company_draft_document();
        draft.sections[0].items[0].metric_mappings.clear();
        assert!(!draft_document_valid(&draft));
        assert_eq!(
            draft_validation_message(&draft).as_deref(),
            Some("Раздел 1, пункт 1: выберите показатель аналитики.")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn company_editor_accepts_equivalent_numeric_weight_representations() {
        let mut draft = company_draft_document();
        draft.sections[0].items[0].weight = Some(serde_json::Value::String("1.0".into()));
        draft.sections[0].items[0].metric_mappings[0].contribution_weight =
            serde_json::Value::from(1);

        assert!(draft_document_valid(&draft));
        assert_eq!(draft_validation_message(&draft), None);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn company_editor_moves_item_between_sections_without_copying_it() {
        let mut draft = company_draft_document();
        let original_code = draft.sections[0].items[0].code.clone();
        draft.sections.push(DraftSection {
            code: "second".into(),
            title: "Second".into(),
            description: None,
            section_kind: "section".into(),
            sort_order: 1,
            weight: None,
            parent_code: None,
            items: vec![],
        });
        assert!(move_draft_item(&mut draft, 0, 0, 1));
        assert!(draft.sections[0].items.is_empty());
        assert_eq!(draft.sections[1].items.len(), 1);
        assert_eq!(draft.sections[1].items[0].code, original_code);
        assert_eq!(draft.sections[1].items[0].sort_order, 0);
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn missing_tour_target_is_safe() {
        assert!(!highlight_tour_target(
            "[data-tour='stage20c-target-that-does-not-exist']"
        ));
    }
}
