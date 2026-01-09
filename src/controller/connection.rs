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

use crate::common::{
    Result,
    communication::{Request, Response},
};

use super::cfgparse::{AgentConfig, AgentId};

pub struct Connection {
    _sock: TcpStream,
}

pub type Connections = HashMap<AgentId, Connection>;

pub fn connect_agents(cfg: &HashMap<AgentId, AgentConfig>) -> Result<Connections> {
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

pub trait ConnectionOps {
    fn send(&mut self, req: Request) -> Res<()>;
    fn recv(&mut self) -> Res<Response>;
    fn close(self);
}

pub mod tcpmsgpack {
    use std::{
        io::{Read, Write},
        net::{Shutdown, TcpStream},
    };

    use rmp_serde::Serializer;
    use serde::Serialize;

    use crate::common::{
        Result,
        communication::{self, Request, Response},
        emsg, msgpack_impl,
    };

    use super::ConnectionOps;

    pub struct TcpMsgpackConnection {
        conn: TcpStream,
    }

    impl TcpMsgpackConnection {
        pub fn from_endpoint(endpoint: &str) -> Result<Self> {
            Ok(Self {
                conn: TcpStream::connect(endpoint)
                    .map_err(|e| format!("failed to connect to agent {endpoint}: {e}"))?,
            })
        }
    }

    impl ConnectionOps for TcpMsgpackConnection {
        fn send(&mut self, req: Request) -> Result<()> {
            let mut msg_buf = vec![];
            let msg = msgpack_impl::Request::from(req);
            msg.serialize(&mut Serializer::new(&mut msg_buf)).unwrap(); // cannot fail

            let msg_size = (msg_buf.len() as u32).to_le_bytes();
            self.conn
                .write_all(&msg_size)
                .map_err(|e| format!("failed to send msgsize: {e}"))?;
            self.conn
                .write_all(&msg_buf)
                .map_err(|e| format!("failed to send msgbuf: {e}"))?;

            Ok(())
        }

        fn recv(&mut self) -> Result<Response> {
            let msg_size = u32::from_le_bytes({
                let mut msg_size = [0u8; 4];
                self.conn
                    .read_exact(&mut msg_size)
                    .or(emsg("truncated msgsize"))?;
                msg_size
            });

            let msg_buf = {
                let mut msg = vec![0u8; msg_size as usize];
                self.conn
                    .read_exact(&mut msg)
                    .or(emsg("truncated message"))?;
                msg
            };

            rmp_serde::from_slice::<msgpack_impl::Response>(&msg_buf)
                .map(communication::Response::from)
                .map_err(|e| format!("failed to parse msgpack::Request message: {e}"))
        }

        fn close(self) {
            self.conn
                .shutdown(Shutdown::Both)
                .expect("failed to close the connection");
        }
    }
}
