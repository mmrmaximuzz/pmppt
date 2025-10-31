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

use std::path::Path;
use std::path::PathBuf;

use env_logger::Env;
use log::{error, info};

use pmppt::agent;
use pmppt::common::Res;
use pmppt::common::emsg;

fn find_max_numeric_dir(base: &Path) -> Res<u32> {
    let mut max_dir = 0;

    for dir in base
        .read_dir()
        .map_err(|e| format!("cannot read dir: {e}"))?
        .flatten()
    {
        let name = dir.file_name();
        match name.to_string_lossy().parse::<u32>() {
            Ok(value) => max_dir = std::cmp::max(max_dir, value),
            Err(_) => continue,
        }
    }

    Ok(max_dir)
}

fn create_outdir(base: &Path) -> Res<PathBuf> {
    if base.exists() && !base.is_dir() {
        return emsg(&format!(
            "path provided '{}' is not a directory",
            base.to_string_lossy()
        ));
    }

    let new_dir_num = if base.exists() {
        find_max_numeric_dir(base)? + 1
    } else {
        0
    };

    let new_dir = base.join(Path::new(&new_dir_num.to_string()));
    std::fs::create_dir_all(&new_dir).map_err(|e| format!("cannot create ouput dir {e}"))?;

    Ok(new_dir)
}

fn main_selfhosted(args: &[String]) -> Res<()> {
    use pmppt::agent::proto_impl::selfhosted;

    if args.len() != 2 {
        return emsg("usage: PROG local PATH_TO_CONFIG PATH_TO_OUTPUT");
    }

    let json_path = &args[0];
    let logs_path = PathBuf::from(&args[1]);
    let outdir = create_outdir(&logs_path)?;

    info!("agent is in selfhosted mode with config: {}", json_path);
    info!("output directory: {}", outdir.to_string_lossy());
    let proto = selfhosted::SelfHostedProtocol::from_json(json_path)?;
    let agent = agent::Agent::new(proto, outdir.clone());

    info!("starting the agent");
    agent.serve();

    info!("done, output directory: {}", outdir.to_string_lossy());
    Ok(())
}

fn main_wrapper(args: &[String]) -> Res<()> {
    // init log with Info level by default
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("pmppt-agent");

    if args.len() < 2 {
        return emsg("usage: PROG (selfhosted) ARGS...");
    }

    match args[1].as_str() {
        "selfhosted" => main_selfhosted(&args[2..]),
        _ => emsg("Only 'selfhosted' transport is supported"),
    }
}

fn main() {
    // TODO: here will be better CLI arguments parsing
    let args: Vec<String> = std::env::args().collect();
    if let Err(msg) = main_wrapper(&args) {
        error!("Error: {}", msg);
        std::process::exit(1);
    }
}
