//! Auth state management — stores JWT session in localStorage.

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "yarbot_auth";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthState {
    pub access_token: String,
    pub employee_id: i64,
    pub org_id: i64,
    pub name: String,
    pub is_admin: bool,
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
