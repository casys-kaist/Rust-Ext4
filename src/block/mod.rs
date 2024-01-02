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

mod allocator;

use crate::block_group::BlockGroup;
use crate::cache::Cache;
use crate::filesystem::FileSystem;
use crate::superblock::SuperBlock;
use crate::transaction::{Collector, Transaction};
use crate::types::Zero;
use crate::{
    BlockGroupId, Config, FsError, InodeNumber, LogicalBlockNumber, RwLock, RwLockReadGuard,
    RwLockWriteGuard,
};
use alloc::sync::Arc;

pub(crate) type Block<B, D> = RwLock<B, D>;
pub(crate) struct BlockRef<'a, 'b, C: Config, const BLK_SIZE: usize, const MUT: bool> {
    lba: LogicalBlockNumber,
    inner: Arc<Block<C::Buffer<BLK_SIZE>, C::D>>,
    collector: Option<&'b Collector>,
    _lt: core::marker::PhantomData<&'a ()>,
}

impl<'a, 'b, C: Config, const BLK_SIZE: usize, const MUT: bool> BlockRef<'a, 'b, C, BLK_SIZE, MUT> {
    #[inline]
    pub fn lba(&self) -> LogicalBlockNumber {
        self.lba
    }

    #[inline]
    pub fn read(&self) -> RwLockReadGuard<C::Buffer<BLK_SIZE>, C::D> {
        self.inner.read()
    }
}

impl<'a, 'b, C: Config, const BLK_SIZE: usize> BlockRef<'a, 'b, C, BLK_SIZE, true> {
    #[inline]
    pub fn write<'c>(&'c self) -> BlockRefWriteGuard<'b, 'c, C, BLK_SIZE> {
        BlockRefWriteGuard {
            collector: self.collector.as_ref().cloned(),
            lba: self.lba,
            inner: self.inner.write(),
        }
    }
}

pub struct BlockRefWriteGuard<'a, 'b, C: Config, const BLK_SIZE: usize> {
    collector: Option<&'a Collector>,
    lba: LogicalBlockNumber,
    inner: RwLockWriteGuard<'b, C::Buffer<BLK_SIZE>, C::D>,
}

impl<'a, 'b, C: Config, const BLK_SIZE: usize> core::ops::Deref
    for BlockRefWriteGuard<'a, 'b, C, BLK_SIZE>
{
    type Target = C::Buffer<BLK_SIZE>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, 'b, C: Config, const BLK_SIZE: usize> core::ops::DerefMut
    for BlockRefWriteGuard<'a, 'b, C, BLK_SIZE>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        if let Some(collector) = self.collector.take() {
            collector.track(self.lba);
        }
        &mut self.inner
    }
}

fn find_one(bitmap: &[u8], start: usize) -> Option<usize> {
    let (s_grp, s_ofs) = (start >> 3, start & 7);
    for (group, b) in bitmap.iter().enumerate().skip(s_grp) {
        let b = if group == s_grp {
            b & !((1 << s_ofs) - 1)
        } else {
            *b
        };
        if b != 0 {
            return Some(group * 8 + 7 - (b & !(b - 1)).leading_zeros() as usize);
        }
    }
    None
}

fn find_zero(bitmap: &[u8], start: usize) -> Option<usize> {
    let (s_grp, s_ofs) = (start >> 3, start & 7);
    for (group, b) in bitmap.iter().enumerate().skip(s_grp) {
        let b = if group == s_grp {
            b | ((1 << s_ofs) - 1)
        } else {
            *b
        } ^ u8::MAX;
        if b != 0 {
            return Some(group * 8 + 7 - (b & !(b - 1)).leading_zeros() as usize);
        }
    }
    None
}

pub struct Manager<C: Config, const BLK_SIZE: usize> {
    pub(crate) allocator: allocator::Allocator<C::S>,
    pub(crate) blocks: Cache<LogicalBlockNumber, Block<C::Buffer<BLK_SIZE>, C::D>, C::D>,
    pub(crate) conf: C,
}

type ReadOutput<Buffer, D> = Result<Arc<Block<Buffer, D>>, FsError>;

impl<C: Config, const BLK_SIZE: usize> Manager<C, BLK_SIZE> {
    #[inline]
    pub fn new(conf: C) -> Self {
        Self {
            allocator: allocator::Allocator::new(BLK_SIZE),
            blocks: Cache::new(),
            conf,
        }
    }

