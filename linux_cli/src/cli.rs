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

use clap::{Args, Parser, Subcommand};

/// Ext4 linux binder.
#[derive(Parser, Debug)]
#[clap(about, version, author)]
pub struct MainArg {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// start Shell
    Shell(PathArg),
    /// Format the filesystem.
    Format(FormatArg),
    /// Fuzz the filesystem implementaion.
    Fuzz,
}

#[derive(Debug, Args)]
pub struct PathArg {
    /// Image path.
    #[clap(short, long)]
    pub path: String,
}

#[derive(Debug, Args)]
pub struct FormatArg {
    /// Target path to format.
    #[clap(short, long)]
    pub path: String,
    /// Size of a block.
    #[clap(short, long, parse(try_from_str = parse_blk_size))]
    pub block: usize,
    /// Create device if not exist with given size
    #[clap(short, long, parse(try_from_str = parse_file_size))]
    pub create: Option<u64>,
}

#[derive(Debug)]
enum ParseError {
    RequireInteger,
    UnsupportedBlockSize,
    UnknownPostFix,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Self::RequireInteger => write!(f, "Integer is required"),
            Self::UnsupportedBlockSize => write!(
                f,
                "Unsupported Block size (Supported size: 1024, 2048, 4096)."
            ),
            Self::UnknownPostFix => write!(f, "Unknown postfix"),
        }
    }
}

impl std::error::Error for ParseError {}

fn parse_blk_size(b: &str) -> Result<usize, ParseError> {
    use std::str::FromStr;
    match u32::from_str(b) {
        Ok(1024) => Ok(1024),
        Ok(2048) => Ok(2048),
        Ok(4096) => Ok(4096),
        Ok(_) => Err(ParseError::UnsupportedBlockSize),
        Err(_) => Err(ParseError::RequireInteger),
    }
}

fn parse_file_size(b: &str) -> Result<u64, ParseError> {
    use std::str::FromStr;

    let mut b = String::from(b);
    u64::from_str(&b).or_else(|_| match b.pop() {
        Some('K') | Some('k') => u64::from_str(&b)
            .map(|s| s * 1024)
            .map_err(|_| ParseError::RequireInteger),
        Some('M') | Some('m') => u64::from_str(&b)
            .map(|s| s * 1024 * 1024)
            .map_err(|_| ParseError::RequireInteger),
        Some('G') | Some('g') => u64::from_str(&b)
            .map(|s| s * 1024 * 1024 * 1024)
            .map_err(|_| ParseError::RequireInteger),
        Some(_) => Err(ParseError::UnknownPostFix),
        _ => Err(ParseError::RequireInteger),
    })
}
