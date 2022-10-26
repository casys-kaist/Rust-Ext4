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

// Rework
mod cursor;

use crate::filesystem::FileSystem;
use crate::transaction::Transaction;
use crate::{Config, FileBlockNumber, FsError, InodeNumber};

pub(crate) use cursor::{Cursor, CursorMut};

#[derive(Debug, Clone, Default)]
pub struct Legacy {
    pub(super) addresses: [u32; 15],
}

impl Legacy {
    pub(crate) fn cursor_last_mut<'a, 'b, 'c, C: Config, const BLK_SIZE: usize>(
        &'a mut self,
        _ino: InodeNumber,
        _fs: &'b FileSystem<C, BLK_SIZE>,
        _tx: &'c Transaction,
    ) -> Result<CursorMut<'a, 'b, 'c, C, BLK_SIZE>, FsError> {
        todo!()
    }

    pub(crate) fn cursor_from_fba<'a, 'b, C: Config, const BLK_SIZE: usize>(
        &'a self,
        _ino: InodeNumber,
        fs: &'b FileSystem<C, BLK_SIZE>,
        fba: FileBlockNumber,
    ) -> Result<Cursor<'a, 'b, C, BLK_SIZE>, FsError> {
        Ok(Cursor {
            fba,
            direct: self,
            _fs: fs,
            _error: None,
        })
    }

    pub(crate) fn cursor_from_fba_mut<'a, 'b, 'c, C: Config, const BLK_SIZE: usize>(
        &'a mut self,
        ino: InodeNumber,
        fs: &'b FileSystem<C, BLK_SIZE>,
        fba: FileBlockNumber,
        tx: &'c Transaction,
    ) -> Result<CursorMut<'a, 'b, 'c, C, BLK_SIZE>, FsError> {
        Ok(CursorMut {
            fba,
            _ino: ino,
            direct: self,
            _fs: fs,
            _tx: tx,
            _error: None,
        })
    }
}
