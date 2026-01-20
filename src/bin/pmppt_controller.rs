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

use std::{
    env,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    str::FromStr,
};

use pmppt::{
    common::{self, Result},
    controller::{self, activity::ActivityDatabase, cfgparse::ParserDatabase},
};

fn main() {
    if let Err(msg) = main_wrapper() {
        eprintln!("PMPTT controller failed the execution: {msg}.");
        std::process::exit(1);
    }
}

fn main_wrapper() -> Result<()> {
    let (config_path, base_outdir_path) = parse_cli_args()?;

    let parsers_db = get_parsers();
    let activities_db = get_activities();

    let raw_config_str = read_config_file(&config_path)?;
    let raw_cfg = controller::cfgparse::RawConfig::parse(&raw_config_str)
        .map_err(|e| format!("failed to parse raw config: {e}"))?;
    let (agents_cfg, runtime_cfg) = controller::verify_config(raw_cfg, parsers_db)
        .map_err(|e| format!("failed to validate config: {e}"))?;

    let runtime = controller::create_runtime(runtime_cfg, activities_db)
        .map_err(|e| format!("failed to create runtime: {e}"))?;
    let agents = controller::connect_agents(agents_cfg)
        .map_err(|e| format!("failed to connect agents: {e}"))?;
    let outdir = common::create_next_numeric_dir_in(&base_outdir_path)
        .map_err(|e| format!("failed to create output dir: {e}"))?;

    controller::run(agents, runtime, &outdir).map_err(|e| format!("error while running: {e}"))?;
    println!("completed: output directory {outdir:?}");
    Ok(())
}

fn parse_cli_args() -> Result<(PathBuf, PathBuf)> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        return Err(format!("usage: {} PATH_TO_CONFIG PATH_TO_OUTPUT", args[0]));
    }

    let cfgpath = PathBuf::from_str(&args[1]).map_err(|e| format!("bad config path: {e}"))?;
    let outpath = PathBuf::from_str(&args[2]).map_err(|e| format!("bad output path: {e}"))?;

    Ok((cfgpath, outpath))
}

fn get_parsers() -> ParserDatabase {
    use crate::controller::cfgparse::yaml_parsers;
    yaml_parsers::export_all()
}

fn get_activities() -> ActivityDatabase {
    use crate::controller::activity::default_activities;
    default_activities::export_all()
}

fn read_config_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| format!("failed to open config '{path:?}: {e}"))?;

    let mut config = String::with_capacity(4096);
    file.read_to_string(&mut config)
        .map_err(|e| format!("failed to read {path:?}: {e}"))?;

    Ok(config)
}
