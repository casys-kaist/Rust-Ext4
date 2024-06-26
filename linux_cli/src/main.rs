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

mod cli;
mod shell;

use std::fs::OpenOptions;
use std::path::Path;

fn do_shell(path: &str) {
    if let Ok(fs) = ext4::open_fs::<_, 1024>(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(path))
            .expect("Failed to open device."),
    ) {
        shell::Shell::new(fs).run()
    } else if let Ok(fs) = ext4::open_fs::<_, 2048>(
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(path))
            .expect("Failed to open device."),
    ) {
        shell::Shell::new(fs).run()
    } else {
        shell::Shell::new(
            ext4::open_fs::<_, 4096>(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(Path::new(path))
                    .expect("Failed to open device."),
            )
            .unwrap(),
        )
        .run()
    }
}

fn main() {
    use clap::Parser;
    use cli::{Command, FormatArg, MainArg, PathArg};

    match MainArg::parse().command {
        Command::Shell(PathArg { path }) => do_shell(&path),
        Command::Format(FormatArg {
            path,
            block,
            create,
        }) => linux_driver::format(&path, block, create).expect("Failed to format the device."),
        Command::Fuzz => todo!(),
    };
}
