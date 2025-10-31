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

use std::{collections::HashMap, net::TcpStream};

use crate::common::Res;

use super::configuration::{AgentConfig, AgentId};

pub struct Connection {
    _sock: TcpStream,
}

pub type Connections = HashMap<AgentId, Connection>;

pub fn connect_agents(cfg: &HashMap<AgentId, AgentConfig>) -> Res<Connections> {
    let mut conns = HashMap::default();
    for (name, params) in cfg {
        let ip = params.ip;
        let port = params.port;
        let sock = TcpStream::connect((ip, port))
            .map_err(|e| format!("failed to connect agent '{name}' ({ip}, {port}): {e}"))?;
        conns.insert(name.clone(), Connection { _sock: sock });
    }
    Ok(conns)
}
