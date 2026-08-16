//! Account navigation source of truth shared by desktop, mobile and route handling.

use dioxus::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NavigationId {
    Today,
    Assessments,
    AssessmentsActive,
    AssessmentsHistory,
    AssessmentsPlan,
    Templates,
    TemplateLibrary,
    CompanyTemplates,
    Organization,
    OrganizationEmployees,
    OrganizationVenues,
    OrganizationAccess,
    OrganizationInvitations,
    Analytics,
    AnalyticsBuilder,
    AnalyticsResults,
    AnalyticsRestaurant,
    AnalyticsSources,
    Team,
    TeamShifts,
    TeamTasks,
    TeamCalendar,
    JoinOrganization,
    Profile,
    Security,
    Logout,
    Measure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationKind {
    Page,
    Subpage,
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationProductState {
    Active,
    Available,
    ComingSoon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationCapability {
    Authenticated,
    Manager,
    OrganizationManager,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationRole {
    Employee,
    VenueManager,
    OrganizationManager,
    Owner,
}

impl NavigationRole {
    pub const fn is_manager(self) -> bool {
        !matches!(self, Self::Employee)
    }

    pub const fn can_manage_organization(self) -> bool {
        matches!(self, Self::OrganizationManager | Self::Owner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobilePlacement {
    Primary,
    More,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationZone {
    Main,
    Service,
    Hidden,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationBadgeSource {
    ActiveAssessments,
    PendingInvitations,
    TasksAwaitingAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationIconId {
    Home,
    ClipboardCheck,
    PlayCircle,
    History,
    Flag,
    Layers,
    Library,
    Files,
    BuildingCog,
    Users,
    Store,
    ShieldHierarchy,
    UserPlus,
    Chart,
    Sliders,
    ChartCombined,
    Gauge,
    ListTree,
    UsersRound,
    Clock,
    SquareCheck,
    Calendar,
    Link,
    User,
    ShieldCheck,
    LogOut,
    PlusCircle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavigationItem {
    pub id: NavigationId,
    pub route: Option<&'static str>,
    pub parent_id: Option<NavigationId>,
    pub label: &'static str,
    pub icon: NavigationIconId,
    pub order: u8,
    pub capability: NavigationCapability,
    pub kind: NavigationKind,
    pub state: NavigationProductState,
    pub mobile: MobilePlacement,
    pub zone: NavigationZone,
    pub badge_source: Option<NavigationBadgeSource>,
}

const fn item(
    id: NavigationId,
    route: Option<&'static str>,
    parent_id: Option<NavigationId>,
    label: &'static str,
    icon: NavigationIconId,
    order: u8,
    capability: NavigationCapability,
    kind: NavigationKind,
    state: NavigationProductState,
    mobile: MobilePlacement,
    zone: NavigationZone,
    badge_source: Option<NavigationBadgeSource>,
) -> NavigationItem {
    NavigationItem {
        id,
        route,
        parent_id,
        label,
        icon,
        order,
        capability,
        kind,
        state,
        mobile,
        zone,
        badge_source,
    }
}

pub const NAVIGATION_REGISTRY: [NavigationItem; 27] = [
    item(
        NavigationId::Today,
        Some("#/today"),
        None,
        "Сегодня",
        NavigationIconId::Home,
        10,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Active,
        MobilePlacement::Primary,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::Assessments,
        Some("#/assessments"),
        None,
        "Мои оценки",
        NavigationIconId::ClipboardCheck,
        20,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Available,
        MobilePlacement::Primary,
        NavigationZone::Main,
        Some(NavigationBadgeSource::ActiveAssessments),
    ),
    item(
        NavigationId::AssessmentsActive,
        Some("#/assessments/active"),
        Some(NavigationId::Assessments),
        "Активные",
        NavigationIconId::PlayCircle,
        21,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        Some(NavigationBadgeSource::ActiveAssessments),
    ),
    item(
        NavigationId::AssessmentsHistory,
        Some("#/assessments/history"),
        Some(NavigationId::Assessments),
        "История",
        NavigationIconId::History,
        22,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::AssessmentsPlan,
        Some("#/assessments/plan"),
        Some(NavigationId::Assessments),
        "Плановые показатели",
        NavigationIconId::Flag,
        23,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::ComingSoon,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::Templates,
        Some("#/templates"),
        None,
        "Шаблоны",
        NavigationIconId::Layers,
        30,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Available,
        MobilePlacement::Primary,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::TemplateLibrary,
        Some("#/templates/library"),
        Some(NavigationId::Templates),
        "Библиотека шаблонов",
        NavigationIconId::Library,
        31,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::CompanyTemplates,
        Some("#/templates/company"),
        Some(NavigationId::Templates),
        "Мои шаблоны",
        NavigationIconId::Files,
        32,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::Organization,
        Some("#/organization"),
        None,
        "Настройка организации",
        NavigationIconId::BuildingCog,
        40,
        NavigationCapability::OrganizationManager,
        NavigationKind::Page,
        NavigationProductState::Available,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::OrganizationEmployees,
        Some("#/organization/employees"),
        Some(NavigationId::Organization),
        "Сотрудники",
        NavigationIconId::Users,
        41,
        NavigationCapability::OrganizationManager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::OrganizationVenues,
        Some("#/organization/venues"),
        Some(NavigationId::Organization),
        "Рестораны",
        NavigationIconId::Store,
        42,
        NavigationCapability::OrganizationManager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::OrganizationAccess,
        Some("#/organization/access"),
        Some(NavigationId::Organization),
        "Должности и права",
        NavigationIconId::ShieldHierarchy,
        43,
        NavigationCapability::OrganizationManager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::OrganizationInvitations,
        Some("#/organization/invitations"),
        Some(NavigationId::Organization),
        "Приглашения",
        NavigationIconId::UserPlus,
        44,
        NavigationCapability::OrganizationManager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        Some(NavigationBadgeSource::PendingInvitations),
    ),
    item(
        NavigationId::Analytics,
        Some("#/analytics"),
        None,
        "Аналитика",
        NavigationIconId::Chart,
        50,
        NavigationCapability::Manager,
        NavigationKind::Page,
        NavigationProductState::Available,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::AnalyticsBuilder,
        Some("#/analytics/builder"),
        Some(NavigationId::Analytics),
        "Конструктор отчёта",
        NavigationIconId::Sliders,
        51,
        NavigationCapability::Manager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::AnalyticsResults,
        Some("#/analytics/results"),
        Some(NavigationId::Analytics),
        "Результаты",
        NavigationIconId::ChartCombined,
        52,
        NavigationCapability::Manager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::AnalyticsRestaurant,
        Some("#/analytics/restaurant"),
        Some(NavigationId::Analytics),
        "Показатели ресторана",
        NavigationIconId::Gauge,
        53,
        NavigationCapability::Manager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::AnalyticsSources,
        Some("#/analytics/sources"),
        Some(NavigationId::Analytics),
        "Источники показателей",
        NavigationIconId::ListTree,
        54,
        NavigationCapability::Manager,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::Team,
        Some("#/team"),
        None,
        "Управление командой",
        NavigationIconId::UsersRound,
        60,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Available,
        MobilePlacement::Primary,
        NavigationZone::Main,
        Some(NavigationBadgeSource::TasksAwaitingAction),
    ),
    item(
        NavigationId::TeamShifts,
        Some("#/team/shifts"),
        Some(NavigationId::Team),
        "Смены",
        NavigationIconId::Clock,
        61,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::ComingSoon,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::TeamTasks,
        Some("#/team/tasks"),
        Some(NavigationId::Team),
        "Задачи",
        NavigationIconId::SquareCheck,
        62,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Main,
        Some(NavigationBadgeSource::TasksAwaitingAction),
    ),
    item(
        NavigationId::TeamCalendar,
        Some("#/team/calendar"),
        Some(NavigationId::Team),
        "Календарь",
        NavigationIconId::Calendar,
        63,
        NavigationCapability::Authenticated,
        NavigationKind::Subpage,
        NavigationProductState::ComingSoon,
        MobilePlacement::More,
        NavigationZone::Main,
        None,
    ),
    item(
        NavigationId::JoinOrganization,
        Some("#/account/join"),
        None,
        "У меня есть приглашение",
        NavigationIconId::Link,
        80,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Service,
        None,
    ),
    item(
        NavigationId::Profile,
        Some("#/account/profile"),
        None,
        "Профиль",
        NavigationIconId::User,
        81,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Service,
        None,
    ),
    item(
        NavigationId::Security,
        Some("#/account/security"),
        None,
        "Безопасность",
        NavigationIconId::ShieldCheck,
        82,
        NavigationCapability::Authenticated,
        NavigationKind::Page,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Service,
        None,
    ),
    item(
        NavigationId::Logout,
        None,
        None,
        "Выйти",
        NavigationIconId::LogOut,
        83,
        NavigationCapability::Authenticated,
        NavigationKind::Action,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Service,
        None,
    ),
    item(
        NavigationId::Measure,
        Some("#/measure"),
        Some(NavigationId::Assessments),
        "Сделать замер",
        NavigationIconId::PlusCircle,
        90,
        NavigationCapability::Manager,
        NavigationKind::Action,
        NavigationProductState::Active,
        MobilePlacement::More,
        NavigationZone::Hidden,
        None,
    ),
];

pub fn item_by_id(id: NavigationId) -> &'static NavigationItem {
    NAVIGATION_REGISTRY
        .iter()
        .find(|item| item.id == id)
        .expect("registry contains every NavigationId used by production")
}

pub fn visible_for_role(item: &NavigationItem, role: NavigationRole) -> bool {
    match item.capability {
        NavigationCapability::Authenticated => true,
        NavigationCapability::Manager => role.is_manager(),
        NavigationCapability::OrganizationManager => role.can_manage_organization(),
    }
}

pub fn visible_items(role: NavigationRole) -> Vec<&'static NavigationItem> {
    let mut items = NAVIGATION_REGISTRY
        .iter()
        .filter(|item| item.zone != NavigationZone::Hidden && visible_for_role(item, role))
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.order);
    items
}

pub fn parent_for(id: NavigationId) -> Option<NavigationId> {
    item_by_id(id).parent_id
}

pub fn default_child(parent: NavigationId) -> Option<NavigationId> {
    match parent {
        NavigationId::Assessments => Some(NavigationId::AssessmentsActive),
        NavigationId::Templates => Some(NavigationId::TemplateLibrary),
        NavigationId::Organization => Some(NavigationId::OrganizationEmployees),
        NavigationId::Analytics => Some(NavigationId::AnalyticsBuilder),
        NavigationId::Team => Some(NavigationId::TeamTasks),
        _ => None,
    }
}

pub fn navigable_target(id: NavigationId) -> Option<NavigationId> {
    if id == NavigationId::Logout {
        return None;
    }
    default_child(id).or(Some(id))
}

pub fn authorized_target(hash: &str, role: NavigationRole) -> NavigationId {
    resolve_hash(hash)
        .filter(|id| visible_for_role(item_by_id(*id), role))
        .unwrap_or(NavigationId::Today)
}

pub fn route_for(id: NavigationId) -> Option<&'static str> {
    item_by_id(id).route
}

pub fn resolve_hash(hash: &str) -> Option<NavigationId> {
    if hash.starts_with("#token=") {
        return Some(NavigationId::JoinOrganization);
    }
    let matched = NAVIGATION_REGISTRY
        .iter()
        .find(|item| item.route == Some(hash))?;
    default_child(matched.id).or(Some(matched.id))
}

pub fn active_parent(id: NavigationId) -> NavigationId {
    parent_for(id).unwrap_or(id)
}

#[component]
pub fn NavigationIcon(icon: NavigationIconId, class: Option<String>) -> Element {
    let path = match icon {
        NavigationIconId::Home => "M3 11.5 12 4l9 7.5V21h-6v-6H9v6H3Z",
        NavigationIconId::ClipboardCheck => "M9 5h6m-7 9 2.5 2.5L16 11m-9-8h10a2 2 0 0 1 2 2v16H5V5a2 2 0 0 1 2-2Z",
        NavigationIconId::PlayCircle => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Zm-2-14 6 4-6 4Z",
        NavigationIconId::History => "M3 12a9 9 0 1 0 3-6.7L3 8m0-5v5h5m4-1v5l3 2",
        NavigationIconId::Flag => "M5 22V4m0 1h11l-2 4 2 4H5",
        NavigationIconId::Layers => "m12 2 9 5-9 5-9-5Zm-9 10 9 5 9-5M3 17l9 5 9-5",
        NavigationIconId::Library => "M4 4h5v16H4Zm7 0h4v16h-4Zm6 2 3-1 3 14-3 1Z",
        NavigationIconId::Files => "M6 2h9l4 4v14H6Zm9 0v5h5M3 6v16h12",
        NavigationIconId::BuildingCog => "M4 21V5h10v16M8 9h2m-2 4h2m-2 4h2m7-7v3m-3-1.5 2.6 1.5m0 3L16 17.5m-2.6-1.5L16 14.5",
        NavigationIconId::Users => "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2m7-10a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm8-1a4 4 0 0 1 0 8",
        NavigationIconId::Store => "M3 9l2-6h14l2 6m-18 0a3 3 0 0 0 6 0 3 3 0 0 0 6 0 3 3 0 0 0 6 0v12H3Zm5 12v-6h8v6",
        NavigationIconId::ShieldHierarchy => "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Zm0-13v3m-4 4h8m-8 0v2m8-2v2",
        NavigationIconId::UserPlus => "M15 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2m7-10a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm11-2v6m-3-3h6",
        NavigationIconId::Chart => "M4 20V10m6 10V4m6 16v-7m5 7H2",
        NavigationIconId::Sliders => "M4 7h10m4 0h2M4 17h2m4 0h10M14 4v6M6 14v6",
        NavigationIconId::ChartCombined => "M3 20h18M5 17l4-5 4 3 6-9m-12 12V8m6 10V10m6 8V4",
        NavigationIconId::Gauge => "M4 18a8 8 0 1 1 16 0M12 18l4-5m-9-2 1 1m9-1-1 1m-4-4v1",
        NavigationIconId::ListTree => "M4 5h4m4 0h8M4 12h8m4 0h4M4 19h4m4 0h8M8 5v14m0-7h4m0 0v7",
        NavigationIconId::UsersRound => "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2m7-10a4 4 0 1 0 0-8 4 4 0 0 0 0 8Zm8 10v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8",
        NavigationIconId::Clock => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Zm0-15v5l3 2",
        NavigationIconId::SquareCheck => "M9 11l3 3 7-7M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11",
        NavigationIconId::Calendar => "M4 5h16v16H4Zm0 5h16M8 2v6m8-6v6m-8 6h2m4 0h2m-8 4h2m4 0h2",
        NavigationIconId::Link => "M10 13a5 5 0 0 0 7.5.5l2-2a5 5 0 0 0-7-7l-1.1 1.1M14 11a5 5 0 0 0-7.5-.5l-2 2a5 5 0 0 0 7 7l1.1-1.1",
        NavigationIconId::User => "M12 12a5 5 0 1 0 0-10 5 5 0 0 0 0 10Zm-9 10a9 9 0 0 1 18 0",
        NavigationIconId::ShieldCheck => "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Zm-4-10 3 3 5-6",
        NavigationIconId::LogOut => "M10 17l5-5-5-5m5 5H3m9-9h7a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-7",
        NavigationIconId::PlusCircle => "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Zm0-14v8m-4-4h8",
    };
    rsx! {
        svg {
            class: class.unwrap_or_else(|| "navigation-icon".into()),
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.8",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            "focusable": "false",
            path { d: path }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn registry_ids_and_page_routes_are_unique_and_icon_complete() {
        let ids = NAVIGATION_REGISTRY
            .iter()
            .map(|item| item.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), NAVIGATION_REGISTRY.len());
        let routes = NAVIGATION_REGISTRY
            .iter()
            .filter(|item| item.kind != NavigationKind::Action)
            .map(|item| item.route.expect("pages and subpages have routes"))
            .collect::<HashSet<_>>();
        assert_eq!(
            routes.len(),
            NAVIGATION_REGISTRY
                .iter()
                .filter(|item| item.kind != NavigationKind::Action)
                .count()
        );
    }

    #[wasm_bindgen_test]
    fn parent_graph_is_acyclic_and_defaults_exist() {
        let parent_map = NAVIGATION_REGISTRY
            .iter()
            .map(|item| (item.id, item.parent_id))
            .collect::<HashMap<_, _>>();
        for item in NAVIGATION_REGISTRY {
            let mut cursor = Some(item.id);
            let mut seen = HashSet::new();
            while let Some(id) = cursor {
                assert!(seen.insert(id));
                cursor = parent_map[&id];
            }
        }
        for parent in [
            NavigationId::Assessments,
            NavigationId::Templates,
            NavigationId::Organization,
            NavigationId::Analytics,
            NavigationId::Team,
        ] {
            let child = default_child(parent).expect("parent has a default child");
            assert_eq!(parent_for(child), Some(parent));
        }
    }

    #[wasm_bindgen_test]
    fn role_visibility_is_fail_closed() {
        let employee = visible_items(NavigationRole::Employee);
        assert!(employee
            .iter()
            .any(|item| item.id == NavigationId::TeamTasks));
        assert!(!employee
            .iter()
            .any(|item| item.id == NavigationId::Analytics));
        assert!(!employee
            .iter()
            .any(|item| item.id == NavigationId::Organization));
        let venue = visible_items(NavigationRole::VenueManager);
        assert!(venue.iter().any(|item| item.id == NavigationId::Analytics));
        assert!(!venue
            .iter()
            .any(|item| item.id == NavigationId::Organization));
        for role in [NavigationRole::OrganizationManager, NavigationRole::Owner] {
            assert!(visible_items(role)
                .iter()
                .any(|item| item.id == NavigationId::OrganizationInvitations));
        }
    }

    #[wasm_bindgen_test]
    fn active_child_exposes_parent_and_deferred_pages_remain_honest_routes() {
        assert_eq!(
            active_parent(NavigationId::OrganizationVenues),
            NavigationId::Organization
        );
        assert_eq!(
            navigable_target(NavigationId::TeamShifts),
            Some(NavigationId::TeamShifts)
        );
        assert_eq!(
            navigable_target(NavigationId::AssessmentsPlan),
            Some(NavigationId::AssessmentsPlan)
        );
        assert_eq!(
            navigable_target(NavigationId::Team),
            Some(NavigationId::TeamTasks)
        );
    }

    #[wasm_bindgen_test]
    fn unauthorized_deep_links_fail_closed_to_today() {
        assert_eq!(
            authorized_target("#/analytics/results", NavigationRole::Employee),
            NavigationId::Today
        );
        assert_eq!(
            authorized_target("#/organization/access", NavigationRole::VenueManager),
            NavigationId::Today
        );
        assert_eq!(
            authorized_target("#/analytics/results", NavigationRole::VenueManager),
            NavigationId::AnalyticsResults
        );
        assert_eq!(
            authorized_target("#/organization/access", NavigationRole::Owner),
            NavigationId::OrganizationAccess
        );
    }

    #[wasm_bindgen_test]
    fn deep_links_and_invitation_hash_are_exact() {
        assert_eq!(
            resolve_hash("#/analytics/sources"),
            Some(NavigationId::AnalyticsSources)
        );
        assert_eq!(resolve_hash("#/analytics/sources/extra"), None);
        assert_eq!(
            resolve_hash("#token=synthetic"),
            Some(NavigationId::JoinOrganization)
        );
        assert_eq!(
            resolve_hash("#/organization"),
            Some(NavigationId::OrganizationEmployees)
        );
    }

    #[wasm_bindgen_test]
    fn mobile_more_contains_every_visible_non_primary_item() {
        for role in [
            NavigationRole::Employee,
            NavigationRole::VenueManager,
            NavigationRole::OrganizationManager,
            NavigationRole::Owner,
        ] {
            let visible = visible_items(role);
            let primary = visible
                .iter()
                .filter(|item| item.parent_id.is_none() && item.mobile == MobilePlacement::Primary)
                .count();
            assert!(primary <= 4);
            assert!(visible
                .iter()
                .filter(|item| item.mobile == MobilePlacement::More)
                .all(|item| item.zone != NavigationZone::Hidden));
        }
    }

    #[wasm_bindgen_test]
    fn every_subpage_has_an_exact_independent_route() {
        for item in NAVIGATION_REGISTRY
            .iter()
            .filter(|item| item.kind == NavigationKind::Subpage)
        {
            let route = item.route.expect("subpage route");
            assert_eq!(resolve_hash(route), Some(item.id));
            assert!(item.parent_id.is_some());
        }
    }

    #[wasm_bindgen_test]
    fn child_visibility_never_exposes_a_hidden_parent() {
        for role in [
            NavigationRole::Employee,
            NavigationRole::VenueManager,
            NavigationRole::OrganizationManager,
            NavigationRole::Owner,
        ] {
            let visible = visible_items(role)
                .into_iter()
                .map(|item| item.id)
                .collect::<HashSet<_>>();
            for child in NAVIGATION_REGISTRY
                .iter()
                .filter(|item| visible.contains(&item.id))
                .filter_map(|item| item.parent_id)
            {
                assert!(visible.contains(&child));
            }
        }
    }

    #[wasm_bindgen_test]
    fn mobile_primary_set_is_bounded_and_comes_from_registry() {
        let primary = visible_items(NavigationRole::Employee)
            .into_iter()
            .filter(|item| item.parent_id.is_none() && item.mobile == MobilePlacement::Primary)
            .map(|item| item.id)
            .collect::<Vec<_>>();
        assert_eq!(
            primary,
            vec![
                NavigationId::Today,
                NavigationId::Assessments,
                NavigationId::Templates,
                NavigationId::Team
            ]
        );
    }

    #[wasm_bindgen_test]
    fn service_actions_are_separate_from_work_navigation() {
        for id in [
            NavigationId::JoinOrganization,
            NavigationId::Profile,
            NavigationId::Security,
            NavigationId::Logout,
        ] {
            assert_eq!(item_by_id(id).zone, NavigationZone::Service);
        }
        assert!(NAVIGATION_REGISTRY
            .iter()
            .filter(|item| item.zone == NavigationZone::Main)
            .all(|item| item.id != NavigationId::Logout));
    }

    #[wasm_bindgen_test]
    fn local_icon_source_contains_no_network_or_emoji_dependency() {
        let source = include_str!("navigation.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();
        assert!(source.contains("svg {"));
        assert!(!source.contains("http://"));
        assert!(!source.contains("https://"));
        assert!(!source.contains("cdn"));
    }

    #[wasm_bindgen_test]
    fn badge_sources_are_typed_and_never_fake_numbers() {
        let badge_items = NAVIGATION_REGISTRY
            .iter()
            .filter_map(|item| item.badge_source)
            .collect::<Vec<_>>();
        assert!(!badge_items.is_empty());
        assert!(badge_items.into_iter().all(|source| matches!(
            source,
            NavigationBadgeSource::ActiveAssessments
                | NavigationBadgeSource::PendingInvitations
                | NavigationBadgeSource::TasksAwaitingAction
        )));
    }

    #[wasm_bindgen_test]
    fn registry_order_is_stable_and_labels_are_nonempty() {
        let mut previous = 0;
        for item in NAVIGATION_REGISTRY {
            assert!(item.order > previous);
            assert!(!item.label.trim().is_empty());
            previous = item.order;
        }
    }

    #[wasm_bindgen_test]
    fn company_switch_and_unknown_routes_have_safe_home_fallback() {
        assert_eq!(
            authorized_target("#/unknown", NavigationRole::Owner),
            NavigationId::Today
        );
        assert_eq!(
            authorized_target("", NavigationRole::Employee),
            NavigationId::Today
        );
        assert_eq!(route_for(NavigationId::Today), Some("#/today"));
    }
}
