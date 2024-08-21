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

#[repr(transparent)]
pub struct RwLock<V, D> {
    inner: std::sync::RwLock<V>,
    _p: core::marker::PhantomData<D>,
}
#[repr(transparent)]
pub struct RwLockReadGuard<'a, V, D> {
    inner: std::sync::RwLockReadGuard<'a, V>,
    _p: core::marker::PhantomData<D>,
}
impl<'a, V, D> std::ops::Deref for RwLockReadGuard<'a, V, D> {
    type Target = V;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}
#[repr(transparent)]
pub struct RwLockWriteGuard<'a, V, D> {
    inner: std::sync::RwLockWriteGuard<'a, V>,
    _p: std::marker::PhantomData<D>,
}
impl<'a, V, D> std::ops::Deref for RwLockWriteGuard<'a, V, D> {
    type Target = V;
    fn deref(&self) -> &Self::Target {
        &*self.inner
    }
}
impl<'a, V, D> std::ops::DerefMut for RwLockWriteGuard<'a, V, D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.inner
    }
}
impl<V, D> RwLock<V, D> {
    #[inline]
    pub fn new(inner: V) -> Self {
        Self {
            inner: std::sync::RwLock::new(inner),
            _p: std::marker::PhantomData,
        }
    }
    #[inline]
    pub fn write(&self) -> RwLockWriteGuard<V, D> {
        RwLockWriteGuard {
            inner: self.inner.write().unwrap(),
            _p: std::marker::PhantomData,
        }
    }
    #[inline]
    pub fn write_no_dep(&self) -> RwLockWriteGuard<V, D> {
        RwLockWriteGuard {
            inner: self.inner.write().unwrap(),
            _p: std::marker::PhantomData,
        }
    }
    #[inline]
    pub fn read(&self) -> RwLockReadGuard<V, D> {
        RwLockReadGuard {
            inner: self.inner.read().unwrap(),
            _p: std::marker::PhantomData,
        }
    }
    #[inline]
    pub fn try_write(&self) -> Result<RwLockWriteGuard<V, D>, ()> {
        self.inner
            .try_write()
            .map(|inner| RwLockWriteGuard {
                inner,
                _p: std::marker::PhantomData,
            })
            .map_err(|_| ())
    }
    #[inline]
    pub fn try_read(&self) -> Result<RwLockReadGuard<V, D>, ()> {
        self.inner
            .try_read()
            .map(|inner| RwLockReadGuard {
                inner,
                _p: std::marker::PhantomData,
            })
            .map_err(|_| ())
    }
}

#[doc(hidden)]
#[derive(Default, Clone)]
pub struct SpinningDreamer {}

use core::sync::atomic::AtomicU32;

use fs_core::FsError;

use crate::Config;

impl synchronizations::ticket_lock::Dreamer for SpinningDreamer {
    const DEFAULT: Self = Self {};
    type HookAux = ();
    #[inline]
    fn sleeping(&self, state: &AtomicU32, ticket: u32) {
        while state.load(core::sync::atomic::Ordering::Relaxed) != ticket {
            core::hint::spin_loop();
        }
    }
    #[inline]
    fn waking_up(&self) {}
    #[inline]
    fn prehook(&self) -> Self::HookAux {}
    #[inline]
    fn acquire_hook(&self) {}
    #[inline]
    fn release_hook(&self) {}
}

/// A Opaque dreamer for RwLock.
///
/// We will not use the implementation of synchromization::RwLock on std.
/// This is opaque object that implements synchronization::Dreamer<State=AtomicUsize>.
#[doc(hidden)]
#[derive(Default, Clone)]
pub struct OpaqueDreamer {}

impl synchronizations::rwlock::Dreamer for OpaqueDreamer {
    const DEFAULT: Self = Self {};
    type HookAux = ();

    #[inline]
    fn sleeping(&self, _s: &core::sync::atomic::AtomicUsize, _h: synchronizations::rwlock::Hint) {
        unreachable!()
    }
    #[inline]
    fn waking_up(&self) {
        unreachable!()
    }
    #[inline]
    fn prehook(&self) -> Self::HookAux {
        unreachable!()
    }
    #[inline]
    fn acquire_hook(&self, _h: synchronizations::rwlock::Hint, _d: bool) {
        unreachable!()
    }
    #[inline]
    fn release_hook(&self, _h: synchronizations::rwlock::Hint) {
        unreachable!()
    }
}

#[cfg(target_family = "unix")]
impl Config for std::fs::File {
    type D = crate::std::OpaqueDreamer;
    type S = crate::std::SpinningDreamer;
    type Buffer<const N: usize> = Box<[u8; N]>;

    fn read_bytes<const N: usize>(&self, ofs: usize) -> Result<Box<[u8; N]>, FsError> {
        use std::os::unix::fs::FileExt;
        let mut b = Box::new([0; N]);
        self.read_at(b.as_mut(), ofs as u64)
            .map_err(|_| FsError::IoError)
            .map(|_| b)
    }

