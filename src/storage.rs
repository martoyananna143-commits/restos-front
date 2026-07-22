//! LocalStorage persistence for form answers using gloo-storage

use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::AnswerValue;

const STORAGE_KEY_PREFIX: &str = "rest_form_";

/// Saved answer state
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SavedAnswer {
    pub value: Option<AnswerValue>,
    pub comment: Option<String>,
}

/// Saved form state
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedFormState {
    pub answers: HashMap<i64, SavedAnswer>,
}

fn get_storage_key(token: &str) -> String {
    // Use first 16 chars of token as key to keep it short
    let short_token = &token[..token.len().min(16)];
    format!("{}{}", STORAGE_KEY_PREFIX, short_token)
}

/// Storage entry for persistent state
struct StorageEntry<T> {
    key: String,
    value: T,
}

/// Persistent storage hook for form data
pub struct UsePersistent<T: 'static> {
    inner: Signal<StorageEntry<T>>,
}

impl<T> Clone for UsePersistent<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for UsePersistent<T> {}

impl<T: Serialize + DeserializeOwned + Clone + 'static> UsePersistent<T> {
    /// Returns a clone of the value
    pub fn get(&self) -> T {
        self.inner.read().value.clone()
    }

    /// Sets the value and persists to localStorage
    pub fn set(&mut self, value: T) {
        let mut inner = self.inner.write();
        let _ = LocalStorage::set(inner.key.as_str(), &value);
        inner.value = value;
    }
    
    /// Clear the storage entry
    pub fn clear(&self) {
        let key = &self.inner.read().key;
        LocalStorage::delete(key);
    }
}

/// Create a persistent storage hook for form answers
pub fn use_form_storage(token: &str) -> UsePersistent<SavedFormState> {
    let key = get_storage_key(token);
    let state = use_signal(move || {
        let value: SavedFormState = LocalStorage::get(key.as_str())
            .ok()
            .unwrap_or_default();
        StorageEntry { key, value }
    });
    
    UsePersistent { inner: state }
}

/// Simple functions for direct storage access (backwards compatibility)
pub fn save_form_state(token: &str, state: &SavedFormState) {
    let key = get_storage_key(token);
    let _ = LocalStorage::set(key.as_str(), state);
}

pub fn load_form_state(token: &str) -> Option<SavedFormState> {
    let key = get_storage_key(token);
    LocalStorage::get(key.as_str()).ok()
}

pub fn clear_form_state(token: &str) {
    let key = get_storage_key(token);
    LocalStorage::delete(key.as_str());
}