    pub(crate) fn build_buddy(
        &self,
        bg: &BlockGroup<BLK_SIZE>,
        sb: &SuperBlock<C, BLK_SIZE>,
    ) -> Result<(), FsError> {
        let bblock = self.get(bg.block_bitmap_lba)?;
        let bitmap = bblock.read();
        let mut pos = 0;
        while pos < bg.blocks_count as usize {
            let first_zero = if let Some(p) = find_zero(bitmap.as_ref(), pos) {
                p
            } else {
                break;
            };
            let len = find_one(bitmap.as_ref(), first_zero).unwrap_or(bg.blocks_count as usize)
                - first_zero;
            pos = first_zero + len;
            self.allocator.push_chunk(
                LogicalBlockNumber::from_bgid_index(bg.bgid, first_zero, sb),
                len,
            );
        }
        Ok(())
    }

    #[inline]
    pub fn allocate(
        &self,
        ino: InodeNumber,
        size: usize,
        hope: LogicalBlockNumber,
        fs: &FileSystem<C, BLK_SIZE>,
        trans: &Transaction,
    ) -> Result<(LogicalBlockNumber, usize), FsError> {
        self.allocator.allocate(ino, fs, size, hope, trans)
    }

    #[inline]
    pub fn deallocate(
        &self,
        ino: InodeNumber,
        lba: LogicalBlockNumber,
        size: usize,
        fs: &FileSystem<C, BLK_SIZE>,
        trans: &Transaction,
    ) -> Result<(), FsError> {
        self.allocator.deallocate(ino, lba, size, fs, trans)
    }

    fn _read_contents(&self, lba: LogicalBlockNumber) -> ReadOutput<C::Buffer<BLK_SIZE>, C::D> {
        Ok(Arc::new(RwLock::new(
            self.conf
                .read_bytes::<BLK_SIZE>(lba.0 as usize * BLK_SIZE)?,
        )))
    }

    pub(crate) fn get<'j>(
        &self,
        lba: LogicalBlockNumber,
    ) -> Result<BlockRef<'_, 'j, C, BLK_SIZE, false>, FsError> {
        self.blocks
            .get_or_insert_arc(lba, |_| self._read_contents(lba))
            .map(|inner| BlockRef {
                lba,
                inner,
                collector: None,
                _lt: core::marker::PhantomData,
            })
    }

    #[inline]
    pub(crate) fn get_mut<'l, 'j>(
        &'l self,
        lba: LogicalBlockNumber,
        collector: &'j Collector,
    ) -> Result<BlockRef<'l, 'j, C, BLK_SIZE, true>, FsError>
    where
        'l: 'j,
    {
        self.blocks
            .get_or_insert_arc(lba, |_| self._read_contents(lba))
            .map(|inner| BlockRef {
                lba,
                inner,
                collector: Some(collector),
                _lt: core::marker::PhantomData,
            })
    }

    #[inline]
    pub(crate) fn get_mut_noload<'l, 'j>(
        &'l self,
        lba: LogicalBlockNumber,
        collector: &'j Collector,
    ) -> Result<BlockRef<'l, 'j, C, BLK_SIZE, true>, FsError>
    where
        'l: 'j,
    {
        let mut created = false;
        collector.track(lba);
        Ok(self
            .blocks
            .get_or_insert_arc::<_, ()>(lba, |_| {
                created = true;
                Ok(Arc::new(RwLock::new(C::Buffer::<BLK_SIZE>::zeroed())))
            })
            .map(|inner| BlockRef {
                lba,
                inner,
                collector: None,
                _lt: core::marker::PhantomData,
            })
            .unwrap())
    }

    pub fn get_io_request(
        &self,
        lba: LogicalBlockNumber,
    ) -> Result<(usize, C::Buffer<BLK_SIZE>), FsError> {
        if let Some(blk) = self.blocks.get(&lba) {
            let guard = blk.read();
            Ok((lba.0 as usize * BLK_SIZE, guard.clone()))
        } else {
            todo!();
            // FIXME: OK? or Error?
        }
    }

    pub(crate) fn apply_deallocation_on_core(
        &self,
        bgid: BlockGroupId,
        ofs: usize,
        count: usize,
        fs: &FileSystem<C, BLK_SIZE>,
    ) -> Result<(), FsError> {
        let guard = fs.get_block_group(bgid)?;
        let bg = guard.as_ref().unwrap();
        self.allocator.push_chunk(
            LogicalBlockNumber::from_bgid_index(bgid, ofs, &fs.sb),
            count,
        );
        bg.deallocate_blocks(count);
        Ok(())
    }
}
