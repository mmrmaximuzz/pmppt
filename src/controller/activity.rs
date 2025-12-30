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

use crate::common::{Res, communication::Id};

use super::{configuration::Run, connection::ConnectionOps};

pub fn process_run(_run: &Run) -> Res<()> {
    Ok(())
}

pub trait Activity {
    fn start(&mut self, conn: &mut dyn ConnectionOps) -> Res<Option<Id>>;
    fn stop(&mut self, _conn: &mut dyn ConnectionOps) -> Res<()> {
        // by default stop is a noop
        Ok(())
    }
}

pub mod default_activities {
    use std::path::PathBuf;
    use std::thread::sleep;
    use std::time::Duration;

    use crate::common::Res;
    use crate::common::communication::Request::{self, Poll};
    use crate::common::communication::{Id, Response, SpawnMode};
    use crate::controller::connection::ConnectionOps;

    use super::Activity;

    struct Sleeper {
        period: Duration,
    }

    impl Activity for Sleeper {
        fn start(&mut self, _conn: &mut dyn ConnectionOps) -> Res<Option<Id>> {
            sleep(self.period);
            Ok(None)
        }
    }

    pub fn get_sleeper(period: Duration) -> Box<dyn Activity> {
        Box::new(Sleeper { period })
    }

    struct Poller {
        pattern: String,
        id: Option<Id>,
    }

    impl Activity for Poller {
        fn start(&mut self, conn: &mut dyn ConnectionOps) -> Res<Option<Id>> {
            conn.send(Poll {
                pattern: self.pattern.clone(),
            })
            .map_err(|e| format!("failed to send Poll for '{}': {e}", self.pattern))?;

            match conn.recv().map_err(|e| {
                format!(
                    "failed to get response for Poll for '{}': {e}",
                    self.pattern
                )
            })? {
                Response::Poll(Ok(id)) => {
                    self.id = Some(id);
                    Ok(Some(id))
                }
                Response::Poll(Err(e)) => {
                    Err(format!("failed to spawn Poll for '{}': {e}", self.pattern))
                }
                other => unreachable!(
                    "protocol exception: got bad agent response for Poll '{}': {other:?}",
                    self.pattern
                ),
            }
        }

        fn stop(&mut self, conn: &mut dyn ConnectionOps) -> Res<()> {
            let id = match self.id {
                Some(id) => id,
                None => {
                    return Err(format!(
                        "trying to stop non-started poller '{}'",
                        self.pattern
                    ));
                }
            };

            conn.send(Request::Stop { id }).map_err(|e| {
                format!(
                    "failed to send Stop request for Poller '{}'(id={id}): {e}",
                    self.pattern
                )
            })?;

            match conn.recv().map_err(|e| {
                format!(
                    "failed to get response for Stop request for '{}'(id={id}): {e}",
                    self.pattern
                )
            })? {
                Response::Stop(resp_id) => resp_id
                    .map_err(|e| format!("failed to stop Poll '{}'(id={id}): {e}", self.pattern))?,
                other => unreachable!(
                    "protocol exception: got bad agent response for Poll Stop '{}': {other:?}",
                    self.pattern
                ),
            };

            Ok(())
        }
    }

    impl Poller {
        fn create(pattern: &str) -> Box<dyn Activity> {
            Box::new(Self {
                pattern: pattern.to_string(),
                id: None,
            })
        }
    }

    pub fn proc_meminfo() -> Box<dyn Activity> {
        Poller::create("/proc/meminfo")
    }

    pub fn proc_net_dev() -> Box<dyn Activity> {
        Poller::create("/proc/net/dev")
    }

    struct Launcher {
        comm: String,
        args: Vec<String>,
        mode: SpawnMode,
        id: Option<Id>,
    }

    impl Activity for Launcher {
        fn start(&mut self, conn: &mut dyn ConnectionOps) -> Res<Option<Id>> {
            conn.send(Request::Spawn {
                cmd: self.comm.clone(),
                args: self.args.clone(),
                mode: self.mode,
            })
            .map_err(|e| {
                format!(
                    "failed to send Launcher request for comm '{}', {e}",
                    self.comm
                )
            })?;

            self.id = match conn.recv().map_err(|e| {
                format!(
                    "failed to get response for Launcher for comm '{}': {e}",
                    self.comm
                )
            })? {
                Response::SpawnFg(Ok(_)) => None, // TODO: use fg result
                Response::SpawnFg(Err(e)) => {
                    return Err(format!(
                        "error in Launcher foreground spawn comm '{}': {e}",
                        self.comm
                    ));
                }
                Response::SpawnBg(Ok(id)) => Some(id),
                Response::SpawnBg(Err(e)) => {
                    return Err(format!(
                        "error in Launcher background spawn comm '{}': {e}",
                        self.comm
                    ));
                }
                other => unreachable!(
                    "protocol exception: got bad agent response for Launcher start with comm '{}': {other:?}",
                    self.comm
                ),
            };

            Ok(self.id)
        }

        fn stop(&mut self, conn: &mut dyn ConnectionOps) -> Res<()> {
            match self.id {
                None => Ok(()),
                Some(id) => {
                    conn.send(Request::Stop { id }).map_err(|e| {
                        format!(
                            "failed to send request for stop Launcher comm '{}': {e}",
                            self.comm
                        )
                    })?;

                    match conn.recv().map_err(|e| {
                        format!(
                            "failed to get response for stop Launcher comm '{}': {e}",
                            self.comm
                        )
                    })? {
                        Response::Stop(Ok(resp_id)) => {
                            assert_eq!(resp_id, id);
                            Ok(())
                        }
                        Response::Stop(Err(e)) => {
                            Err(format!("failed to stop Launcher comm '{}': {e}", self.comm))
                        }
                        other => unreachable!(
                            "protocol exception: got bad agent response for Launcher stop with comm '{}': {other:?}",
                            self.comm
                        ),
                    }
                }
            }
        }
    }

    pub fn launch_mpstat() -> Box<dyn Activity> {
        Box::new(Launcher {
            comm: String::from("mpstat"),
            mode: SpawnMode::BackgroundKill,
            args: ["-P", "ALL", "1"].into_iter().map(String::from).collect(),
            id: None,
        })
    }

    pub fn launch_iostat_on(devs: &[PathBuf]) -> Box<dyn Activity> {
        Box::new(Launcher {
            comm: String::from("iostat"),
            mode: SpawnMode::BackgroundKill,
            args: ["-d", "-t", "-x", "-m", "1"]
                .into_iter()
                .map(String::from)
                .chain(devs.iter().map(|p| p.to_string_lossy().to_string()))
                .collect(),
            id: None,
        })
    }

    pub fn launch_iostat() -> Box<dyn Activity> {
        launch_iostat_on(&[])
    }

    pub fn launch_fio(cfg: Vec<String>) -> Box<dyn Activity> {
        Box::new(Launcher {
            comm: String::from("fio"),
            mode: SpawnMode::BackgroundWait,
            args: cfg,
            id: None,
        })
    }

    pub fn launch_flamegraph() -> Box<dyn Activity> {
        Box::new(Launcher {
            comm: String::from("flamegraph"),
            mode: SpawnMode::BackgroundWait, // TODO: need to add SIGINT handler
            args: ["-F", "99", "--", "--all-cpus"]
                .into_iter()
                .map(String::from)
                .collect(),
            id: None,
        })
    }
}
