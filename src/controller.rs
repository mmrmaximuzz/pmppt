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

pub mod activity;
pub mod cfgparse;
pub mod connection;
pub mod storage;

use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
    thread,
};

use activity::{Activity, ActivityConfig, ActivityDatabase};
use cfgparse::{
    ActivityChain, AgentConfig, AgentId, ParserDatabase, RawActivityArgs, RawConfig,
    RawRuntimeConfig, RawSetupConfig,
};
use connection::Connection;
use storage::Storage;

use crate::common::Result;

pub type AgentConnections = HashMap<AgentId, Arc<Mutex<Box<dyn Connection + Send>>>>;

pub fn connect_agents(cfg: HashMap<AgentId, AgentConfig>) -> Result<AgentConnections> {
    // do not show actual implementation to external code
    use crate::controller::connection::tcpmsgpack::TcpMsgpackConnection;
    use std::net::TcpStream;

    let mut conns = HashMap::default();
    for (name, params) in cfg {
        let ip = params.ip;
        let port = params.port;
        let conn = TcpStream::connect((ip, port))
            .map_err(|e| format!("agent '{name}' ({ip}, {port}) error: {e}"))?;
        conns.insert(
            name.clone(),
            Arc::new(Mutex::new(
                Box::new(TcpMsgpackConnection::from_conn(conn)) as Box<dyn Connection + Send>
            )),
        );
    }
    Ok(conns)
}

pub type AgentsConfiguration = HashMap<AgentId, AgentConfig>;
pub type ActivityChainConfiguration = Vec<(String, ActivityConfig)>;
pub type StageConfiguragion = HashMap<String, ActivityChainConfiguration>;
pub type RuntimeConfiguration = Vec<(String, StageConfiguragion)>;

pub type Runtime = Vec<(String, HashMap<String, Vec<(String, Box<dyn Activity + Send>)>>)>;

pub fn verify_config(
    raw_cfg: RawConfig,
    parsers: ParserDatabase,
) -> Result<(AgentsConfiguration, RuntimeConfiguration)> {
    let setup_cfg =
        verify_setup_config(raw_cfg.setup).map_err(|e| format!("bad 'setup' config: {e}"))?;
    let run_cfg = verify_runtime_config(raw_cfg.runtime, &setup_cfg, parsers)
        .map_err(|e| format!("bad 'runtime' config: {e}"))?;
    Ok((setup_cfg, run_cfg))
}

fn verify_setup_config(setup: RawSetupConfig) -> Result<AgentsConfiguration> {
    if setup.agents.is_empty() {
        return Err("expected at least one agent in 'setup', but got none".to_string());
    }
    Ok(setup.agents)
}

fn verify_runtime_config(
    run: RawRuntimeConfig,
    agents: &AgentsConfiguration,
    parsers: ParserDatabase,
) -> Result<RuntimeConfiguration> {
    if run.is_empty() {
        return Err("expected at least one stage in 'runtime', but got none".to_string());
    }

    let mut stages = vec![];
    for (i, mut stage) in run.into_iter().enumerate() {
        if stage.len() != 1 {
            return Err(format!(
                "bad stage #{i} - map with single item (stage name) expected, but got {}",
                stage.len()
            ));
        }

        // process single map item
        for (stage_name, activities) in stage.drain().take(1) {
            let stage = verify_runtime_stage(activities, agents, &parsers)
                .map_err(|e| format!("bad stage '{stage_name}': {e}"))?;
            stages.push((stage_name, stage));
        }
    }
    Ok(stages)
}

fn verify_runtime_stage(
    mut activities: HashMap<String, ActivityChain>,
    agents: &AgentsConfiguration,
    parsers: &ParserDatabase,
) -> Result<HashMap<String, Vec<(String, ActivityConfig)>>> {
    let mut stage = HashMap::new();
    for (agent, chain) in activities.drain() {
        if !agents.contains_key(&agent) {
            return Err(format!("agent '{agent}' not found"));
        }

        let mut activities: Vec<(String, ActivityConfig)> = vec![];
        for (i, activity) in chain.into_iter().enumerate() {
            let activity = verify_activity(activity, parsers)
                .map_err(|e| format!("bad activity #{i}: {e}"))?;
            activities.push(activity);
        }
        let res = stage.insert(agent, activities);
        assert!(res.is_none());
    }

    Ok(stage)
}

