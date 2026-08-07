//! Components module — exports all page components.

pub mod nav_bar;
mod shared;

pub use shared::{
    AccountPageSkeleton, AnalyticsPageSkeleton, ConfigListPageSkeleton, EmployeesPageSkeleton,
    ErrorView, EvaluationDetailSkeleton, EvaluationsOverviewSkeleton, FormPrepareSkeleton,
    HomeDashboardSkeleton, LoadingView, SessionGateSkeleton,
};
mod inputs;
mod question;
mod form;
mod home_page;
mod employees_page;
mod evaluations_page;
mod analytics_page;
mod criteria_select_page;
mod auth_page;
mod account_page;
mod pin_step_up;
mod criteria_page;
mod criterion_sets_page;
mod evaluation_types_page;
mod evaluations_section;
mod ai_assistant_page;
mod internships_page;
mod assessments;
mod assessment_attempts;
mod account_portal;

pub use auth_page::AuthPage;
pub use account_page::AccountPage;
pub use pin_step_up::PinStepUpScreen;
pub use evaluations_section::EvaluationsSection;
pub use form::EvaluationForm;
pub use home_page::HomePage;
pub use employees_page::EmployeesPage;
pub use evaluations_page::EvaluationsPage;
pub use analytics_page::AnalyticsPage;
pub use criteria_page::CriteriaPage;
pub use criterion_sets_page::CriterionSetsPage;
pub use evaluation_types_page::EvaluationTypesPage;
pub use ai_assistant_page::AiAssistantPage;
pub use internships_page::InternshipsPage;
pub use assessments::AssessmentsPage;
pub use assessment_attempts::AssessmentAttemptsPage;
pub use account_portal::{
    root_after_account_logout, startup_root_state, AccountAuthPage, AccountPilotShell,
    AccountRootState,
};
#[allow(unused_imports)]
pub use criteria_select_page::CriteriaSelectPage;
