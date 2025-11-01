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

use std::fmt::Display;

/// Request from a Controller to an Agent
#[derive(Debug, Clone)]
pub enum Request {
    Poll {
        pattern: String,
    },
    Spawn {
        cmd: String,
        args: Vec<String>,
        mode: SpawnMode,
    },
    FinishAll,
    Abort,
}

#[derive(Debug, Clone, Copy)]
pub enum SpawnMode {
    Foreground,
    BackgroundWait,
    BackgroundKill,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq)]
pub struct Id(u32);
pub type IdOrError = Result<Id, String>;
pub type OutOrError = Result<(Vec<u8>, Vec<u8>), String>;

impl Id {
    pub fn from_u32(id: u32) -> Self {
        Self(id)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent's result for incoming request.
pub enum Response {
    Poll(IdOrError),
    SpawnFg(OutOrError),
    SpawnBg(IdOrError),
}

pub trait PmpptSerializer {
    fn sreq(&self, req: &Request) -> Vec<u8>;
    fn dreq(&self, data: &[u8]) -> Option<Request>;
    fn sresp(&self, resp: &Response) -> Vec<u8>;
    fn dresp(&self, data: &[u8]) -> Option<Response>;
}
