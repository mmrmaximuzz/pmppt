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

#[derive(Deserialize, Serialize)]
pub enum SpawnMode {
    Foreground,
    BackgroundWait,
    BackgroundKill,
}

impl From<SpawnMode> for communication::SpawnMode {
    fn from(value: SpawnMode) -> Self {
        match value {
            SpawnMode::Foreground => Self::Foreground,
            SpawnMode::BackgroundWait => Self::BackgroundWait,
            SpawnMode::BackgroundKill => Self::BackgroundKill,
        }
    }
}

impl From<communication::SpawnMode> for SpawnMode {
    fn from(value: communication::SpawnMode) -> Self {
        match value {
            communication::SpawnMode::Foreground => Self::Foreground,
            communication::SpawnMode::BackgroundWait => Self::BackgroundWait,
            communication::SpawnMode::BackgroundKill => Self::BackgroundKill,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub enum Request {
    Poll {
        pattern: String,
    },
    Spawn {
        cmd: String,
        args: Vec<String>,
        mode: SpawnMode,
    },
    Stop {
        id: u32,
    },
    StopAll,
    Collect,
    End,
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
            Request::Stop { id } => communication::Request::Stop { id: Id::from(id) },
            Request::StopAll => communication::Request::StopAll,
            Request::Collect => communication::Request::Collect,
            Request::End => communication::Request::End,
            Request::Abort => communication::Request::Abort,
        }
    }
}

impl From<communication::Request> for Request {
    fn from(value: communication::Request) -> Self {
        match value {
            communication::Request::Poll { pattern } => Self::Poll { pattern },
            communication::Request::Spawn { cmd, args, mode } => Self::Spawn {
                cmd,
                args,
                mode: SpawnMode::from(mode),
            },
            communication::Request::Stop { id } => Self::Stop { id: id.into() },
            communication::Request::StopAll => Self::StopAll,
            communication::Request::Collect => Self::Collect,
            communication::Request::End => Self::End,
            communication::Request::Abort => Self::Abort,
        }
    }
}

/// Agent's result for incoming request.
#[derive(Deserialize, Serialize)]
pub enum Response {
    Poll(Result<u32, String>),
    SpawnFg(Result<(Vec<u8>, Vec<u8>), String>),
    SpawnBg(Result<u32, String>),
    Stop(Result<u32, String>),
    StopAll(Result<(), String>),
    Collect(Result<Vec<u8>, String>),
}

impl From<communication::Response> for Response {
    fn from(value: communication::Response) -> Self {
        match value {
            communication::Response::Poll(res) => Self::Poll(res.map(u32::from)),
            communication::Response::SpawnFg(res) => Self::SpawnFg(res),
            communication::Response::SpawnBg(res) => Self::SpawnBg(res.map(u32::from)),
            communication::Response::Stop(res) => Self::Stop(res.map(u32::from)),
            communication::Response::StopAll(res) => Self::StopAll(res),
            communication::Response::Collect(res) => Self::Collect(res),
        }
    }
}

impl From<Response> for communication::Response {
    fn from(value: Response) -> Self {
        match value {
            Response::Poll(res) => Self::Poll(res.map(Id::from)),
            Response::SpawnFg(res) => Self::SpawnFg(res),
            Response::SpawnBg(res) => Self::SpawnBg(res.map(Id::from)),
            Response::Stop(res) => Self::Stop(res.map(Id::from)),
            Response::StopAll(res) => Self::StopAll(res),
            Response::Collect(res) => Self::Collect(res),
        }
    }
}
