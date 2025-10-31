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

//! Implementation of the local protocol (based on explicit static JSON files)
pub mod selfhosted {
    use std::fs;
    use std::io::Read;
    use std::time::Duration;

    use log::{debug, error};
    use serde::Deserialize;
    use serde_json::Value;

    use crate::agent::AgentOps;
    use crate::common::communication::{Request, Response, SpawnMode};

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

                    // when local requests are over, implicitly generate Finish request
                    None => break Request::Finish,
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

                    // emulate the Abort message from the controller
                    self.requests.push(SelfHostedRequest::Abort);
                }
                Response::Poll(Ok(id)) => {
                    debug!("Poll result: id={}", id);
                }

                Response::SpawnFg(Err(msg)) => {
                    error!(
                        r#"FG spawn failed: req={:?}, error="{}""#,
                        self.current, msg
                    );

                    // emulate the Abort message from the controller
                    self.requests.push(SelfHostedRequest::Abort);
                }
                Response::SpawnFg(Ok(_)) => {
                    // no need for FG spawn result in local mode
                }

                Response::SpawnBg(Err(msg)) => {
                    error!(
                        r#"BG spawn failed: req={:?}, error="{}""#,
                        self.current, msg
                    );

                    // emulate the Abort message from the controller
                    self.requests.push(SelfHostedRequest::Abort);
                }
                Response::SpawnBg(Ok(id)) => {
                    debug!("BG spawn result: id={}", id);
                }
            }

            // in local mode this function cannot fail
            Some(())
        }
    }
}