    fn write_bytes<const N: usize>(
        &self,
        ofs: usize,
        buf: &Self::Buffer<N>,
    ) -> Result<(), FsError> {
        use std::os::unix::fs::FileExt;
        self.write_at(buf.as_ref(), ofs as u64)
            .map_err(|_| FsError::IoError)
            .map(|_| ())?;
        self.sync_data().map_err(|_| FsError::IoError)
    }
    fn write_vectored<const N: usize>(
        &self,
        mut ofs: usize,
        bufs: &[&Self::Buffer<N>],
    ) -> Result<(), FsError> {
        for buf in bufs {
            use std::os::unix::fs::FileExt;
            self.write_at(buf.as_ref(), ofs as u64)
                .map_err(|_| FsError::IoError)?;
            ofs += buf.as_ref().len();
        }
        self.sync_data().map_err(|_| FsError::IoError)?;
        Ok(())
    }
    fn write_bios<const N: usize>(
        &self,
        bio: Vec<(usize, Self::Buffer<N>)>,
    ) -> Result<(), FsError> {
        for (ofs, buf) in bio.into_iter() {
            use std::os::unix::fs::FileExt;
            self.write_at(buf.as_ref(), ofs as u64)
                .map_err(|_| FsError::IoError)
                .map(|_| ())?;
        }
        self.sync_data().map_err(|_| FsError::IoError)
    }
    fn total_size(&self) -> usize {
        use std::io::{Seek, SeekFrom};
        use std::os::unix::fs::MetadataExt;

        let meta = self.metadata().unwrap();
        if meta.rdev() == 0 {
            meta.len() as usize
        } else {
            self.try_clone().unwrap().seek(SeekFrom::End(0)).unwrap() as usize
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::format;
    use rand::distributions::{Alphanumeric, Distribution};
    use rand::{thread_rng, Rng};
    use std::fs::OpenOptions;
    use std::path::PathBuf;

    struct UpperHex;

    impl Distribution<u8> for UpperHex {
        fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> u8 {
            const RANGE: u32 = 16;
            const GEN_ASCII_STR_CHARSET: &[u8] = b"0123456789ABCDEF";
            loop {
                let var = rng.next_u32() >> (32 - 4);
                if var < RANGE {
                    return GEN_ASCII_STR_CHARSET[var as usize];
                }
            }
        }
    }

    pub(crate) fn test_oracle(disk: &std::path::Path) -> bool {
        use std::process::Command;

        let output = Command::new("fsck.ext4")
            .args(["-f", "-n", disk.to_str().unwrap()])
            .output()
            .unwrap();
        if output.status.success() {
            true
        } else {
            println!("{:?}", String::from_utf8_lossy(&output.stdout));
            false
        }
    }

    pub(crate) fn make_alphanumeric_string(len: usize) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(len)
            .map(char::from)
            .collect()
    }

    pub(crate) fn make_upper_hex_string(len: usize) -> String {
        thread_rng()
            .sample_iter(&UpperHex)
            .take(len)
            .map(char::from)
            .collect()
    }

    pub(crate) fn make_disk(block_size: usize, total_size: u64) -> PathBuf {
        use std::os::unix::fs::FileExt;

        let mut disk = std::path::PathBuf::new();
        disk.push(r"/tmp");
        disk.push(format!("{}.disk", make_alphanumeric_string(8)));

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(disk.as_path())
            .expect("Failed to create file.");
        let bytes = (0..block_size as usize).map(|_| 0).collect::<Vec<u8>>();
        for b in (0..total_size).step_by(block_size as usize) {
            file.write_at(&bytes, b).expect("Failed to fill data.");
        }
        file.sync_data().expect("Failed to flush data.");

        let (uuid, volumn_name, hash_seed) = (
            make_upper_hex_string(16),
            b"AAAABBBBCCCCDDDD",
            make_upper_hex_string(16),
        );
        match block_size {
            1024 => format::format::<_, 1024>(
                file,
                uuid.as_bytes().try_into().unwrap(),
                &volumn_name,
                hash_seed.as_bytes().try_into().unwrap(),
            )
            .map(|_| ()),
            2048 => format::format::<_, 2048>(
                file,
                uuid.as_bytes().try_into().unwrap(),
                &volumn_name,
                hash_seed.as_bytes().try_into().unwrap(),
            )
            .map(|_| ()),
            4096 => format::format::<_, 4096>(
                file,
                uuid.as_bytes().try_into().unwrap(),
                &volumn_name,
                hash_seed.as_bytes().try_into().unwrap(),
            )
            .map(|_| ()),
            _ => unreachable!(),
        }
        .expect("Failed to format the device.");
        disk
    }

    pub(crate) fn run_test(
        action: impl FnOnce(alloc::sync::Arc<crate::FileSystem<std::fs::File, 4096>>),
        size_in_mi: u64,
    ) {
        const M: u64 = 1024 * 1024;

        let disk = make_disk(4096, size_in_mi * M);
        action(
            crate::open_fs(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(disk.as_path())
                    .expect("failed to open disk"),
            )
            .expect("Failed to open fs"),
        );
        let r = test_oracle(&disk);
        let _ = std::fs::remove_file(&disk);
        assert!(r, "fsck failed");
    }
}