fn verify_activity(
    mut activity: HashMap<String, RawActivityArgs>,
    parsers: &ParserDatabase,
) -> Result<(String, ActivityConfig)> {
    if activity.len() != 1 {
        return Err(format!(
            "activity format expects map with single item (activity name), but got {} items",
            activity.len()
        ));
    }

    // extract its single item
    if let Some((name, args)) = activity.drain().take(1).next() {
        let parser = match parsers.get(name.as_str()) {
            None => return Err(format!("parser not found for {name} activity")),
            Some(parser) => parser,
        };
        let argvalue = match args.args {
            Some(val) => Some(parser(val)?),
            None => None,
        };
        return Ok((
            name,
            ActivityConfig {
                value: argvalue,
                input: args.input,
                output: args.output,
            },
        ));
    }
    unreachable!()
}

pub fn create_runtime(
    runtime_cfg: RuntimeConfiguration,
    activities: ActivityDatabase,
) -> Result<Runtime> {
    let mut result = Vec::with_capacity(runtime_cfg.len());
    for (stage_name, stage_cfg) in runtime_cfg {
        let mut stage = HashMap::new();
        for (agent, chain_cfg) in stage_cfg {
            let mut chain = Vec::with_capacity(chain_cfg.len());
            for (activity_name, activity_cfg) in chain_cfg {
                let factory = activities
                    .get(activity_name.as_str())
                    .ok_or_else(|| format!("failed to find factory for '{activity_name}'"))?;
                let activity = factory(activity_cfg)
                    .map_err(|e| format!("failed to construct '{activity_name}': {e}"))?;
                chain.push((activity_name, activity));
            }
            stage.insert(agent, chain);
        }
        result.push((stage_name, stage));
    }
    Ok(result)
}

