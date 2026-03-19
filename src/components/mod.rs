//! Components module — exports all page components.

pub mod nav_bar;
mod shared;
mod inputs;
mod question;
mod form;
mod home_page;
mod employees_page;
mod evaluations_page;
mod analytics_page;
mod criteria_select_page;
mod auth_page;
mod criteria_page;
mod criterion_sets_page;
mod evaluation_types_page;
mod evaluations_section;

pub use auth_page::AuthPage;
pub use evaluations_section::EvaluationsSection;
pub use form::EvaluationForm;
pub use home_page::HomePage;
pub use employees_page::EmployeesPage;
pub use evaluations_page::EvaluationsPage;
pub use analytics_page::AnalyticsPage;
pub use criteria_page::CriteriaPage;
pub use criterion_sets_page::CriterionSetsPage;
pub use evaluation_types_page::EvaluationTypesPage;
#[allow(unused_imports)]
pub use criteria_select_page::CriteriaSelectPage;
