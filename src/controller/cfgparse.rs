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

use crate::{common::Result, types::ConfigValue};
use serde::Deserialize;
use serde_yml;

/// Main structure describing just-parsed structure, may be incorrect for PMPPT run
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    pub setup: RawSetupConfig,
    pub runtime: RawRuntimeConfig,
}

impl RawConfig {
    pub fn parse(s: &str) -> Result<RawConfig> {
        serde_yml::from_str::<RawConfig>(s).map_err(|e| format!("failed to parse config: {e}"))
    }
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
pub type RawArgs = HashMap<String, serde_yml::Value>;
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RawActivityArgs {
    pub args: Option<RawArgs>,
    #[serde(rename = "in")]
    pub input: Option<HashMap<String, String>>,
    #[serde(rename = "out")]
    pub output: Option<HashMap<String, String>>,
}

pub type ActivityArgsParser = Box<dyn Fn(RawArgs) -> Result<ConfigValue>>;
pub type ParserDatabase = HashMap<&'static str, ActivityArgsParser>;

pub mod yaml_parsers {
    use std::collections::HashMap;
    use std::time::Duration;

    use crate::common::{Result, communication::SpawnMode};
    use crate::types::ConfigValue;

    use super::{ParserDatabase, RawArgs};

    trait ExportedYamlParser {
        fn name(&self) -> &'static str;
        fn parse(&self, args: RawArgs) -> Result<ConfigValue>;
    }

    struct NoArgsParser {
        name: &'static str,
    }

    impl NoArgsParser {
        fn new(name: &'static str) -> Self {
            Self { name }
        }
    }

    impl ExportedYamlParser for NoArgsParser {
        fn name(&self) -> &'static str {
            self.name
        }

        fn parse(&self, args: RawArgs) -> Result<ConfigValue> {
            if !args.is_empty() {
                return Err(format!(
                    "'{}' expects no args, but has {} keys",
                    self.name(),
                    args.len()
                ));
            };

            // shame
            Err(format!(
                "'{}' has no arguments just remove 'args' at all",
                self.name()
            ))
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum YamlValueExtractor {
        TimeDurationSecs,
        String,
    }

    impl YamlValueExtractor {
        fn try_extract(&self, val: &serde_yml::Value) -> Result<ConfigValue> {
            match (self, &val) {
                (YamlValueExtractor::TimeDurationSecs, serde_yml::Value::Number(n)) => Ok(
                    ConfigValue::Time(Duration::from_secs_f64(n.as_f64().unwrap())),
                ),
                (YamlValueExtractor::String, serde_yml::Value::String(s)) => {
                    Ok(ConfigValue::String(s.to_string()))
                }
                _ => Err(format!("expected value of type {self}, but got {val:?}",)),
            }
        }
    }

    impl std::fmt::Display for YamlValueExtractor {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let s = match self {
                YamlValueExtractor::TimeDurationSecs => "TimeDurationSeconds(float)",
                YamlValueExtractor::String => "String",
            };
            f.write_str(s)
        }
    }

    // parser YAML mapping with a single key
    struct SingleArgParser {
        name: &'static str,
        arg_name: String,
        arg_type: YamlValueExtractor,
    }

    impl SingleArgParser {
        fn new(name: &'static str, arg_name: &str, arg_type: YamlValueExtractor) -> Self {
            Self {
                name,
                arg_name: arg_name.to_string(),
                arg_type,
            }
        }
    }

    impl ExportedYamlParser for SingleArgParser {
        fn name(&self) -> &'static str {
            self.name
        }

        fn parse(&self, args: RawArgs) -> Result<ConfigValue> {
            if args.len() != 1 {
                return Err(format!(
                    "'{}' expects single-key map, but got {} keys",
                    self.name(),
                    args.len()
                ));
            };

            // single-key unpack
            if let Some((k, v)) = args.into_iter().take(1).next() {
                if k != self.arg_name {
                    return Err(format!(
                        "'{}' expected single key '{}' but got '{k}'",
                        self.name(),
                        self.arg_name
                    ));
                }

                return self.arg_type.try_extract(&v).map_err(|e| {
                    format!(
                        "'{}' failed to match single key '{}' of type {}: {e}",
                        self.name(),
                        self.arg_name,
                        self.arg_type
                    )
                });
            }
            unreachable!()
        }
    }

    struct MappingArgParser {
        exp_args: HashMap<String, (YamlValueExtractor, bool)>,
    }

    impl MappingArgParser {
        fn new(args: &[(&str, (YamlValueExtractor, bool))]) -> Self {
            let mut argmap = HashMap::new();
            for (a, (ext, opt)) in args {
                let res = argmap.insert(a.to_string(), (*ext, *opt));
                assert!(res.is_none())
            }

            Self { exp_args: argmap }
        }

