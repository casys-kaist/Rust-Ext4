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

use crate::filesystem::FileSystem;
use crate::transaction::Transaction;
use crate::types::BlockGroupId;
use crate::{Config, FileType, FsError, InodeNumber, RwLock, RwLockReadGuard};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

/// Allocator
pub(crate) struct Allocator<C: Config> {
    last_inode_bg_id: AtomicU32,
    pub(super) bitmap: Vec<RwLock<Option<Box<[AtomicU8]>>, C::D>>,
}

impl<C: Config> Allocator<C> {
    pub fn new(bg: usize) -> Self {
        Self {
            last_inode_bg_id: AtomicU32::new(0),
            bitmap: (0..bg).map(|_| RwLock::new(None)).collect(),
        }
    }

    pub fn bitmap<const BLK_SIZE: usize>(
        &self,
        bgid: BlockGroupId,
        fs: &FileSystem<C, BLK_SIZE>,
    ) -> Result<RwLockReadGuard<Option<Box<[AtomicU8]>>, C::D>, FsError> {
        let guard = self.bitmap[bgid.0 as usize].read();
        if guard.is_some() {
            Ok(guard)
        } else {
            drop(guard);
            let mut guard = self.bitmap[bgid.0 as usize].write();
            if guard.is_none() {
                let bg = fs.get_block_group(bgid);
                let bblock = fs.blocks.get(bg.inode_bitmap_lba)?;
                *guard = Some(bblock.read().iter().cloned().map(AtomicU8::new).collect());
            }
            drop(guard);
            Ok(self.bitmap[bgid.0 as usize].read())
        }
    }

    pub fn try_allocate_at<const BLK_SIZE: usize>(
        &self,
        i: u32,
        fs: &FileSystem<C, BLK_SIZE>,
    ) -> Result<Option<InodeNumber>, FsError> {
        let ino = InodeNumber(i);
        let (bgid, idx) = ino.into_bgid_index(&fs.sb);
        let (group, ofs) = (idx / 8, idx & 7);

        let guard = self.bitmap(bgid, fs)?;
        if guard.as_ref().unwrap()[group].fetch_or(1 << ofs, Ordering::Relaxed) & (1 << ofs) == 0 {
            Ok(Some(ino))
        } else {
            Ok(None)
        }
    }

    pub fn get_free_ino<const BLK_SIZE: usize>(
        &self,
        bgid: BlockGroupId,
        fs: &FileSystem<C, BLK_SIZE>,
    ) -> Result<Option<usize>, FsError> {
        let guard = self.bitmap(bgid, fs)?;
        for (group, bits) in guard.as_ref().unwrap().iter().enumerate() {
            loop {
                // CAS to get bits.
                let val = bits.load(Ordering::Relaxed);
                let x = val ^ core::u8::MAX;
                if x != 0 {
                    // toggle all bits.
                    let (mask, ret) = {
                        let pos = 7 - (x & !(x - 1)).leading_zeros() as usize;
                        //println!(
                        //    "{:08b} {}\n{:08b}",
                        //    val,
                        //    pos + ofs * 8,
                        //    1 << pos,
                        //);
                        (1 << pos, group * 8 + pos)
                    };
                    // Check whether previous value does not hold the one on the position.
                    if bits.fetch_or(mask, Ordering::Relaxed) & mask == 0 {
                        return Ok(Some(ret));
                    }
                } else {
                    break;
                }
            }
        }
        Ok(None)
    }

    pub fn allocate<const BLK_SIZE: usize>(
        &self,
        fs: &FileSystem<C, BLK_SIZE>,
        de: FileType,
        trans: &Transaction,
    ) -> Result<InodeNumber, FsError> {
        if fs.sb.get_free_inodes_count() == 0 {
            return Err(FsError::FsFull);
        }

        let bgid = self.last_inode_bg_id.load(Ordering::Acquire);
        for bgid in (0..fs.sb.bg_count)
            .map(BlockGroupId)
            .cycle()
            .skip(bgid as usize)
            .take(fs.sb.bg_count as usize)
        {
            let bg = fs.get_block_group(bgid);
            if bg.get_free_inodes_count() > 0 {
                if let Some(ofs) = self.get_free_ino(bgid, fs)? {
                    bg.allocate_inode_on_bg(ofs as u32, trans, de);
                    fs.sb.dec_free_inodes_count(trans);
                    self.last_inode_bg_id.store(bgid.0, Ordering::Release);

                    return Ok(InodeNumber::from_bgid_index(bgid, ofs, &fs.sb));
                }
            }
        }

        Err(FsError::FsFull)
    }

    pub fn deallocate<const BLK_SIZE: usize>(
        &self,
        fs: &FileSystem<C, BLK_SIZE>,
        ino: InodeNumber,
        de: FileType,
        trans: &Transaction,
    ) {
        let (bgid, ofs) = ino.into_bgid_index(&fs.sb);
        let bg = fs.get_block_group(bgid);
        trans.inode_deallocation_on_bg(ino, bgid, bg.inode_bitmap_lba, ofs as usize, de);
        trans.free_inodes_count_inc_on_sb();
    }
}
