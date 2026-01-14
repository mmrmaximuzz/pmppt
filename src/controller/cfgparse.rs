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

use std::{collections::HashMap, net::IpAddr};

use crate::common::Result;
use serde::Deserialize;
use serde_yml;

use super::activity::ActivityConfig;

/// Main structure describing just-parsed structure, may be incorrect for PMPPT run
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    pub setup: RawSetupConfig,
    pub runtime: RawRuntimeConfig,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RawSetupConfig {
    pub agents: HashMap<AgentId, AgentConfig>,
}
pub type RawRuntimeConfig = Vec<HashMap<StageName, HashMap<AgentId, ActivityChain>>>;

pub type AgentId = String;
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    pub ip: IpAddr,
    pub port: u16,
}

pub type StageName = String;
pub type ActivityName = String;
pub type ActivityChain = Vec<HashMap<ActivityName, RawActivityArgs>>;
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RawActivityArgs {
    pub args: Option<serde_yml::Value>,
    #[serde(rename = "in")]
    pub input: Option<HashMap<String, String>>,
    #[serde(rename = "out")]
    pub output: Option<HashMap<String, String>>,
}

pub type ActivityParser = Box<dyn Fn(RawActivityArgs) -> Result<ActivityConfig>>;

pub fn parse_raw_config(s: &str) -> Result<RawConfig> {
    serde_yml::from_str::<RawConfig>(s).map_err(|e| format!("failed to parse config: {e}"))
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use indoc::indoc;

    use super::parse_raw_config;

    #[test]
    fn must_not_accept_empty() {
        let cfg = "";
        parse_raw_config(cfg).unwrap_err();
    }

    #[test]
    fn should_parse_trivial_simplest() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
        "};
        let cfg = parse_raw_config(cfg).unwrap();
        assert!(cfg.setup.agents.is_empty());
        assert_eq!(cfg.runtime.len(), 0);
    }

    #[test]
    fn should_not_parse_setup_extra_fields() {
        let cfg = indoc! {"
            setup:
              agents:
              extra_field:
            runtime:
        "};
        parse_raw_config(cfg).unwrap_err();
    }

    #[test]
    fn should_not_parse_agent_extra_fields() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 50000
                  extra: field
            runtime:
        "};
        parse_raw_config(cfg).unwrap_err();
    }

    #[test]
    fn should_parse_trivial_empty_stage() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
              - stage0:
        "};
        let cfg = parse_raw_config(cfg).expect("failed to parse");
        assert!(cfg.setup.agents.is_empty());
        assert_eq!(cfg.runtime.len(), 1);
        assert_eq!(cfg.runtime[0].len(), 1);
        assert!(cfg.runtime[0].contains_key("stage0"));
        assert_eq!(cfg.runtime[0]["stage0"].len(), 0);
    }

    #[test]
    fn should_parse_single_agent_empty_chain() {
        let cfg = indoc! {"
            setup:
              agents:
                a0:
                  ip: 127.0.0.1
                  port: 50000
            runtime:
              - stage0:
                  a0:
        "};
        let cfg = parse_raw_config(cfg).expect("failed to parse");
        assert!(cfg.setup.agents.contains_key("a0"));
        assert_eq!(cfg.runtime.len(), 1);
        assert_eq!(cfg.runtime[0].len(), 1);
        assert!(cfg.runtime[0].contains_key("stage0"));
    }

    #[test]
    fn should_parse_full_config() {
        let cfg = indoc! {"
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
        let cfg = parse_raw_config(cfg).expect("failed to parse");
        assert_eq!(cfg.setup.agents.len(), 2);
        assert_eq!(cfg.setup.agents["a1"].ip, Ipv4Addr::LOCALHOST);
        assert_eq!(cfg.setup.agents["a2"].ip, Ipv4Addr::LOCALHOST);
        assert_eq!(cfg.setup.agents["a1"].port, 50001);
        assert_eq!(cfg.setup.agents["a2"].port, 50002);

        assert_eq!(cfg.runtime.len(), 3);
        assert_eq!(cfg.runtime[0].len(), 1);
        assert_eq!(cfg.runtime[1].len(), 1);
        assert_eq!(cfg.runtime[2].len(), 1);
        assert!(cfg.runtime[0].contains_key("prepare"));
        assert!(cfg.runtime[1].contains_key("bench"));
        assert!(cfg.runtime[2].contains_key("prefinal"));

        assert!(cfg.runtime[0]["prepare"].contains_key("a1"));
        assert!(cfg.runtime[0]["prepare"].contains_key("a2"));
        assert!(!cfg.runtime[1]["bench"].contains_key("a1"));
        assert!(cfg.runtime[1]["bench"].contains_key("a2"));
        assert!(cfg.runtime[2]["prefinal"].contains_key("a1"));
        assert!(cfg.runtime[2]["prefinal"].contains_key("a2"));
    }
}
