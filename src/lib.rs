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

//! Ext4 implementation.
//!
//! <https://ext4.wiki.kernel.org/index.php/Ext4_Disk_Layout>

#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]
#![deny(unsafe_code)]
#![feature(array_chunks, min_specialization)]

#[macro_use]
mod prelude;
pub use prelude::*;

extern crate alloc;

#[macro_use]
mod inode;

// Impls.
mod block;
mod block_group;
mod cache;
mod crc;
mod directory;
mod file;
mod filesystem;
mod hasher;
mod superblock;
mod transaction;
mod types;
#[allow(dead_code)]
mod utils;

pub use crate::{directory::Directory, file::File, filesystem::FileSystem};
use alloc::sync::Arc;
pub use fs_core::{FileType, FsError, InodeMode};
pub use inode::{AddressingOutput, Inode};
pub use transaction::Transaction;
pub use types::{
    BlockGroupId, Config, FileBlockNumber, FsObject, InodeNumber, LogicalBlockNumber, Zero,
};

pub enum FsBlkSizeDispatch<C: Config> {
    Blk1024(Arc<FileSystem<C, 1024>>),
    Blk2048(Arc<FileSystem<C, 2048>>),
    Blk4096(Arc<FileSystem<C, 4096>>),
}

/// Open filesystem from io.
pub fn open_fs<C: Config, const N: usize>(conf: C) -> Result<Arc<FileSystem<C, N>>, FsError> {
    superblock::new_sb(&conf).map(|sb| FileSystem::new(conf, sb))
}

pub mod format;
