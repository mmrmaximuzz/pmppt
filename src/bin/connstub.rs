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

//! Just some test program to implement the trait methods, not a useful executable

use std::{env, fs::File, io::Write, path::PathBuf};

use pmppt::{
    common::{
        Res,
        communication::{Request, Response},
        emsg,
    },
    controller::{
        activity::default_activities::{self},
        connection::{ConnectionOps, tcpmsgpack::TcpMsgpackConnection},
    },
};

fn lookup_paths<C: ConnectionOps>(conn: &mut C, pattern: &str) -> Res<Vec<PathBuf>> {
    conn.send(Request::LookupPaths {
        pattern: pattern.to_string(),
    })
    .map_err(|e| format!("failed to send LookupPaths for {pattern}: {e}"))?;

    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv LookupPaths response for {pattern}: {e}"))?;
    match recv {
        Response::LookupPaths(res) => res,
        _ => unreachable!("bad answer for LookupPaths request {recv:?}"),
    }
}

fn main_wrapper() -> Res<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return emsg(&format!("usage: {} IPADDR:PORT OUTPUT_ARCHIVE", args[0]));
    }
    let endpoint = &args[1];
    let output_path = PathBuf::from(&args[2]);

    let mut conn = TcpMsgpackConnection::from_endpoint(endpoint)?;

    let mpstat = default_activities::launch_mpstat();
    let iostat = default_activities::launch_iostat();
    let netdev = default_activities::proc_net_dev();
    let meminfo = default_activities::proc_meminfo();
    let fio = default_activities::launch_fio(vec![
        String::from("--name=cpuburn"),
        String::from("--ioengine=cpuio"),
        String::from("--cpuload=100"),
        String::from("--time_based=1"),
        String::from("--runtime=5"),
    ]);

    let mut activities = [
        (mpstat, "mpstat", None),
        (iostat, "iostat", None),
        (netdev, "netdev", None),
        (meminfo, "meminfo", None),
        (fio, "fio", None),
    ];

    println!("starting scenario");
    for item in &mut activities {
        let res = item.0.start(&mut conn)?;
        item.2 = res;
    }

    println!("waiting the load to end");

    conn.send(Request::StopAll)
        .map_err(|e| format!("failed to send Stop request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Stop response: {e}"))?;
    match &recv {
        Response::StopAll(Ok(..)) => (),
        Response::StopAll(Err(e)) => return emsg(&format!("failed to stop poll: {e}")),
        _ => unreachable!("bad protocol response for Stop request from agent"),
    };

    println!("Collecting data");
    conn.send(Request::Collect)
        .map_err(|e| format!("failed to send Collect request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Collect response: {e}"))?;
    let data = match recv {
        Response::Collect(Ok(data)) => data,
        Response::Collect(Err(e)) => return emsg(&format!("failed to collect results: {e}")),
        _ => unreachable!("bad protocol response for Collect request from agent"),
    };

    println!("Writing archive");
    File::create(output_path.join("out.tgz"))
        .unwrap()
        .write_all(&data)
        .unwrap();
    drop(data); // explicitly release the memory used for archive

    println!("Writing activity map");
    let mut f = File::create(output_path.join("out.map")).unwrap();
    for (_, name, id) in activities {
        f.write_all(format!("{:03} {name}\n", u32::from(id.unwrap())).as_bytes())
            .unwrap();
    }

    println!("Terminating session");
    conn.send(Request::End)
        .map_err(|e| format!("failed to send End request: {e}"))?;
    conn.close();

    Ok(())
}

fn main() {
    if let Err(msg) = main_wrapper() {
        eprintln!("Error occured while running PMPTT controller stub: {msg}.");
        std::process::exit(1);
    }
}
