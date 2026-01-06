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

use std::{collections::HashMap, time::Duration};

use crate::{
    common::{Res, communication::Id},
    controller::storage::Storage,
    types::ConfigValue,
};

use super::{configuration::Run, connection::ConnectionOps};

pub fn process_run(_run: &Run) -> Res<()> {
    Ok(())
}

pub trait Activity {
    fn start(&mut self, conn: &mut dyn ConnectionOps, stor: &mut Storage) -> Res<Option<Id>>;
    fn stop(&mut self, _conn: &mut dyn ConnectionOps) -> Res<()> {
        // by default stop is a noop
        Ok(())
    }
}

type ArtifactNameBinding = HashMap<String, String>;

#[derive(Debug, Default)]
pub struct ActivityConfig {
    pub value: Option<ConfigValue>,
    pub input: Option<ArtifactNameBinding>,
    pub output: Option<ArtifactNameBinding>,
}

impl ActivityConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_none() && self.input.is_none() && self.output.is_none()
    }

    pub fn has_value(&self) -> bool {
        self.value.is_some()
    }

    pub fn has_artifacts_in(&self) -> bool {
        self.input.is_some()
    }

    pub fn has_artifacts_out(&self) -> bool {
        self.output.is_some()
    }

    pub fn has_artifacts(&self) -> bool {
        self.has_artifacts_in() || self.has_artifacts_out()
    }

    pub fn with_time(time: Duration) -> Self {
        Self {
            value: Some(ConfigValue::Time(time)),
            ..Default::default()
        }
    }

    pub fn with_str<T: AsRef<str>>(s: T) -> Self {
        Self {
            value: Some(ConfigValue::String(s.as_ref().to_string())),
            ..Default::default()
        }
    }

    pub fn artifact_in<T: AsRef<str>>(mut self, art_name: T, bind_name: T) -> Self {
        if let Some(ref mut input) = self.input {
            input.insert(
                art_name.as_ref().to_string(),
                bind_name.as_ref().to_string(),
            );
        } else {
            self.input = Some(HashMap::from([(
                art_name.as_ref().to_string(),
                bind_name.as_ref().to_string(),
            )]));
        }
        self
    }

    pub fn artifact_out<T: AsRef<str>>(mut self, art_name: T, bind_name: T) -> Self {
        if let Some(ref mut output) = self.output {
            output.insert(
                art_name.as_ref().to_string(),
                bind_name.as_ref().to_string(),
            );
        } else {
            self.output = Some(HashMap::from([(
                art_name.as_ref().to_string(),
                bind_name.as_ref().to_string(),
            )]));
        }
        self
    }

    fn verify_single_artifact(
        bind: &Option<ArtifactNameBinding>,
        expected_name: &str,
    ) -> Res<String> {
        let (art_name, bind_name) = match bind {
            Some(output) => {
                let items: Vec<_> = output.iter().collect();
                match &items[..] {
                    [a] => a.clone(),
                    [] => {
                        return Err(format!(
                            "expected single artifact '{expected_name}', but got none"
                        ));
                    }
                    _ => {
                        return Err(format!(
                            "expected single artifact '{expected_name}', but got many: {items:?}"
                        ));
                    }
                }
            }
            None => {
                return Err(format!(
                    "expected single artifact '{expected_name}', but got no artifacts"
                ));
            }
        };

        if art_name != expected_name {
            return Err(format!(
                "expected artifact '{expected_name}', but got '{art_name}'"
            ));
        }

        Ok(bind_name.to_string())
    }

    pub fn verify_single_artifact_in<T: AsRef<str>>(&self, expected_name: T) -> Res<String> {
        Self::verify_single_artifact(&self.input, expected_name.as_ref())
    }

    pub fn verify_single_artifact_out<T: AsRef<str>>(&self, expected_name: T) -> Res<String> {
        Self::verify_single_artifact(&self.output, expected_name.as_ref())
    }
}

pub type ActivityCreator = dyn Fn(ActivityConfig) -> Res<Box<dyn Activity>>;

pub mod default_activities {
    use std::thread::sleep;
    use std::time::Duration;

    use crate::common::Res;
    use crate::common::communication::Request::{self, Poll};
    use crate::common::communication::{Id, Response, SpawnMode};
    use crate::controller::activity::ActivityConfig;
    use crate::controller::connection::ConnectionOps;
    use crate::controller::storage::Storage;
    use crate::types::{ArtifactValue, ConfigValue};

    use super::Activity;

    struct Sleeper {
        period: Duration,
    }

    impl Activity for Sleeper {
        fn start(&mut self, _conn: &mut dyn ConnectionOps, _stor: &mut Storage) -> Res<Option<Id>> {
            sleep(self.period);
            Ok(None)
        }
    }

    pub fn sleeper_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if conf.has_artifacts() {
            return Err(format!(
                "sleeper does not accept artifacts but got: {conf:?}"
            ));
        }

