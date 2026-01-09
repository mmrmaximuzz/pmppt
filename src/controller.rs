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

use std::collections::HashMap;

use activity::ActivityConfig;
use cfgparse::{
    ActivityChain, AgentConfig, AgentId, RawActivityArgs, RawConfig, RawRuntimeConfig,
    RawSetupConfig,
};
use connection::Connection;

use crate::common::Result;

pub type AgentConnections = HashMap<AgentId, Box<dyn Connection>>;

pub fn connect_agents(cfg: &HashMap<AgentId, AgentConfig>) -> Result<AgentConnections> {
    // do not show actual implementation to external code
    use crate::controller::connection::tcpmsgpack::TcpMsgpackConnection;
    use std::net::TcpStream;

    let mut conns = HashMap::default();
    for (name, params) in cfg {
        let ip = params.ip;
        let port = params.port;
        let conn = TcpStream::connect((ip, port))
            .map_err(|e| format!("failed to connect agent '{name}' ({ip}, {port}): {e}"))?;
        conns.insert(
            name.clone(),
            Box::new(TcpMsgpackConnection::from_conn(conn)) as Box<dyn Connection>,
        );
    }
    Ok(conns)
}

pub type AgentsConfiguration = HashMap<AgentId, AgentConfig>;
pub type RuntimeConfiguration = Vec<()>;

pub fn verify_config(raw_cfg: RawConfig) -> Result<(AgentsConfiguration, RuntimeConfiguration)> {
    let setup_cfg =
        verify_setup_config(raw_cfg.setup).map_err(|e| format!("bad 'setup' config: {e}"))?;
    let run_cfg = verify_runtime_config(raw_cfg.runtime, &setup_cfg)
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
        for (name, activities) in stage.drain().take(1) {
            let stage = verify_runtime_stage(activities, agents)
                .map_err(|e| format!("bad stage '{name}': {e}"))?;
            stages.push(stage);
        }
    }
    Ok(stages)
}

fn verify_runtime_stage(
    mut activities: HashMap<String, ActivityChain>,
    agents: &AgentsConfiguration,
) -> Result<()> {
    for (agent, chain) in activities.drain() {
        if !agents.contains_key(&agent) {
            return Err(format!("agent '{agent}' not found"));
        }

        for (i, activity) in chain.into_iter().enumerate() {
            verify_activity(activity).map_err(|e| format!("bad activity #{i}: {e}"))?;
        }
    }

    Ok(())
}

fn verify_activity(
    mut activity: HashMap<String, RawActivityArgs>,
) -> Result<(String, ActivityConfig)> {
    if activity.len() != 1 {
        return Err(format!(
            "activity format expects map with single item (activity name), but got {} items",
            activity.len()
        ));
    }

    // extract its single item
    for (name, args) in activity.drain().take(1) {
        dbg!((name, args));
    }

    Err(String::new())
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use indoc::indoc;

    use crate::controller::{
        cfgparse::parse_raw_config, verify_runtime_config, verify_setup_config,
    };

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
                - cpu:
                - memory:
              a2:
                - cpu:
                - memory:
          - bench:
              a2:
                - get_info:
                    out:
                      output_artifact: ARTIFACT_NAME
                - bench_me:
                    args:
                      arg1: value1
                      arg2: value2
                    in:
                      input_artifact: ARTIFACT_NAME
          - prefinal:
              a1:
                - collect:
              a2:
                - collect:
    "};

    #[test]
    fn should_not_verify_empty_agents() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
        "};

        let cfg = parse_raw_config(cfg).unwrap();
        verify_setup_config(cfg.setup).unwrap_err();
    }

    #[test]
    fn verify_agents_ok() {
        let cfg = parse_raw_config(OK_EXAMPLE).unwrap();
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

        let cfg = parse_raw_config(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup).unwrap_err();
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

        let cfg = parse_raw_config(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup).unwrap_err();
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

        let cfg = parse_raw_config(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup).unwrap_err();
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

        let cfg = parse_raw_config(cfg).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup).unwrap_err();
    }

    #[test]
    fn verify_runtime_ok() {
        let cfg = parse_raw_config(OK_EXAMPLE).unwrap();
        let setup = verify_setup_config(cfg.setup).unwrap();
        verify_runtime_config(cfg.runtime, &setup).unwrap();
    }
}
