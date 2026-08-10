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
mod ai_assistant_page;
mod analytics_page;
mod assessment_attempts;
mod assessment_management;
mod assessments;
mod auth_page;
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
mod pin_step_up;
mod question;
mod standalone_account_auth;
mod workforce_onboarding;

pub use account_page::AccountPage;
pub use account_portal::{
    root_after_account_logout, startup_root_state, AccountPilotShell, AccountRootState,
};
pub use ai_assistant_page::AiAssistantPage;
pub use analytics_page::AnalyticsPage;
pub use assessment_attempts::AssessmentAttemptsPage;
pub use assessment_management::{
    bootstrap_owner_capability, capability_from_probe, AssessmentManagementPage, ManagerCapability,
};
pub use assessments::AssessmentsPage;
pub use auth_page::AuthPage;
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
pub use pin_step_up::PinStepUpScreen;
pub use standalone_account_auth::AccountAuthPage;
pub use workforce_onboarding::WorkforceOnboardingPage;