        match conf.value {
            Some(ConfigValue::Time(period)) => Ok(Box::new(Sleeper { period })),
            other => Err(format!(
                "sleeper expects just single UInt argument, got {other:?}"
            )),
        }
    }

    struct Poller {
        pattern: String,
        id: Option<Id>,
    }

    impl Activity for Poller {
        fn start(&mut self, conn: &mut dyn ConnectionOps, _stor: &mut Storage) -> Res<Option<Id>> {
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
        fn create(pattern: &str) -> Self {
            Self {
                pattern: pattern.to_string(),
                id: None,
            }
        }
    }

    pub fn proc_meminfo_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if !conf.is_empty() {
            return Err(format!(
                "meminfo poller expects no config, but got: {conf:?}"
            ));
        }
        Ok(Box::new(Poller::create("/proc/meminfo")))
    }

    pub fn proc_net_dev_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if !conf.is_empty() {
            return Err(format!(
                "net_dev poller expects no config, but got: {conf:?}"
            ));
        }
        Ok(Box::new(Poller::create("/proc/net/dev")))
    }

    struct Launcher {
        comm: String,
        args: Vec<String>,
        mode: SpawnMode,
        id: Option<Id>,
    }

    impl Activity for Launcher {
        fn start(&mut self, conn: &mut dyn ConnectionOps, _stor: &mut Storage) -> Res<Option<Id>> {
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

    pub fn mpstat_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if !conf.is_empty() {
            return Err(format!("mpstat accepts empty config, but got {conf:?}"));
        }

        Ok(Box::new(Launcher {
            comm: String::from("mpstat"),
            mode: SpawnMode::BackgroundKill,
            args: ["-P", "ALL", "1"].into_iter().map(String::from).collect(),
            id: None,
        }))
    }

    struct IostatLauncher {
        launcher: Launcher,
        input: Option<String>,
    }

    impl Activity for IostatLauncher {
        fn start(&mut self, conn: &mut dyn ConnectionOps, stor: &mut Storage) -> Res<Option<Id>> {
            if let Some(ref name) = self.input {
                let items = match stor.get(name) {
                    ArtifactValue::StringList(items) => items,
                };
                // extend default iostat launcher with custom device paths
                self.launcher.args.extend_from_slice(&items);
            }
            self.launcher.start(conn, stor)
        }

        fn stop(&mut self, conn: &mut dyn ConnectionOps) -> Res<()> {
            self.launcher.stop(conn)
        }
    }

    pub fn iostat_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if conf.has_value() {
            return Err(format!("iostat expect no value, but got {:?}", conf.value));
        }

        if conf.has_artifacts_out() {
            return Err(format!(
                "iostat expect no output artifacts but got {:?}",
                conf.output
            ));
        }

        let devs_art_name = if let Some(_) = conf.input {
            Some(
                conf.verify_single_artifact_in("devices")
                    .map_err(|e| format!("iostat expect optional input artifact: {e}"))?,
            )
        } else {
            None
        };

        Ok(Box::new(IostatLauncher {
            launcher: Launcher {
                comm: String::from("iostat"),
                mode: SpawnMode::BackgroundKill,
                args: ["-d", "-t", "-x", "-m", "1"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                id: None,
            },
            input: devs_art_name,
        }))
    }

    pub fn launch_fio(cfg: Vec<String>) -> Box<dyn Activity> {
        Box::new(Launcher {
            comm: String::from("fio"),
            mode: SpawnMode::BackgroundWait,
            args: cfg,
            id: None,
        })
    }

    pub fn flamegraph_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if !conf.is_empty() {
            return Err(format!("flamegraph expects no config, but got: {conf:?}"));
        }

        Ok(Box::new(Launcher {
            comm: String::from("flamegraph"),
            mode: SpawnMode::BackgroundWait, // TODO: add SIGINT handler
            args: ["-F", "99", "--", "--all-cpus", "sleep", "3"]
                .into_iter()
                .map(String::from)
                .collect(),
            id: None,
        }))
    }

    struct LookupPaths {
        pattern: String,
        out_paths: String,
    }

    impl Activity for LookupPaths {
        fn start(&mut self, conn: &mut dyn ConnectionOps, stor: &mut Storage) -> Res<Option<Id>> {
            conn.send(Request::LookupPaths {
                pattern: self.pattern.clone(),
            })
            .map_err(|e| format!("failed to send LookupPath request: {e}"))?;

            let paths = match conn
                .recv()
                .map_err(|e| format!("failed to recv LookupPath response: {e}"))?
            {
                Response::LookupPaths(path_bufs) => path_bufs?,
                otherwise => unreachable!("protocol exception: bad response: {otherwise:?}"),
            };

            let paths = paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();

            stor.set(&self.out_paths, ArtifactValue::StringList(paths));
            Ok(None)
        }
    }

    pub fn lookup_creator(conf: ActivityConfig) -> Res<Box<dyn Activity>> {
        if conf.has_artifacts_in() {
            return Err(format!(
                "lookup expects no input artifacts but got: {:?}",
                conf.input
            ));
        }

        let pattern = match conf.value {
            Some(ConfigValue::String(ref pattern)) => pattern.clone(),
            other => {
                return Err(format!(
                    "lookup expects config value of type String, got: {other:?}"
                ));
            }
        };

        let out_paths = conf
            .verify_single_artifact_out("paths")
            .map_err(|e| format!("lookup out 'paths': {e}"))?;

        Ok(Box::new(LookupPaths { pattern, out_paths }))
    }
}