        fn parse(&self, input: RawArgs) -> Result<HashMap<String, ConfigValue>> {
            let mut result = HashMap::new();
            for (key, val) in &input {
                let (exp_type, _) = match self.exp_args.get(key) {
                    Some(exp_type) => exp_type,
                    None => return Err(format!("found unknown key '{key}'")),
                };
                let v = exp_type
                    .try_extract(val)
                    .map_err(|e| format!("bad value for key '{key}': {e}"))?;
                result.insert(key.to_string(), v);
            }

            for (key, (_, required)) in &self.exp_args {
                if *required && !input.contains_key(key) {
                    return Err(format!("failed to find required key '{key}'"));
                }
            }

            Ok(result)
        }
    }

    struct GenericPollerParser;
    impl ExportedYamlParser for GenericPollerParser {
        fn name(&self) -> &'static str {
            "poller"
        }

        fn parse(&self, args: RawArgs) -> Result<ConfigValue> {
            let values = MappingArgParser::new(&[
                ("pattern", (YamlValueExtractor::String, true)),
                ("hint", (YamlValueExtractor::String, false)),
            ])
            .parse(args)?;

            let ConfigValue::String(pattern) = values["pattern"].clone() else {
                unreachable!()
            };
            let hint = values.get("hint").map(|h| {
                let ConfigValue::String(hint) = h.clone() else {
                    unreachable!()
                };
                hint
            });

            Ok(ConfigValue::PollArgs { pattern, hint })
        }
    }

    struct GenericLaunchParser;
    impl ExportedYamlParser for GenericLaunchParser {
        fn name(&self) -> &'static str {
            "launch"
        }

        fn parse(&self, args: RawArgs) -> Result<ConfigValue> {
            let values = MappingArgParser::new(&[
                ("comm", (YamlValueExtractor::String, true)),
                ("mode", (YamlValueExtractor::String, true)),
                ("hint", (YamlValueExtractor::String, false)),
            ])
            .parse(args)?;

            let ConfigValue::String(comm) = values["comm"].clone() else {
                unreachable!()
            };
            let ConfigValue::String(mode) = values["mode"].clone() else {
                unreachable!()
            };
            let mode = match mode.as_str() {
                "fg" => SpawnMode::Foreground,
                "bgkill" => SpawnMode::BackgroundKill,
                "bgwait" => SpawnMode::BackgroundWait,
                other => return Err(format!("bad launch mode: {other}")),
            };
            let hint = values.get("hint").map(|h| match h {
                ConfigValue::String(hint) => hint.clone(),
                _ => unreachable!(),
            }).unwrap_or_default();

            Ok(ConfigValue::LaunchArgs { comm, mode, args: vec![], hint })
        }
    }

    pub fn export_all() -> ParserDatabase {
        let parsers: Vec<Box<dyn ExportedYamlParser>> = vec![
            Box::new(NoArgsParser::new("mpstat")),
            Box::new(NoArgsParser::new("iostat")),
            Box::new(NoArgsParser::new("proc_net_dev")),
            Box::new(NoArgsParser::new("proc_meminfo")),
            Box::new(NoArgsParser::new("flamegraph")),
            Box::new(SingleArgParser::new(
                "sleep",
                "secs",
                YamlValueExtractor::TimeDurationSecs,
            )),
            Box::new(SingleArgParser::new(
                "lookup_paths",
                "pattern",
                YamlValueExtractor::String,
            )),
            Box::new(GenericPollerParser),
            Box::new(GenericLaunchParser),
        ];

        let mut result: ParserDatabase = HashMap::new();
        for parser in parsers {
            let res = result.insert(parser.name(), Box::new(move |v| parser.parse(v)));
            assert!(res.is_none());
        }
        result
    }
}

#[cfg(test)]
mod test {
    use std::net::Ipv4Addr;

    use indoc::indoc;

    use super::RawConfig;

    #[test]
    fn must_not_accept_empty() {
        let cfg = "";
        RawConfig::parse(cfg).unwrap_err();
    }

    #[test]
    fn should_parse_trivial_simplest() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
        "};
        let cfg = RawConfig::parse(cfg).unwrap();
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
        RawConfig::parse(cfg).unwrap_err();
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
        RawConfig::parse(cfg).unwrap_err();
    }

    #[test]
    fn should_parse_trivial_empty_stage() {
        let cfg = indoc! {"
            setup:
              agents:
            runtime:
              - stage0:
        "};
        let cfg = RawConfig::parse(cfg).expect("failed to parse");
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
        let cfg = RawConfig::parse(cfg).expect("failed to parse");
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
        let cfg = RawConfig::parse(cfg).expect("failed to parse");
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
