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

use std::{env, fs::File, io::Read, path::PathBuf, str::FromStr};

use pmppt::{
    common::{Res, emsg},
    controller::{activity, configuration, connection},
};

fn main() {
    if let Err(msg) = main_wrapper() {
        eprintln!("Error occured while running PMPTT controller: {msg}.");
        std::process::exit(1);
    }
}

fn main_wrapper() -> Res<()> {
    let config_path_str = parse_cli_args()?;
    let config_str = read_config_file(config_path_str)?;
    let cfg = configuration::parse_config(&config_str)?;
    run(cfg)
}

fn parse_cli_args() -> Res<String> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return emsg(&format!("usage: {} PATH_TO_CONFIG", args[0]));
    }

    Ok(args[1].clone())
}

fn read_config_file(pathstr: String) -> Res<String> {
    let config_path =
        PathBuf::from_str(&pathstr).map_err(|e| format!("bad path provided '{pathstr}: {e}"))?;

    let mut file = File::open(config_path)
        .map_err(|e| format!("failed to to open config path '{pathstr}: {e}"))?;

    let mut config = String::with_capacity(8192);
    file.read_to_string(&mut config)
        .map_err(|e| format!("failed to read file {pathstr}: {e}"))?;

    Ok(config)
}

fn run(cfg: configuration::Config) -> Res<()> {
    println!("{cfg:?}");
    activity::process_run(&cfg.run)?;
    connection::connect_agents(&cfg.setup.agents)?;
    Ok(())
}
