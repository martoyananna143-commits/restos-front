//! Auth state management — stores JWT session in localStorage.

use crate::types::OrgInfo;
use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "restos_auth";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthState {
    pub access_token: String,
    pub employee_id: i64,
    pub org_id: i64,
    pub name: String,
    pub is_admin: bool,
    #[serde(default)]
    pub is_superuser: bool,
    #[serde(default)]
    pub available_orgs: Vec<OrgInfo>,
}

impl AuthState {
    pub fn load() -> Option<Self> {
        LocalStorage::get(STORAGE_KEY).ok()
    }

    pub fn save(&self) {
        let _ = LocalStorage::set(STORAGE_KEY, self);
    }

    pub fn clear() {
        LocalStorage::delete(STORAGE_KEY);
    }
}
