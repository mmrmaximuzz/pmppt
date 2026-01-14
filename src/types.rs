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

use std::time::Duration;

#[derive(Clone, Debug)]
pub enum ArtifactValue {
    StringList(Vec<String>),
}

// Keep in sync with ArtifactValue.
// Or remove code duplication by using some kind of macro
#[derive(Clone, Debug)]
pub enum ArtifactValueType {
    StringList,
}

// .INI-file like structure
#[derive(Clone, Debug, Default)]
pub struct IniLike {
    pub global: Vec<String>,
    pub sections: Vec<(String, Vec<String>)>,
}

impl IniLike {
    pub fn with_global(cfg: &[&str]) -> Self {
        IniLike {
            global: cfg.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    pub fn section(mut self, name: &str, cfg: &[&str]) -> Self {
        self.sections.push((
            name.to_string(),
            cfg.iter().map(|s| s.to_string()).collect(),
        ));
        self
    }
}

#[derive(Clone, Debug)]
pub enum ConfigValue {
    String(String),
    T2String((String, String)),
    Time(Duration),
    Ini(IniLike),
}
