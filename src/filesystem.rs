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

use crate::block;
use crate::block_group::BlockGroup;
use crate::directory::Directory;
use crate::inode;
use crate::superblock::SuperBlock;
use crate::transaction::{Events, Transaction};
use crate::types::BlockGroupId;
use crate::{std::RwLock, std::RwLockReadGuard, Config, FileType, FsError, FsObject, InodeNumber};
use alloc::collections::LinkedList;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;

pub struct FileSystem<C: Config, const BLK_SIZE: usize> {
    pub(crate) sb: SuperBlock<C, BLK_SIZE>,

    block_groups: Vec<RwLock<Option<BlockGroup<BLK_SIZE>>, C::D>>,

    pub(crate) blocks: block::Manager<C, BLK_SIZE>,
    pub(crate) inodes: inode::Manager<C, BLK_SIZE>,
}

impl<C: Config, const BLK_SIZE: usize> FileSystem<C, BLK_SIZE> {
    #[inline]
    pub fn conf(&self) -> &C {
        &self.blocks.conf
    }

    #[inline]
    pub fn conf_mut(&mut self) -> &mut C {
        &mut self.blocks.conf
    }

    pub fn into_inner(self) -> C {
        self.blocks.conf
    }

    #[inline]
    pub fn get_inode_as_fs_object(
        self: &Arc<Self>,
        ino: InodeNumber,
        hint: Option<FileType>,
    ) -> Result<FsObject<C, BLK_SIZE>, FsError> {
        self.inodes.get(self, ino, hint).map(|n| n.into_fs_object())
    }

    #[inline]
    pub fn allocate_inode_as_fs_object<'a>(
        self: &'a Arc<Self>,
        ftype: FileType,
        tx: &Transaction,
    ) -> Result<(InodeNumber, FsObject<C, BLK_SIZE>), crate::FsError> {
        self.inodes
            .allocate(self, ftype, tx)
            .map(|(ino, n)| (ino, n.into_fs_object()))
    }

    #[inline]
    pub fn root(self: &Arc<Self>) -> Result<Directory<C, BLK_SIZE>, FsError> {
        const EXT4_INODE_ROOT_INDEX: InodeNumber = InodeNumber(2);

        self.inodes
            .get(self, EXT4_INODE_ROOT_INDEX, None)?
            .into_fs_object()
            .get_directory()
            .ok_or(FsError::NotDirectory)
    }

    #[inline]
    pub fn get_block_group(
        &self,
        bgid: BlockGroupId,
    ) -> Result<RwLockReadGuard<Option<BlockGroup<BLK_SIZE>>, C::D>, FsError> {
        let guard = self.block_groups[bgid.0 as usize].read();
        if guard.is_some() {
            Ok(guard)
        } else {
            drop(guard);
            let mut guard = self.block_groups[bgid.0 as usize].write_no_dep();
            if guard.is_none() {
                let (lba, index) = bgid.into_lba_index(&self.sb);
                let tx = self.open_transaction();
                let bg_arr = self.blocks.get_mut(lba, &tx.collector)?;
                *guard = Some(BlockGroup::from_disk(
                    bg_arr,
                    index..index + self.sb.block_desc_size,
                    bgid,
                    self,
                    &tx,
                )?);
                tx.done(self)?;
                // We don't need to hold blockgroup blocks, as they are loaded on memory.
                self.blocks.build_buddy(guard.as_ref().unwrap(), &self.sb)?;
            }
            drop(guard);
            Ok(self.block_groups[bgid.0 as usize].read())
        }
    }

    // Fixme: Lock
    #[inline]
    pub fn open_transaction(&self) -> Transaction {
        Transaction {
            events: Events {
                inner: Some(RefCell::new(LinkedList::new())),
            },
            collector: crate::transaction::Collector::default(),
        }
    }

    /// Open a file sytem from the device `IO`.
    pub fn new(conf: C, sb: SuperBlock<C, BLK_SIZE>) -> Arc<FileSystem<C, BLK_SIZE>> {
        let blocks = block::Manager::new(conf);
        let inodes = inode::Manager::new(&sb);
        let bg_count = sb.bg_count;
        let fs = Arc::new(FileSystem {
            sb,
            blocks,
            block_groups: (0..bg_count).map(|_| RwLock::new(None)).collect(),
            inodes,
        });
        for i in 0..bg_count {
            let _ = fs.get_block_group(BlockGroupId(i));
        }
        fs
    }
}
