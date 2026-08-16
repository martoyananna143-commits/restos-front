//! Components module — exports all page components.

pub mod nav_bar;
mod shared;

pub use shared::{
    AccountPageSkeleton, AnalyticsPageSkeleton, ConfigListPageSkeleton, EmployeesPageSkeleton,
    ErrorView, EvaluationDetailSkeleton, EvaluationsOverviewSkeleton, FormPrepareSkeleton,
    HomeDashboardSkeleton, LoadingView, SessionGateSkeleton,
};
mod account_legal_notice;
mod account_page;
mod account_portal;
mod account_today;
mod ai_assistant_page;
mod analytics_page;
mod assessment_attempts;
mod assessment_management;
mod assessments;
mod auth_page;
mod brand_foundation;
mod criteria_page;
mod criteria_select_page;
mod criterion_sets_page;
mod employees_page;
mod evaluation_types_page;
mod evaluations_page;
mod evaluations_section;
mod form;
mod home_page;
mod inputs;
mod internships_page;
mod join_organization;
mod measurement_launcher;
mod organization_settings;
mod pin_step_up;
mod product_measurement;
mod question;
mod restaurant_metrics_dashboard;
mod russian_phone_input;
mod standalone_account_auth;
mod team_management;
mod workforce_onboarding;

pub use account_page::AccountPage;
pub use account_portal::{
    root_after_account_logout, startup_root_state, AccountPilotShell, AccountRootState,
};
pub use account_today::AccountToday;
pub use ai_assistant_page::AiAssistantPage;
pub use analytics_page::AnalyticsPage;
pub use assessment_attempts::{AssessmentAttemptsPage, AssessmentListView};
pub use assessment_management::{
    bootstrap_owner_capability, capability_from_probe, ManagerCapability,
};
pub use assessments::{AssessmentLibrarySection, AssessmentsPage};
pub use auth_page::AuthPage;
#[cfg(debug_assertions)]
pub use brand_foundation::BrandFoundationProof;
#[allow(unused_imports)]
pub use brand_foundation::{
    BrandButton, BrandButtonVariant, BrandGlass, BrandGlassVariant, BrandStatus,
    BrandStatusVariant, MobileLaunchAnimation,
};
pub use criteria_page::CriteriaPage;
#[allow(unused_imports)]
pub use criteria_select_page::CriteriaSelectPage;
pub use criterion_sets_page::CriterionSetsPage;
pub use employees_page::EmployeesPage;
pub use evaluation_types_page::EvaluationTypesPage;
pub use evaluations_page::EvaluationsPage;
pub use evaluations_section::EvaluationsSection;
pub use form::EvaluationForm;
pub use home_page::HomePage;
pub use internships_page::InternshipsPage;
pub use join_organization::JoinOrganizationPage;
pub use measurement_launcher::MeasurementLauncher;
pub use organization_settings::{OrganizationSettingsPage, OrganizationSettingsSection};
pub use pin_step_up::PinStepUpScreen;
pub use product_measurement::ProductMeasurementPanel;
pub use restaurant_metrics_dashboard::{MetricsNavigationView, RestaurantMetricsDashboardPage};
pub use standalone_account_auth::AccountAuthPage;
pub use team_management::{TeamArea, TeamManagementPage};
pub use workforce_onboarding::WorkforceOnboardingPage;
