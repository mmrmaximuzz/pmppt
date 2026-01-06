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

// Keep in sync with ArtifactValue
// TODO: remove code duplication by using some kind of macro
#[derive(Clone, Debug)]
pub enum ArtifactValueType {
    StringList,
}

#[derive(Clone, Debug)]
pub enum ConfigValue {
    String(String),
    Time(Duration),
}
