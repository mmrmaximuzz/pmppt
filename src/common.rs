// PMPPT - Poor Man's Performance Profiler Tool
// Copyright (C) 2025  Maxim Petrov
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::path::{Path, PathBuf};

pub mod communication;
pub mod msgpack_impl;

/// Use simple text descriptions as error typoe for all the errors in PMPPT.
pub type Result<T> = std::result::Result<T, String>;

/// Little helper function to convert str literals to error message.
pub fn emsg<T, U: AsRef<str>>(s: U) -> Result<T> {
    Err(s.as_ref().to_string())
}

fn find_max_numeric_dir(base: &Path) -> Result<u32> {
    let mut max_dir = 0;

    for dir in base
        .read_dir()
        .map_err(|e| format!("cannot read dir: {e}"))?
        .flatten()
    {
        let name = dir.file_name();
        match name.to_string_lossy().parse::<u32>() {
            Ok(value) => max_dir = std::cmp::max(max_dir, value),
            Err(_) => continue,
        }
    }

    Ok(max_dir)
}

pub fn create_next_numeric_dir_in(base: &Path) -> Result<PathBuf> {
    if base.exists() && !base.is_dir() {
        return Err(format!("path '{base:?}' is not a directory"));
    }

    let next_dir_num = if base.exists() {
        find_max_numeric_dir(base)? + 1
    } else {
        0
    };

    let new_dir = base.join(Path::new(&next_dir_num.to_string()));
    std::fs::create_dir_all(&new_dir).map_err(|e| format!("cannot create dir in {base:?}: {e}"))?;

    Ok(new_dir)
}
