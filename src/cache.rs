// Copyright 2021 Computer Architecture and Systems Lab
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{RwDreamer, RwLock};
use alloc::sync::Arc;
use hashbrown::{hash_map::Entry, HashMap};

pub struct Cache<K, V, D>
where
    K: core::hash::Hash + Eq + Clone + core::fmt::Debug + Send,
    V: Send + Sync,
    D: RwDreamer,
{
    inner: RwLock<HashMap<K, Arc<V>>, D>,
}

impl<K, V, D> Cache<K, V, D>
where
    K: core::hash::Hash + Eq + Clone + core::fmt::Debug + Send,
    V: Send + Sync,
    D: RwDreamer,
{
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    #[inline]
    pub fn get(&self, k: &K) -> Option<Arc<V>> {
        self.inner.read().get(k).cloned()
    }

    #[inline]
    pub fn get_or_insert<F, E>(&self, k: K, f: F) -> Result<Arc<V>, E>
    where
        F: FnOnce(K) -> Result<V, E>,
    {
        if let Some(v) = self.get(&k) {
            Ok(v)
        } else {
            match self.inner.write().entry(k.clone()) {
                Entry::Vacant(e) => f(k).map(|v| e.insert(Arc::new(v)).clone()),
                Entry::Occupied(e) => Ok(e.into_mut().clone()),
            }
        }
    }

    #[inline]
    pub fn get_or_insert_arc<F, E>(&self, k: K, f: F) -> Result<Arc<V>, E>
    where
        F: FnOnce(K) -> Result<Arc<V>, E>,
    {
        if let Some(v) = self.get(&k) {
            Ok(v)
        } else {
            match self.inner.write().entry(k.clone()) {
                Entry::Vacant(e) => f(k).map(|v| e.insert(v).clone()),
                Entry::Occupied(e) => Ok(e.into_mut().clone()),
            }
        }
    }

    #[inline]
    pub fn take(&self, k: &K) -> Option<Arc<V>> {
        self.inner.write().remove(k)
    }

    #[inline]
    pub fn _flush(&self) {
        *self.inner.write() = HashMap::new();
    }
}
