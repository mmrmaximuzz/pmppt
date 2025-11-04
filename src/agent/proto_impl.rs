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

//! Implementations of PMPPT protocol for the agent side.

/// Implementation of the local protocol (based on explicit static JSON files)
pub mod selfhosted {
    use std::fs;
    use std::io::Read;
    use std::time::Duration;

    use log::{debug, error};
    use serde::Deserialize;
    use serde_json::Value;

    use crate::agent::AgentOps;
    use crate::common::communication::{Id, Request, Response, SpawnMode};

    #[derive(Deserialize)]
    enum ExecMode {
        #[serde(rename = "fg")]
        Foreground,
        #[serde(rename = "bgwait")]
        BackgroundWait,
        #[serde(rename = "bgkill")]
        BackgroundKill,
    }

    fn selfhosted_mode_to_agent(mode: Option<ExecMode>) -> SpawnMode {
        match mode {
            // default spawn is foreground
            None => SpawnMode::Foreground,
            // others are just mapped
            Some(ExecMode::Foreground) => SpawnMode::Foreground,
            Some(ExecMode::BackgroundWait) => SpawnMode::BackgroundWait,
            Some(ExecMode::BackgroundKill) => SpawnMode::BackgroundKill,
        }
    }

    #[derive(Deserialize)]
    #[serde(tag = "type", content = "data")]
    enum SelfHostedRequest {
        // mapped PMPPT commands
        Poll {
            pattern: String,
        },
        Spawn {
            cmd: String,
            args: Option<Vec<String>>,
            mode: Option<ExecMode>,
        },
        Finish {
            id: u32,
        },
        Abort,
        // local transport commands (non-PMPPT)
        Pause {
            prompt: Option<String>,
        },
        Sleep {
            time: f64,
        },
    }

    pub struct SelfHostedProtocol {
        requests: Vec<SelfHostedRequest>,
        current: Option<Request>,
    }

    impl SelfHostedProtocol {
        pub fn from_json(json_path: &str) -> Result<Self, String> {
            // first read the JSON file completely
            let content = fs::read_to_string(json_path)
                .map_err(|e| format!("cannot read '{json_path}': {e}"))?;

            // parse as raw JSON list first
            let values: Vec<Value> =
                serde_json::from_str(&content).map_err(|e| format!("bad JSON format: {e}"))?;

            // then map every command to PMPPT protocol
            let mut requests: Vec<SelfHostedRequest> = serde_json::from_value(Value::Array(values))
                .map_err(|e| format!("unsupported command found: {e}"))?;

            // reverse the vector to extract the elements with `pop`
            requests.reverse();

            Ok(SelfHostedProtocol {
                requests,
                current: None,
            })
        }

        /// emulate the Abort message from the controller
        fn initiate_abort(&mut self) {
            self.requests.push(SelfHostedRequest::Abort);
        }
    }

    const CLI_PROMPT: &str = r#"
    ==================================================
    =======   Further execution is paused.     =======
    ======= Press Enter to continue execution. =======
    ==================================================
    "#;

    impl AgentOps for SelfHostedProtocol {
        fn recv_request(&mut self) -> Option<Request> {
            // Extract the new local agent request from the config.
            //
            // In local mode we don't have any real PMPPT controller connected. So we try to
            // imitate its existence by remembering the current executing request to associate agent
            // responses with it.
            self.current = loop {
                match self.requests.pop() {
                    Some(local_req) => match local_req {
                        // provide mapped command as-is
                        SelfHostedRequest::Poll { pattern } => break Request::Poll { pattern },
                        SelfHostedRequest::Spawn { cmd, args, mode } => {
                            break Request::Spawn {
                                cmd,
                                args: args.unwrap_or_default(), // default is no args
                                mode: selfhosted_mode_to_agent(mode), // default is foreground
                            };
                        }
                        SelfHostedRequest::Finish { id } => {
                            break Request::Finish { id: Id::from(id) };
                        }
                        SelfHostedRequest::Abort => break Request::Abort,

                        // handle local commands specially
                        SelfHostedRequest::Sleep { time } => {
                            std::thread::sleep(Duration::from_secs_f64(time));
                            continue;
                        }
                        SelfHostedRequest::Pause { prompt } => {
                            println!("{}", CLI_PROMPT.trim());
                            if let Some(prompt) = prompt {
                                println!("Description: {prompt}");
                            }
                            std::io::stdin()
                                .read_exact(&mut [0u8])
                                .expect("stdin is broken");
                        }
                    },

                    // when local requests are over, implicitly generate FinishAll request
                    None => break Request::FinishAll,
                }
            }
            .into();

            // return the request to the agent to execute
            self.current.clone()
        }

        // imitate that we "receive" a response from the controller
        fn send_response(&mut self, response: Response) -> Option<()> {
            match response {
                // TODO: stop the execution instead of just panic
                Response::Poll(Err(msg)) => {
                    error!(
                        r#"Poll request failed: req={:?}, error="{}""#,
                        self.current, msg
                    );
                    self.initiate_abort();
                }
                Response::Poll(Ok(id)) => {
                    debug!("Poll result: id={id}");
                }

                Response::SpawnFg(Err(msg)) => {
                    error!(
                        r#"FG spawn failed: req={:?}, error="{}""#,
                        self.current, msg
                    );
                    self.initiate_abort();
                }
                Response::SpawnFg(Ok(_)) => {
                    // no need for FG spawn result in local mode
                }

                Response::SpawnBg(Err(msg)) => {
                    error!(
                        r#"BG spawn failed: req={:?}, error="{}""#,
                        self.current, msg
                    );
                    self.initiate_abort();
                }
                Response::SpawnBg(Ok(id)) => {
                    debug!("BG spawn result: id={id}");
                }

                Response::Finish(Ok(id)) => {
                    debug!("Finished activity with id={id}");
                }
                Response::Finish(Err(msg)) => {
                    error!(r#"Activity finish failed: error="{msg}""#);
                    self.initiate_abort();
                }
            }

            // in local mode this function cannot fail
            Some(())
        }
    }
}

/// Implementation of the remote protocol based on MsgPack
pub mod tcpmsgpack {
    use std::{io::{Read, Write}, net::TcpStream};

    use rmp_serde::Serializer;
    use serde::Serialize;

    use crate::{
        agent::AgentOps,
        common::{communication, msgpack_impl},
    };

    pub struct TcpMsgpackProtocol {
        conn: TcpStream,
    }

    impl TcpMsgpackProtocol {
        pub fn from_conn(conn: TcpStream) -> TcpMsgpackProtocol {
            TcpMsgpackProtocol { conn }
        }
    }

    impl AgentOps for TcpMsgpackProtocol {
        fn recv_request(&mut self) -> Option<communication::Request> {
            let mut msg_size = [0u8; 4];
            self.conn.read_exact(&mut msg_size).ok()?;
            let msg_size = u32::from_le_bytes(msg_size);

            let mut msg = vec![0u8; msg_size as usize];
            self.conn.read_exact(&mut msg).ok()?;

            let msg = rmp_serde::from_slice::<msgpack_impl::Request>(&msg).ok()?;
            Some(communication::Request::from(msg))
        }

        fn send_response(&mut self, response: communication::Response) -> Option<()> {
            let mut msg_buf = vec![];
            let msg = msgpack_impl::Response::from(response);
            msg.serialize(&mut Serializer::new(&mut msg_buf)).unwrap();

            let msg_size = msg_buf.len().to_le_bytes();

            self.conn.write_all(&msg_size).ok()?;
            self.conn.write_all(&msg_buf).ok()
        }
    }
}
