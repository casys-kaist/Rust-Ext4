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

//! no_std generic implementations of the synchromization primitives.
#![no_std]

pub mod rwlock;
pub mod simple_lock;
pub mod ticket_lock;

/// Traits for supporting constant creation of the lock.
pub trait HasConstDefault {
    /// Default value.
    const DEFAULT: Self;
}

/// The lock could not be acquired at this time because the operation would
/// otherwise block.
#[derive(Debug)]
pub struct WouldBlock;