// TODO: refactor please
pub fn run(mut agents: AgentConnections, mut runtime: Runtime, outdir: &Path) -> Result<()> {
    let storage = Storage::default();

    // run stages
    for (stage_name, stage) in &mut runtime {
        println!("Staring stage '{stage_name}'");

        thread::scope(|s| -> Result<()> {
            let mut handles = Vec::with_capacity(stage.len());
            for (agent, chain) in stage {
                let stor = &storage;
                let conn = agents.get_mut(agent).unwrap().clone();
                let handle = s.spawn(move || {
                    let mut conn = conn.lock().unwrap();
                    for (activity_name, activity) in chain {
                        activity
                            .start(conn.as_mut(), stor)
                            .map_err(|e| format!("agent {agent}, activity {activity_name}: {e}"))
                            .unwrap()
                    }
                });
                handles.push((agent, handle));
            }

            for (agent_name, handle) in handles {
                if let Err(e) = handle.join() {
                    return Err(format!("error in agent {agent_name}: {e:?}"));
                };
            }
            Ok(())
        })
        .map_err(|e| format!("failed execution: {e}"))?;
    }

    // stop all activities in stages and collect all hints for plotting
    runtime.reverse();
    let mut total_hints = vec![];
    for (stage_name, stage) in &mut runtime {
        println!("Stopping stage '{stage_name}'");

        let hints = thread::scope(|s| {
            let mut result = vec![];
            let mut handles = Vec::with_capacity(stage.len());
            for (agent, chain) in stage {
                let stor = &storage;
                let conn = agents.get_mut(agent).unwrap().clone();

                // stop tasks in reverse order as well
                chain.reverse();
                let handle = s.spawn(move || {
                    let mut hints = vec![];
                    let mut conn = conn.lock().unwrap();
                    for (activity_name, activity) in chain {
                        let hint = activity
                            .stop(conn.as_mut(), stor)
                            .map_err(|e| format!("agent {agent}, activity {activity_name}: {e}"))
                            .unwrap();
                        if let Some(hint) = hint {
                            hints.push((activity_name.clone(), hint))
                        }
                    }
                    hints
                });
                handles.push((agent, handle));
            }

            for (agent_name, handle) in handles {
                match handle.join() {
                    Ok(hints) => result.push((agent_name.clone(), hints)),
                    Err(e) => return Err(format!("error in agent {agent_name}: {e:?}")),
                };
            }
            Ok(result)
        })
        .map_err(|e| format!("failed execution: {e}"))?;
        total_hints.push(hints);
    }

    println!("Collecting data from agents");

    // optimize it for one-traverse loop
    for (agent_name, conn) in agents {
        let agent_path = outdir.join(&agent_name);
        std::fs::create_dir(&agent_path).expect("failed to create dir for agent");

        let data = connection::collect_data(conn.lock().unwrap().as_mut())
            .map_err(|e| format!("failed to collect data from {agent_name}: {e}"))?;
        File::create(agent_path.join("out.tgz"))
            .unwrap()
            .write_all(&data)
            .unwrap();
        drop(data);

        for agent_hints in &total_hints {
            for (agent, hints) in agent_hints {
                if *agent != agent_name {
                    continue;
                }
                let mut file = File::create(agent_path.join("out.map")).unwrap();
                for (activity, (id, hint)) in hints {
                    file.write_all(
                        format!(
                            "{id:03} {activity} {}",
                            if let Some(h) = hint { h } else { "" }
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use indoc::indoc;

    use crate::controller::{cfgparse::RawConfig, verify_runtime_config, verify_setup_config};

    use super::cfgparse::{ParserDatabase, yaml_parsers};

    const OK_EXAMPLE: &str = indoc! {"
        setup:
          agents:
            a1:
              ip: 127.0.0.1
              port: 50001
            a2:
              ip: 127.0.0.1
              port: 50002
        runtime:
          - prepare:
              a1:
                - mpstat:
                - proc_meminfo:
              a2:
                - mpstat:
                - proc_meminfo:
          - bench:
              a2:
                - lookup_paths:
                    out:
                      output_artifact: PATHS
                - iostat:
                    in:
                      input_artifact: PATHS
    "};

    fn get_parsers() -> ParserDatabase {
        yaml_parsers::export_all()
    }

    #[test]
    fn should_not_verify_empty_agents() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
        "};

        let cfg = RawConfig::parse(cfg).unwrap();
        verify_setup_config(cfg.setup).unwrap_err();
    }

    #[test]
    fn verify_agents_ok() {
        let cfg = RawConfig::parse(OK_EXAMPLE).unwrap();
        let agents = verify_setup_config(cfg.setup).unwrap();
        assert_eq!(agents["a1"].ip, Ipv4Addr::LOCALHOST);
        assert_eq!(agents["a1"].port, 50001);
        assert_eq!(agents["a2"].ip, Ipv4Addr::LOCALHOST);
        assert_eq!(agents["a2"].port, 50002);
    }

    #[test]
    fn should_not_verify_empty_runtime() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 8080
            runtime:
        "};

        let cfg = RawConfig::parse(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup, get_parsers()).unwrap_err();
    }

    #[test]
    fn should_not_verify_multistage_runtime() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 8080
            runtime:
              - normal_stage:
              - stage_with:
                some_another_key:
        "};

        let cfg = RawConfig::parse(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup, get_parsers()).unwrap_err();
    }

    #[test]
    fn should_not_verify_bad_agent_runtime() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 8080
            runtime:
              - stage:
                  bad_agent:
                    - activity:
        "};

        let cfg = RawConfig::parse(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup, get_parsers()).unwrap_err();
    }

    #[test]
    fn should_not_verify_multiactivity_runtime() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 8080
            runtime:
              - stage:
                  a0:
                    - activity:
                      with_some_another_key:
        "};

        let cfg = RawConfig::parse(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup, get_parsers()).unwrap_err();
    }

    #[test]
    fn verify_runtime_ok() {
        let cfg = RawConfig::parse(OK_EXAMPLE).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup, get_parsers()).unwrap();
    }
}
