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

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::types::ArtifactValue;

#[derive(Default, Debug)]
pub struct Storage {
    stor: Arc<Mutex<HashMap<String, ArtifactValue>>>,
}

impl Storage {
    pub fn set(&self, key: &str, val: ArtifactValue) {
        let mut stor = self.stor.lock().unwrap();
        let res = stor.insert(key.to_string(), val);

        // TODO: implement storage verification
        assert!(res.is_none(), "artifact with key {key} already existied");
    }

    pub fn get(&self, key: &str) -> ArtifactValue {
        let stor = self.stor.lock().unwrap();
        // TODO: implement storage verification
        let val = stor
            .get(key)
            .unwrap_or_else(|| panic!("failed to get artifact by key '{key}'"));
        (*val).clone()
    }
}
