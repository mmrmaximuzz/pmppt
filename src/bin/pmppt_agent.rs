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

use std::net::Ipv4Addr;
use std::net::TcpListener;
use std::path::PathBuf;

use env_logger::Env;
use log::{error, info};

use pmppt::agent::Agent;
use pmppt::common::Result;
use pmppt::common::create_next_numeric_dir_in;
use pmppt::common::emsg;

fn main_selfhosted(args: &[String]) -> Result<()> {
    use pmppt::agent::proto_impl::selfhosted;

    if args.len() != 2 {
        return emsg("usage: PROG selfhosted PATH_TO_CONFIG PATH_TO_OUTPUT");
    }

    let json_path = &args[0];
    let logs_path = PathBuf::from(&args[1]);
    let outdir = create_next_numeric_dir_in(&logs_path)?;

    info!("agent is in selfhosted mode with config: {json_path}");
    info!("output directory: {outdir:?}");
    let proto = selfhosted::SelfHostedProtocol::from_json(json_path)?;
    let agent = Agent::new(proto, outdir.clone());

    info!("starting the agent");
    agent.serve();

    info!("done, output directory: {outdir:?}");
    Ok(())
}

fn main_tcp_msgpack(args: &[String]) -> Result<()> {
    use pmppt::agent::proto_impl::tcpmsgpack;

    if args.len() != 2 {
        return emsg("usage: PROG tcp-msgpack LOCAL_TCP_PORT PATH_TO_OUTPUT");
    }

    let port = args[0]
        .parse::<u16>()
        .map_err(|e| format!("cannot get TCP port to listen: {e}"))?;
    let logs_path = PathBuf::from(&args[1]);

    info!("starting agent in tcp-msgpack mode: port={port}, output={logs_path:?}");

    let listener = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 1), port))
        .map_err(|e| format!("cannot bind TCP listener on port {port}: {e}"))?;

    loop {
        info!("waiting for new connection from controller");

        let (conn, from) = listener
            .accept()
            .map_err(|e| format!("TCP listen failed: {e}"))?;
        info!("got new connection from {from}");

        let outdir = create_next_numeric_dir_in(&logs_path)?;
        let proto = tcpmsgpack::TcpMsgpackProtocol::from_conn(conn);
        let agent = Agent::new(proto, outdir.clone());
        agent.serve();
        info!("done with {from}, logs here: {outdir:?}");
    }
}

fn main_wrapper(args: &[String]) -> Result<()> {
    // init log with Info level by default
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("pmppt-agent starting");

    if args.len() < 2 {
        return emsg("usage: PROG (selfhosted|tcp-msgpack) ARGS...");
    }

    match args[1].as_str() {
        "selfhosted" => main_selfhosted(&args[2..]),
        "tcp-msgpack" => main_tcp_msgpack(&args[2..]),
        other => Err(format!("unsupported agent mode '{other}'")),
    }
}

fn main() {
    // TODO: here will be better CLI arguments parsing
    let args: Vec<String> = std::env::args().collect();
    if let Err(msg) = main_wrapper(&args) {
        error!("Error: {msg}");
        std::process::exit(1);
    }
}
