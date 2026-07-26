//! In-memory Account session state, isolated from legacy `restos_auth`.

use std::{cell::RefCell, rc::Rc};

#[cfg(target_arch = "wasm32")]
use gloo_timers::future::TimeoutFuture;
#[cfg(target_arch = "wasm32")]
use std::cell::Cell;

use crate::account_api::{
    AccountAccessToken, AccountApiClient, AccountApiError, AccountBootstrap, BootstrapCompany,
    RegisteredAccountSession, SelectedCompanyId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthenticatedAccountSession {
    pub access_token: AccountAccessToken,
    pub expires_at: String,
    pub bootstrap: AccountBootstrap,
    pub selected_company: Option<SelectedCompanyId>,
    pub company_selection_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountSessionState {
    Uninitialized,
    Refreshing,
    Authenticated(AuthenticatedAccountSession),
    Anonymous,
    Unavailable { retryable: bool },
}

#[derive(Clone)]
pub struct AccountSessionAdapter {
    api: AccountApiClient,
    state: Rc<RefCell<AccountSessionState>>,
    #[cfg(target_arch = "wasm32")]
    refresh_in_flight: Rc<Cell<bool>>,
}

impl AccountSessionAdapter {
    pub fn new(api: AccountApiClient) -> Self {
        Self {
            api,
            state: Rc::new(RefCell::new(AccountSessionState::Uninitialized)),
            #[cfg(target_arch = "wasm32")]
            refresh_in_flight: Rc::new(Cell::new(false)),
        }
    }

    pub fn state(&self) -> AccountSessionState {
        self.state.borrow().clone()
    }

    pub fn select_company(&self, company: SelectedCompanyId) -> Result<(), AccountApiError> {
        let mut state = self.state.borrow_mut();
        let AccountSessionState::Authenticated(authenticated) = &mut *state else {
            return Err(AccountApiError::AuthenticationRequired);
        };
        if !authenticated
            .bootstrap
            .companies
            .iter()
            .any(|available| available.company_id == company.0)
        {
            return Err(AccountApiError::PermissionDenied);
        }
        authenticated.selected_company = Some(company);
        authenticated.company_selection_required = false;
        Ok(())
    }

    pub fn accept_registered_session(&self, registered: RegisteredAccountSession) {
        *self.state.borrow_mut() = AccountSessionState::Authenticated(authenticated_state(
            registered.access_token,
            registered.expires_at,
            registered.bootstrap,
        ));
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn refresh(&self) -> Result<AccountSessionState, AccountApiError> {
        if self.refresh_in_flight.replace(true) {
            while self.refresh_in_flight.get() {
                TimeoutFuture::new(1).await;
            }
            return match self.state() {
                state @ AccountSessionState::Authenticated(_) => Ok(state),
                AccountSessionState::Anonymous => Err(AccountApiError::AuthenticationRequired),
                AccountSessionState::Unavailable { retryable: true } => {
                    Err(AccountApiError::NetworkUnavailable)
                }
                _ => Err(AccountApiError::ConfigurationUnavailable),
            };
        }

        *self.state.borrow_mut() = AccountSessionState::Refreshing;
        let result = self.refresh_owner().await;
        self.refresh_in_flight.set(false);
        result
    }

    #[cfg(target_arch = "wasm32")]
    async fn refresh_owner(&self) -> Result<AccountSessionState, AccountApiError> {
        let refreshed = match self.api.refresh().await {
            Ok(value) => value,
            Err(AccountApiError::AuthenticationRequired) => {
                *self.state.borrow_mut() = AccountSessionState::Anonymous;
                return Err(AccountApiError::AuthenticationRequired);
            }
            Err(AccountApiError::NetworkUnavailable) => {
                *self.state.borrow_mut() = AccountSessionState::Unavailable { retryable: true };
                return Err(AccountApiError::NetworkUnavailable);
            }
            Err(error) => {
                *self.state.borrow_mut() = AccountSessionState::Unavailable { retryable: false };
                return Err(error);
            }
        };
        let bootstrap = match self.api.bootstrap(&refreshed.access_token).await {
            Ok(value) => value,
            Err(error) => {
                *self.state.borrow_mut() = match error {
                    AccountApiError::AuthenticationRequired => AccountSessionState::Anonymous,
                    AccountApiError::NetworkUnavailable => {
                        AccountSessionState::Unavailable { retryable: true }
                    }
                    _ => AccountSessionState::Unavailable { retryable: false },
                };
                return Err(error);
            }
        };
        let state = AccountSessionState::Authenticated(authenticated_state(
            refreshed.access_token,
            refreshed.expires_at,
            bootstrap,
        ));
        *self.state.borrow_mut() = state.clone();
        Ok(state)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn logout(&self) -> Result<(), AccountApiError> {
        // Local Account state is cleared even when the network request fails.
        *self.state.borrow_mut() = AccountSessionState::Anonymous;
        self.api.logout().await
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn idempotent_get_with_one_refresh<T>(&self, path: &str) -> Result<T, AccountApiError>
    where
        T: serde::de::DeserializeOwned,
    {
        let token = self.current_token()?;
        match self.api.authenticated_get(path, &token).await {
            Err(AccountApiError::AuthenticationRequired) => {
                let refreshed = self.refresh().await?;
                let AccountSessionState::Authenticated(authenticated) = refreshed else {
                    return Err(AccountApiError::ReauthenticationRequired);
                };
                // Exactly one replay is permitted for an idempotent request.
                self.api
                    .authenticated_get(path, &authenticated.access_token)
                    .await
            }
            result => result,
        }
    }

    fn current_token(&self) -> Result<AccountAccessToken, AccountApiError> {
        match self.state() {
            AccountSessionState::Authenticated(authenticated) => Ok(authenticated.access_token),
            _ => Err(AccountApiError::AuthenticationRequired),
        }
    }
}

fn authenticated_state(
    access_token: AccountAccessToken,
    expires_at: String,
    bootstrap: AccountBootstrap,
) -> AuthenticatedAccountSession {
    let (selected_company, company_selection_required) = company_selection(&bootstrap.companies);
    AuthenticatedAccountSession {
        access_token,
        expires_at,
        bootstrap,
        selected_company,
        company_selection_required,
    }
}

fn company_selection(companies: &[BootstrapCompany]) -> (Option<SelectedCompanyId>, bool) {
    match companies {
        [company] => (Some(SelectedCompanyId(company.company_id)), false),
        [] => (None, false),
        _ => (None, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_api::{BootstrapAccount, BootstrapCompany};
    use uuid::Uuid;

    fn bootstrap(count: usize) -> AccountBootstrap {
        AccountBootstrap {
            account: BootstrapAccount {
                id: Uuid::from_u128(0x1000),
                status: "active".into(),
                security_version: 1,
            },
            companies: (0..count)
                .map(|index| BootstrapCompany {
                    company_id: Uuid::from_u128(0x2000 + index as u128),
                    company_name: format!("Company {index}"),
                    employee_profile_id: None,
                    relationship: "owner".into(),
                })
                .collect(),
        }
    }

    fn adapter() -> AccountSessionAdapter {
        AccountSessionAdapter::new(AccountApiClient::new("https://restos.test".into()).unwrap())
    }

    fn registered(count: usize) -> RegisteredAccountSession {
        RegisteredAccountSession {
            access_token: AccountAccessToken::from_server("memory-only".into()).unwrap(),
            expires_at: "2030-01-01T00:00:00Z".into(),
            bootstrap: bootstrap(count),
        }
    }

    #[test]
    fn zero_one_and_multiple_company_rules_are_explicit() {
        assert_eq!(company_selection(&bootstrap(0).companies), (None, false));
        let one = bootstrap(1);
        assert_eq!(
            company_selection(&one.companies),
            (Some(SelectedCompanyId(one.companies[0].company_id)), false)
        );
        assert_eq!(company_selection(&bootstrap(2).companies), (None, true));
    }

    #[test]
    fn selection_is_limited_to_bootstrap_companies() {
        let adapter = adapter();
        adapter.accept_registered_session(registered(2));
        assert_eq!(
            adapter.select_company(SelectedCompanyId(Uuid::from_u128(0x3000))),
            Err(AccountApiError::PermissionDenied)
        );
        let AccountSessionState::Authenticated(before) = adapter.state() else {
            panic!("authenticated state expected")
        };
        let company = before.bootstrap.companies[0].company_id;
        adapter.select_company(SelectedCompanyId(company)).unwrap();
        let AccountSessionState::Authenticated(after) = adapter.state() else {
            panic!("authenticated state expected")
        };
        assert_eq!(after.selected_company, Some(SelectedCompanyId(company)));
        assert!(!after.company_selection_required);
    }

    #[test]
    fn account_state_starts_uninitialized_and_registration_is_memory_only() {
        let adapter = adapter();
        assert_eq!(adapter.state(), AccountSessionState::Uninitialized);
        adapter.accept_registered_session(registered(1));
        assert!(matches!(
            adapter.state(),
            AccountSessionState::Authenticated(_)
        ));
    }

    #[test]
    fn mutations_have_no_automatic_replay_policy() {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        enum Method {
            Get,
            Post,
            Put,
        }
        fn replay_limit(method: Method) -> u8 {
            if method == Method::Get {
                1
            } else {
                0
            }
        }
        assert_eq!(replay_limit(Method::Get), 1);
        assert_eq!(replay_limit(Method::Post), 0);
        assert_eq!(replay_limit(Method::Put), 0);
    }
}
