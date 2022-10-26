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

mod raw;

use crate::crc::{Crc16, Crc32c};
use crate::filesystem::FileSystem;
use crate::transaction::Transaction;
use crate::types::BlockGroupId;

use crate::superblock::{Ext4FeatureReadOnly, SuperBlock};
use crate::{Config, FileType, FsError, InodeNumber, LogicalBlockNumber};
use bitflags::bitflags;

pub use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
pub(crate) use raw::Manipulator;

bitflags! {
    pub struct BlockGroupFlag: u16 {
        /// Inode table/bitmap not in use
        const INODE_UNINIT = 0x0001;
        /// Block bitmap not in use
        const BLOCK_UNINIT = 0x0002;
        /// On-disk itable initialized to zero
        const ITABLE_ZEROED = 0x0004;
    }
}

pub struct BlockGroup<const BLK_SIZE: usize> {
    // Readonly
    pub bgid: BlockGroupId,

    pub inode_table_first_block: LogicalBlockNumber,
    pub block_bitmap_lba: LogicalBlockNumber,
    pub inode_bitmap_lba: LogicalBlockNumber,

    pub blocks_count: u32,

    // Rw
    free_blocks_count: AtomicU32,
    free_inodes_count: AtomicU32,

    // FIXME
    _itable_unused: AtomicU32,
}

impl<const BLK_SIZE: usize> BlockGroup<BLK_SIZE> {
    pub(crate) fn calculate_csum<C: Config, T>(
        bgid: BlockGroupId,
        manipulator: &mut Manipulator<T>,
        sb: &SuperBlock<C, BLK_SIZE>,
    ) -> u16
    where
        T: core::convert::AsRef<[u8]>,
        T: core::convert::AsMut<[u8]>,
    {
        if sb
            .features_readonly
            .contains(Ext4FeatureReadOnly::METADATA_CSUM)
        {
            let orig = manipulator.checksum().get();
            manipulator.checksum().set(0);

            let mut crc = Crc32c::default();
            crc.write(&sb.uuid);
            crc.write(&bgid.0.to_le_bytes());
            crc.write(manipulator.rw.inner().as_ref());

            manipulator.checksum().set(orig);

            crc.finish() as u16
        } else if sb.features_readonly.contains(Ext4FeatureReadOnly::GDT_CSUM) {
            let bytes = manipulator.rw.inner().as_ref();
            let mut crc = Crc16::default();
            crc.write(&sb.uuid);
            crc.write(&bgid.0.to_le_bytes());
            crc.write(&bytes[..0x1E]);
            if bytes.len() > 0x20 {
                crc.write(&bytes[0x20..]);
            }
            crc.finish()
        } else {
            0
        }
    }

    #[inline]
    pub fn allocate_inode_on_bg(&self, ofs: u32, trans: &Transaction, de: FileType) {
        self.free_inodes_count.fetch_sub(1, Ordering::Relaxed);
        trans.inode_allocation_on_bg(self.bgid, self.inode_bitmap_lba, ofs as usize, de);
    }

    #[inline]
    pub fn deallocate_inode(&self) {
        self.free_inodes_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn get_free_inodes_count(&self) -> u32 {
        self.free_inodes_count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn get_free_blocks_count(&self) -> u32 {
        self.free_blocks_count.load(Ordering::Acquire)
    }

    #[inline]
    pub fn allocate_blocks(&self, ino: InodeNumber, ofs: usize, count: usize, trans: &Transaction) {
        self.free_blocks_count
            .fetch_sub(count as u32, Ordering::Release);
        trans.block_allocation_on_bg(ino, self.bgid, self.block_bitmap_lba, ofs, count);
    }

    #[inline]
    pub fn deallocate_blocks(&self, count: usize) {
        self.free_blocks_count
            .fetch_add(count as u32, Ordering::Release);
    }

    pub fn from_disk<C: Config>(
        raw: &[u8],
        bgid: BlockGroupId,
        fs: &FileSystem<C, BLK_SIZE>,
    ) -> Result<Self, FsError> {
        let mut manipulator = Manipulator::new(raw);
        let blocks_count = match bgid.0.cmp(&fs.sb.bg_count) {
            core::cmp::Ordering::Less => fs.sb.blocks_per_group,
            core::cmp::Ordering::Equal => {
                (fs.sb.blocks_count - (fs.sb.blocks_per_group as u64 * (bgid.0 as u64 - 1))) as u32
            }
            core::cmp::Ordering::Greater => {
                panic!("{:?}", FsError::InvalidFs("bgid > sb.bg_count"))
            }
        };

        BlockGroup {
            bgid,
            inode_table_first_block: LogicalBlockNumber(manipulator.inode_table().get()),
            block_bitmap_lba: LogicalBlockNumber(manipulator.block_bitmap().get()),
            inode_bitmap_lba: LogicalBlockNumber(manipulator.inode_bitmap().get()),
            blocks_count,

            free_blocks_count: AtomicU32::new(manipulator.free_blocks_count().get()),
            free_inodes_count: AtomicU32::new(manipulator.free_inodes_count().get()),

            _itable_unused: AtomicU32::new(manipulator.itable_unused().get()),
        }
        .verify(
            BlockGroupFlag::from_bits_truncate(manipulator.flags().get()),
            manipulator.checksum().get(),
        )
    }

    pub fn verify(mut self, flags: BlockGroupFlag, _csum: u16) -> Result<Self, FsError> {
        self.initialize_bg(flags)?;
        Ok(self)
        /*
        if self.free_blocks_count.load(Ordering::Acquire)
            == self.blocks_count - self.block_bitmap.count_used()
        {
            Ok(self)
        } else {
            Err(FsError::InvalidFs("Bg is corrupted"))
        }
        */
    }

    pub fn initialize_bg(&mut self, flags: BlockGroupFlag) -> Result<(), FsError> {
        if flags.contains(BlockGroupFlag::BLOCK_UNINIT) {
            todo!()
            /* rc = ext4_fs_init_block_bitmap(ref);
                if (rc != EOK) {
                    ext4_block_set(fs->bdev, &ref->block);
                    return rc;
                }
                ext4_bg_clear_flag(bg, EXT4_BLOCK_GROUP_BLOCK_UNINIT);
                ref->dirty = true;
            */
        }

        if flags.contains(BlockGroupFlag::INODE_UNINIT) {
            todo!()
            /*
            rc = ext4_fs_init_inode_bitmap(ref);
            if (rc != EOK) {
                ext4_block_set(ref->fs->bdev, &ref->block);
                return rc;
            }

            ext4_bg_clear_flag(bg, EXT4_BLOCK_GROUP_INODE_UNINIT);

            if (!ext4_bg_has_flag(bg, EXT4_BLOCK_GROUP_ITABLE_ZEROED)) {
                rc = ext4_fs_init_inode_table(ref);
                if (rc != EOK) {
                    ext4_block_set(fs->bdev, &ref->block);
                    return rc;
                }

                ext4_bg_set_flag(bg, EXT4_BLOCK_GROUP_ITABLE_ZEROED);
            }

            ref->dirty = true;
            */
        }
        // Apply to disk.
        Ok(())
    }
}
