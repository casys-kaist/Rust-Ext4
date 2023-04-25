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

use crate::inode::Inode;
use crate::Config;
use alloc::sync::Arc;
use path::PathBuf;

pub struct Symlink<C: Config, const BLK_SIZE: usize> {
    pub(crate) inode: Arc<Inode<C, BLK_SIZE>>,
}

impl<C: Config, const BLK_SIZE: usize> Symlink<C, BLK_SIZE> {
    pub fn readlink(&self) -> PathBuf {
        let size = self.inode.get_size() as usize;
        if size <= 60 {
            PathBuf::from(
                core::str::from_utf8(&self.inode.rw.read().addresses.to_inline_data()[..size])
                    .unwrap(),
            )
        } else {
            todo!()
        }
    }
}
