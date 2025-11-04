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

use serde::{Deserialize, Serialize};

use super::communication::{self, Id};

#[derive(Deserialize)]
pub enum SpawnMode {
    Foreground,
    BackgroundWait,
    BackgroundKill,
}

impl From<SpawnMode> for communication::SpawnMode {
    fn from(value: SpawnMode) -> Self {
        match value {
            SpawnMode::Foreground => communication::SpawnMode::Foreground,
            SpawnMode::BackgroundWait => communication::SpawnMode::BackgroundWait,
            SpawnMode::BackgroundKill => communication::SpawnMode::BackgroundKill,
        }
    }
}

#[derive(Deserialize)]
pub enum Request {
    Poll {
        pattern: String,
    },
    Spawn {
        cmd: String,
        args: Vec<String>,
        mode: SpawnMode,
    },
    Finish {
        id: u32,
    },
    FinishAll,
    Abort,
}

impl From<Request> for communication::Request {
    fn from(value: Request) -> Self {
        match value {
            Request::Poll { pattern } => communication::Request::Poll { pattern },
            Request::Spawn { cmd, args, mode } => communication::Request::Spawn {
                cmd,
                args,
                mode: communication::SpawnMode::from(mode),
            },
            Request::Finish { id } => communication::Request::Finish { id: Id::from(id) },
            Request::FinishAll => communication::Request::FinishAll,
            Request::Abort => communication::Request::Abort,
        }
    }
}

/// Agent's result for incoming request.
#[derive(Serialize)]
pub enum Response {
    Poll(Result<u32, String>),
    Finish(Result<u32, String>),
    SpawnBg(Result<u32, String>),
    SpawnFg(Result<(Vec<u8>, Vec<u8>), String>),
}

impl From<communication::Response> for Response {
    fn from(value: communication::Response) -> Self {
        match value {
            communication::Response::Poll(res) => Self::Poll(res.map(u32::from)),
            communication::Response::SpawnFg(res) => Self::SpawnFg(res),
            communication::Response::SpawnBg(res) => Self::SpawnBg(res.map(u32::from)),
            communication::Response::Finish(res) => Self::Finish(res.map(u32::from)),
        }
    }
}
