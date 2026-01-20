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

use std::fmt::Debug;
use std::{collections::HashMap, time::Duration};

use crate::common::communication::SpawnMode;
use crate::{
    common::{Result, communication::Id},
    controller::storage::Storage,
    types::{ConfigValue, IniLike},
};

use super::connection::Connection;

// TODO: change String to more intelligent type
pub type PlotHint = (Id, Option<String>);

pub trait Activity {
    fn start(&mut self, conn: &mut dyn Connection, stor: &Storage) -> Result<()>;
    fn stop(&mut self, _conn: &mut dyn Connection, _stor: &Storage) -> Result<Option<PlotHint>> {
        // by default stop is a noop
        Ok(None)
    }
}

impl Debug for dyn Activity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("Activity {:?}", &self as *const _))
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

    pub fn with_poll_args<T: AsRef<str>>(p: T, h: Option<T>) -> Self {
        Self {
            value: Some(ConfigValue::PollArgs {
                pattern: p.as_ref().to_string(),
                hint: h.map(|h| h.as_ref().to_string()),
            }),
            ..Default::default()
        }
    }

    pub fn with_launch_args<T: AsRef<str>>(
        comm: T,
        mode: SpawnMode,
        args: Vec<String>,
        hint: T,
    ) -> Self {
        Self {
            value: Some(ConfigValue::LaunchArgs {
                comm: comm.as_ref().to_string(),
                mode,
                args,
                hint: hint.as_ref().to_string(),
            }),
            ..Default::default()
        }
    }

    pub fn with_ini(ini: IniLike) -> Self {
        Self {
            value: Some(ConfigValue::Ini(ini)),
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
    ) -> Result<String> {
        let (art_name, bind_name) = match bind {
            Some(output) => {
                let items: Vec<_> = output.iter().collect();
                match &items[..] {
                    [a] => *a,
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

    pub fn verify_single_artifact_in(&self, name: &str) -> Result<String> {
        Self::verify_single_artifact(&self.input, name)
    }

    pub fn verify_single_artifact_out(&self, name: &str) -> Result<String> {
        Self::verify_single_artifact(&self.output, name)
    }

    pub fn verify_optional_single_artifact_in(&self, name: &str) -> Result<Option<String>> {
        if self.input.is_none() {
            Ok(None)
        } else {
            Ok(Some(self.verify_single_artifact_in(name)?))
        }
    }
}

pub type ActivityCreatorFn = fn(ActivityConfig) -> Result<Box<dyn Activity>>;
pub type ActivityCreator = Box<dyn Fn(ActivityConfig) -> Result<Box<dyn Activity + Send>>>;

pub type ActivityDatabase = HashMap<&'static str, ActivityCreator>;

pub mod default_activities {
    use std::collections::HashMap;
    use std::thread::sleep;
    use std::time::Duration;

    use crate::common::Result;
    use crate::common::communication::Request::{self, Poll};
    use crate::common::communication::{Id, Response, SpawnMode};
    use crate::controller::activity::{ActivityConfig, PlotHint};
    use crate::controller::connection::Connection;
    use crate::controller::storage::Storage;
    use crate::types::{ArtifactValue, ConfigValue};

    use super::{Activity, ActivityDatabase};

    trait ExportedActivity {
        fn name(&self) -> &'static str;
        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>>;
    }

    struct Sleeper {
        period: Duration,
    }

    impl Activity for Sleeper {
        fn start(&mut self, _conn: &mut dyn Connection, _stor: &Storage) -> Result<()> {
            sleep(self.period);
            Ok(())
        }
    }

    struct SleeperExport;

    impl ExportedActivity for SleeperExport {
        fn name(&self) -> &'static str {
            "sleep"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if conf.has_artifacts() {
                return Err(format!(
                    "{} does not accept artifacts but got: input={:?}, output={:?}",
                    self.name(),
                    conf.input,
                    conf.output
                ));
            }

            match conf.value {
                Some(ConfigValue::Time(period)) => Ok(Box::new(Sleeper { period })),
                other => Err(format!(
                    "'{}' expects just single Time argument, got {other:?}",
                    self.name()
                )),
            }
        }
    }

    struct Poller {
        pattern: String,
        id: Option<Id>,
        hint: Option<String>,
    }

    impl Activity for Poller {
        fn start(&mut self, conn: &mut dyn Connection, _stor: &Storage) -> Result<()> {
            conn.send(Poll {
                pattern: self.pattern.clone(),
            })
            .map_err(|e| format!("failed to send Poll for '{}': {e}", self.pattern))?;

            self.id = match conn.recv().map_err(|e| {
                format!(
                    "failed to get response for Poll for '{}': {e}",
                    self.pattern
                )
            })? {
                Response::Poll(Ok(id)) => Some(id),
                Response::Poll(Err(e)) => {
                    return Err(format!("failed to spawn Poll for '{}': {e}", self.pattern));
                }
                other => unreachable!(
                    "protocol exception: got bad agent response for Poll '{}': {other:?}",
                    self.pattern
                ),
            };
            Ok(())
        }

        fn stop(&mut self, conn: &mut dyn Connection, _stor: &Storage) -> Result<Option<PlotHint>> {
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

            let recv_id = match conn.recv().map_err(|e| {
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
            assert_eq!(id, recv_id);

            // no special plot info for poller activities - the format is well-known
            Ok(Some((id, self.hint.clone())))
        }
    }

    impl Poller {
        fn create(pattern: &str, hint: Option<String>) -> Self {
            Self {
                pattern: pattern.to_string(),
                id: None,
                hint,
            }
        }
    }

    struct PredefinedPoller {
        name: &'static str,
        pattern: String,
    }

    impl PredefinedPoller {
        fn new<S: AsRef<str>>(name: &'static str, pattern: S) -> Self {
            Self {
                name,
                pattern: pattern.as_ref().to_string(),
            }
        }
    }

    impl ExportedActivity for PredefinedPoller {
        fn name(&self) -> &'static str {
            self.name
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if !conf.is_empty() {
                return Err(format!(
                    "{} poller is pre-defined and expects no config, but got: {conf:?}",
                    self.name
                ));
            }
            // set hint to None, the poller is pre-defined and has its own name
            Ok(Box::new(Poller::create(&self.pattern, None)))
        }
    }

    struct GenericPoller;

    impl ExportedActivity for GenericPoller {
        fn name(&self) -> &'static str {
            "poller"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if conf.has_artifacts() {
                return Err(format!(
                    "'{}' poller expects no artifacts, but got some",
                    self.name()
                ));
            }

            match conf.value {
                Some(ConfigValue::PollArgs { pattern, hint }) => {
                    Ok(Box::new(Poller::create(&pattern, hint)))
                }
                None => Err(format!(
                    "'{}' poller expects configuration but got none",
                    self.name()
                )),
                other => Err(format!(
                    "'{}' poller expects 2-string tuple but got none {other:?}",
                    self.name()
                )),
            }
        }
    }

    struct Launcher {
        comm: String,
        args: Vec<String>,
        mode: SpawnMode,
        id: Option<Id>,
        hint: Option<Box<dyn Fn() -> String + Send>>,
    }

    impl Launcher {
        fn get_hint(&self) -> Option<String> {
            self.hint.as_ref().map(|f| f().to_string())
        }
    }

    impl Activity for Launcher {
        fn start(&mut self, conn: &mut dyn Connection, _stor: &Storage) -> Result<()> {
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
                Response::SpawnFg(res) => {
                    assert_eq!(self.mode, SpawnMode::Foreground);
                    match res {
                        Ok((id, _, _)) => Some(id), // TODO: use out and err from FG process
                        Err(e) => {
                            return Err(format!(
                                "error in Launcher foreground spawn comm '{}': {e}",
                                self.comm
                            ));
                        }
                    }
                }
                Response::SpawnBg(res) => {
                    assert_ne!(self.mode, SpawnMode::Foreground);
                    match res {
                        Ok(id) => Some(id),
                        Err(e) => {
                            return Err(format!(
                                "error in Launcher background spawn comm '{}': {e}",
                                self.comm
                            ));
                        }
                    }
                }
                other => unreachable!(
                    "protocol exception: got bad agent response for Launcher start with comm '{}': {other:?}",
                    self.comm
                ),
            };

            Ok(())
        }

        fn stop(&mut self, conn: &mut dyn Connection, _stor: &Storage) -> Result<Option<PlotHint>> {
            match self.id {
                Some(id) => {
                    // no need to stop foreground processes, they are stopped already
                    if self.mode == SpawnMode::Foreground {
                        // TODO: add ability to provide plotting hint
                        return Ok(Some((id, self.get_hint())));
                    }

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
                            Ok(Some((id, self.get_hint())))
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
                None => unreachable!(),
            }
        }
    }

    struct PredefinedLauncher {
        name: &'static str,
        comm: String,
        mode: SpawnMode,
        args: Vec<String>,
    }

    impl ExportedActivity for PredefinedLauncher {
        fn name(&self) -> &'static str {
            self.name
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if !conf.is_empty() {
                return Err(format!(
                    "'{}' launcher is pre-defined and accepts empty config, but got {conf:?}",
                    self.name()
                ));
            }

            Ok(Box::new(Launcher {
                comm: self.comm.clone(),
                mode: self.mode,
                args: self.args.clone(),
                id: None,
                hint: None,
            }))
        }
    }

    struct GenericLauncher;

    impl ExportedActivity for GenericLauncher {
        fn name(&self) -> &'static str {
            "launch"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if conf.has_artifacts() {
                return Err(format!(
                    "'{}' expects no artifacts, but got some, in='{:?}', out='{:?}'",
                    self.name(),
                    conf.input,
                    conf.output
                ));
            }

            match conf.value {
                Some(ConfigValue::LaunchArgs {
                    comm,
                    mode,
                    args,
                    hint,
                }) => Ok(Box::new(Launcher {
                    comm,
                    args,
                    mode,
                    id: None,
                    hint: Some(Box::new(move || hint.clone())),
                })),
                None => Err(format!("'{}' expects config, but got none", self.name())),
                other => Err(format!(
                    "'{}' expects LaunchArgs confif, but got {other:?}",
                    self.name()
                )),
            }
        }
    }

    struct IostatLauncher {
        launcher: Launcher,
        devs_art_name: Option<String>,
    }

    impl Activity for IostatLauncher {
        fn start(&mut self, conn: &mut dyn Connection, stor: &Storage) -> Result<()> {
            if let Some(ref name) = self.devs_art_name {
                #[expect(clippy::infallible_destructuring_match)]
                let devices = match stor.get(name) {
                    ArtifactValue::StringList(devices) => devices,
                };
                // extend default iostat launcher with custom device paths
                self.launcher.args.extend_from_slice(&devices);
            }
            self.launcher.start(conn, stor)
        }

        fn stop(&mut self, conn: &mut dyn Connection, stor: &Storage) -> Result<Option<PlotHint>> {
            self.launcher.stop(conn, stor)
        }
    }

    struct IostatExport;

    impl ExportedActivity for IostatExport {
        fn name(&self) -> &'static str {
            "iostat"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if conf.has_value() {
                return Err(format!("iostat expect no value, but got {:?}", conf.value));
            }

            if conf.has_artifacts_out() {
                return Err(format!(
                    "iostat expect no output artifacts but got {:?}",
                    conf.output
                ));
            }

            let devs_art_name = conf.verify_optional_single_artifact_in("devices")?;

            Ok(Box::new(IostatLauncher {
                launcher: Launcher {
                    comm: String::from("iostat"),
                    mode: SpawnMode::BackgroundKill,
                    args: ["-d", "-t", "-x", "-m", "1"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                    id: None,
                    hint: None,
                },
                devs_art_name,
            }))
        }
    }

    struct FioExport;

    impl ExportedActivity for FioExport {
        fn name(&self) -> &'static str {
            "fio"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            let fiocfg = match conf.value {
                Some(ConfigValue::Ini(ini)) => ini,
                None => return Err("fio expects configuration value, but got none".to_string()),
                other => {
                    return Err(format!(
                        "fio expects INI-like configuration value, but got: {other:?}"
                    ));
                }
            };

            if fiocfg.sections.is_empty() {
                return Err("fio expects at least one section to run".to_string());
            }

            let mut args: Vec<String> = fiocfg.global.iter().map(|s| format!("--{s}")).collect();
            let mut bw_hint = vec![];
            let mut iops_hint = vec![];
            let mut lat_hint = vec![];

            for (name, config) in fiocfg.sections {
                args.push(format!("--name={name}"));
                for line in &config {
                    args.push(format!("--{line}"));

                    // select the right hint to update
                    let hint = if line.starts_with("write_bw_log=") {
                        &mut bw_hint
                    } else if line.starts_with("write_iops_log=") {
                        &mut iops_hint
                    } else if line.starts_with("write_lat_log=") {
                        &mut lat_hint
                    } else {
                        continue;
                    };

                    hint.push(line.split_once("=").unwrap().1.to_string());
                }
                args.extend_from_slice(
                    &config
                        .iter()
                        .map(|s| format!("--{s}"))
                        .collect::<Vec<String>>(),
                );
            }

            Ok(Box::new(Launcher {
                comm: String::from("fio"),
                mode: SpawnMode::BackgroundWait,
                args,
                id: None,
                hint: if bw_hint.is_empty() && iops_hint.is_empty() && lat_hint.is_empty() {
                    None
                } else {
                    Some(Box::new(move || {
                        [&bw_hint, &iops_hint, &lat_hint]
                            .iter()
                            .map(|h| h.join(":"))
                            .collect::<Vec<String>>()
                            .join(",")
                    }))
                },
            }))
        }
    }

    struct LookupPaths {
        pattern: String,
        out_paths: String,
    }

    impl Activity for LookupPaths {
        fn start(&mut self, conn: &mut dyn Connection, stor: &Storage) -> Result<()> {
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
            Ok(())
        }
    }

    struct LookupPathsExport;

    impl ExportedActivity for LookupPathsExport {
        fn name(&self) -> &'static str {
            "lookup_paths"
        }

        fn creator(&self, conf: ActivityConfig) -> Result<Box<dyn Activity + Send>> {
            if conf.has_artifacts_in() {
                return Err(format!(
                    "'{}' expects no input artifacts but got: {:?}",
                    self.name(),
                    conf.input
                ));
            }

            let pattern = match conf.value {
                Some(ConfigValue::String(ref pattern)) => pattern.clone(),
                other => {
                    return Err(format!(
                        "'{}' expects config value of type String, got: {other:?}",
                        self.name()
                    ));
                }
            };

            let out_paths = conf
                .verify_single_artifact_out("paths")
                .map_err(|e| format!("'{}' out 'paths': {e}", self.name()))?;

            Ok(Box::new(LookupPaths { pattern, out_paths }))
        }
    }

    pub fn export_all() -> ActivityDatabase {
        let exports: Vec<Box<dyn ExportedActivity>> = vec![
            // utilities
            Box::new(SleeperExport),
            // pollers
            Box::new(GenericPoller),
            Box::new(PredefinedPoller::new("proc_meminfo", "/proc/meminfo")),
            Box::new(PredefinedPoller::new("proc_net_dev", "/proc/net/dev")),
            // launchers
            Box::new(GenericLauncher),
            Box::new(PredefinedLauncher {
                name: "mpstat",
                comm: "mpstat".to_string(),
                mode: SpawnMode::BackgroundKill,
                args: ["-P", "ALL", "1"].into_iter().map(String::from).collect(),
            }),
            Box::new(PredefinedLauncher {
                name: "flamegraph",
                comm: "flamegraph".to_string(),
                mode: SpawnMode::BackgroundWait,
                args: ["-F", "99", "--", "--all-cpus", "sleep", "3"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            }),
            Box::new(IostatExport),
            Box::new(LookupPathsExport),
            Box::new(FioExport),
        ];

        let mut result: ActivityDatabase = HashMap::new();
        for e in exports {
            let res = result.insert(e.name(), Box::new(move |c| e.creator(c)));
            assert!(res.is_none());
        }
        result
    }
}
