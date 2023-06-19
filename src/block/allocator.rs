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
use crate::{BlockGroupId, Config, Dreamer, FsError, InodeNumber, LogicalBlockNumber, TicketLock};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

struct BlockGroupBuddy<S>
where
    S: Dreamer,
{
    max_order: usize,
    pub buddies: Vec<TicketLock<BTreeSet<usize>, S>>,
    // ofs -> order
    pub backref: TicketLock<BTreeMap<usize, usize>, S>,
}

impl<S> BlockGroupBuddy<S>
where
    S: Dreamer,
{
    fn new(max_order: usize) -> Self {
        Self {
            max_order,
            buddies: (0..max_order + 1)
                .map(|_| TicketLock::new(BTreeSet::new()))
                .collect(),
            backref: TicketLock::new(BTreeMap::new()),
        }
    }

    fn insert(&self, start: usize, order: usize) {
        // TODO: merge.
        let (mut backref, mut bucket) = (self.backref.lock(), self.buddies[order].lock());
        bucket.insert(start);
        backref.insert(start, order);
    }

    pub(super) fn push_chunk(&self, mut start: usize, mut size: usize) {
        while size > 0 {
            // possible orders: 0 .. BLK_SIZE::BITS + 2
            let order = core::cmp::min(
                core::cmp::min(
                    start.trailing_zeros(),
                    usize::BITS - 1 - size.leading_zeros(),
                ),
                self.max_order as u32,
            );
            self.insert(start, order as usize);
            start += 1 << order;
            size -= 1 << order;
        }
    }

    fn try_allocate(&self, size: usize, hope: Option<usize>) -> Option<(usize, usize)> {
        let mut backref = self.backref.lock();
        if let Some(hope) = hope {
            let mut chain = 0;
            while chain < size {
                if let Some(l) = backref.get(&(hope + chain)) {
                    chain += 1 << l;
                } else {
                    break;
                }
            }
            // if we can allocate more than size / 2 from hope, allocate from the hope.
            if chain > size / 2 {
                let allocated = core::cmp::min(chain, size);
                let mut p = 0;
                while p < allocated {
                    let order = backref.remove(&(hope + p)).unwrap();
                    assert!(self.buddies[order].lock().remove(&(hope + p)));
                    p += if p + (1 << order) <= chain {
                        1 << order
                    } else {
                        let size = chain - p;
                        self.push_chunk(p + size, (1 << order) - size);
                        size
                    };
                }
                debug_assert!(allocated <= size);
                return Some((hope, allocated));
            }
        }

        let min_fit_order = usize::BITS - (size - 1).leading_zeros();
        for (order, bucket) in self.buddies.iter().enumerate().skip(min_fit_order as usize) {
            let mut bucket = bucket.lock();
            if let Some(p) = bucket.iter().next().cloned() {
                backref.remove(&p);
                bucket.remove(&p);
                drop(bucket);
                drop(backref);
                self.push_chunk(p + size, (1 << order) - size);
                return Some((p, size));
            }
        }
        None
    }
}

/// A Block Allocator.
pub(crate) struct Allocator<S>
where
    S: Dreamer,
{
    buddies: Vec<BlockGroupBuddy<S>>,
}

impl<S> Allocator<S>
where
    S: Dreamer,
{
    pub fn new(blk_size: usize, bg: usize) -> Self {
        let max_order = (blk_size - 1).trailing_ones() as usize + 2;
        Self {
            buddies: (0..bg).map(|_| BlockGroupBuddy::new(max_order)).collect(),
        }
    }

    pub(crate) fn push_chunk(&self, start: usize, size: usize, bgid: BlockGroupId) {
        self.buddies[bgid.0 as usize].push_chunk(start, size)
    }

    pub fn allocate<C: Config, const BLK_SIZE: usize>(
        &self,
        ino: InodeNumber,
        fs: &FileSystem<C, BLK_SIZE>,
        size: usize,
        hope: LogicalBlockNumber,
        tx: &Transaction,
    ) -> Result<(LogicalBlockNumber, usize), FsError> {
        let mut size = core::cmp::min(BLK_SIZE * 4, size);
        while size > 0 {
            let (bgid, index) = hope
                .into_bgid_index(&fs.sb)
                .unwrap_or_else(|| panic!("hope: {:?}", hope));
            let mut hope = Some(index);

            for bgid in (0..self.buddies.len())
                .map(|id| BlockGroupId(id as u32))
                .cycle()
                .skip(bgid.0 as usize)
                .take(self.buddies.len())
            {
                let guard = fs.get_block_group(bgid)?;
                let (bg, bd) = (guard.as_ref().unwrap(), &self.buddies[bgid.0 as usize]);
                if let Some((index, allocated)) = bd.try_allocate(size, hope.take()) {
                    // TODO: bitmap update

                    bg.allocate_blocks(ino, index, allocated, tx);
                    return Ok((
                        LogicalBlockNumber::from_bgid_index(bgid, index, &fs.sb),
                        allocated,
                    ));
                }
            }

            size /= 2;
        }
        Err(FsError::FsFull)
    }

    pub fn deallocate<C: Config, const BLK_SIZE: usize>(
        &self,
        ino: InodeNumber,
        mut lba: LogicalBlockNumber,
        mut size: usize,
        fs: &FileSystem<C, BLK_SIZE>,
        trans: &Transaction,
    ) -> Result<(), FsError> {
        // Here the buddy state is not be updated. Just make a transation on here.
        // When writeback is finished, the buddy state will be updated.
        while size > 0 {
            let (bgid, ofs) = lba.into_bgid_index(&fs.sb).unwrap();
            let guard = fs.get_block_group(bgid)?;
            let bg = guard.as_ref().unwrap();
            let count = core::cmp::min(bg.blocks_count as usize - ofs, size);
            trans.block_deallocation_on_bg(ino, bgid, bg.block_bitmap_lba, ofs, count);
            size -= count;
            lba = lba + (count as u64);
        }
        Ok(())
    }
}
