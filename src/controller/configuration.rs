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

use serde::Deserialize;
use serde_yml;

use crate::common::Res;

pub type AgentId = String;

/// Main structure describing PMPPT launch
#[derive(Deserialize, Debug)]
pub struct Config {
    pub setup: Setup,
    pub run: Run,
}

#[derive(Deserialize, Debug)]
pub struct Setup {
    pub agents: HashMap<AgentId, AgentConfig>,
    pub params: Option<HashMap<String, serde_yml::Value>>,
}

#[derive(Deserialize, Debug)]
pub struct AgentConfig {
    pub ip: IpAddr,
    pub port: u16,
}

#[derive(Deserialize, Debug)]
pub struct RunStage {}

pub type Run = Vec<HashMap<AgentId, RunStage>>;

pub fn parse_config(config_str: &str) -> Res<Config> {
    serde_yml::from_str(config_str).map_err(|e| format!("failed to parse config file: {e}"))?
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use super::parse_config;

    #[test]
    fn must_not_accept_empty_content() {
        let empty = "";
        let Err(_) = parse_config(empty) else {
            panic!("must not parse empty content")
        };
    }

    #[test]
    fn should_parse_trivial_config() {
        let trivial = "
        setup:\n
          agents:\n
        run:\n
          - somestring:\n";
        let cfg = parse_config(trivial).expect("failed to parse trivial config");
        assert!(cfg.setup.agents.is_empty());
        assert_eq!(cfg.setup.params, None);
        assert_eq!(cfg.run.len(), 1);
        assert_eq!(cfg.run[0].len(), 1);
        assert!(cfg.run[0].contains_key("somestring"));
    }

    #[test]
    fn should_parse_ok_config() {
        let trivial = "
        setup:\n
          agents:\n
            a1:\n
              ip: 127.0.0.1\n
              port: 50000\n
            a2:\n
              ip: 127.0.0.1\n
              port: 50001\n
            a3:\n
              ip: 127.0.0.1\n
              port: 60000\n
          params:\n
            TEST_TIME_SECS: 600\n
        run:\n
          - a1:\n
            a2:\n
          - a3:\n";
        let localhost = Ipv4Addr::new(127, 0, 0, 1);

        let cfg = parse_config(trivial).expect("failed to parse minimal config");
        let params = cfg.setup.params.expect("failed to parse 'params' section");
        assert_eq!(cfg.setup.agents.len(), 3);
        assert_eq!(cfg.setup.agents["a1"].ip, localhost);
        assert_eq!(cfg.setup.agents["a2"].ip, localhost);
        assert_eq!(cfg.setup.agents["a3"].ip, localhost);
        assert_eq!(cfg.setup.agents["a1"].port, 50000);
        assert_eq!(cfg.setup.agents["a2"].port, 50001);
        assert_eq!(cfg.setup.agents["a3"].port, 60000);
        assert_eq!(params.len(), 1);
        assert_eq!(params["TEST_TIME_SECS"], 600);
        assert_eq!(cfg.run.len(), 2);
        assert_eq!(cfg.run[0].len(), 2);
        assert_eq!(cfg.run[1].len(), 1);
        assert!(cfg.run[0].contains_key("a1"));
        assert!(cfg.run[0].contains_key("a2"));
        assert!(cfg.run[1].contains_key("a3"));
    }
}
