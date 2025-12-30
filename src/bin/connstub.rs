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

use std::{env, fs::File, io::Write, path::PathBuf, time::Duration};

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

const BW_FILE_NAME: &str = "bw";
const LHIST_FILE_NAME: &str = "custom_name2";

fn main_wrapper() -> Res<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return emsg(&format!("usage: {} IPADDR:PORT OUTPUT_ARCHIVE", args[0]));
    }
    let endpoint = &args[1];
    let output_path = PathBuf::from(&args[2]);

    let mut conn = TcpMsgpackConnection::from_endpoint(endpoint)?;

    // first get loop devs
    let loopdevs = lookup_paths(&mut conn, "/dev/loop0")?;

    let mpstat = default_activities::launch_mpstat();
    let iostat = default_activities::launch_iostat_on(&loopdevs);
    let netdev = default_activities::proc_net_dev();
    let meminfo = default_activities::proc_meminfo();

    // add some sleep to get some point before the test for the reference
    let sleeper = default_activities::get_sleeper(Duration::from_secs(2));

    let fio = default_activities::launch_fio(vec![
        String::from("--name=iouring-large-write-verify-loopdev-over-tmpfs"),
        String::from("--ioengine=sync"),
        String::from("--iodepth=1"),
        String::from("--direct=1"),
        String::from("--filename=/dev/loop0"),
        String::from("--rw=readwrite"),
        String::from("--blocksize=4K"),
        String::from("--loops=50"),
        format!("--write_bw_log={BW_FILE_NAME}"),
        format!("--write_hist_log={LHIST_FILE_NAME}"),
        String::from("--log_avg_msec=1000"),
        String::from("--log_hist_msec=1000"),
    ]);

    let mut activities = [
        (mpstat, "mpstat", None, None),
        (iostat, "iostat", None, None),
        (netdev, "netdev", None, None),
        (meminfo, "meminfo", None, None),
        (sleeper, "sleeper", None, None),
        (fio, "fio", None, Some(BW_FILE_NAME)),
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
    for (_, name, id, args) in activities {
        let id = match id {
            Some(id) => u32::from(id),
            None => continue,
        };

        f.write_all(format!("{id:03} {name} {}\n", args.unwrap_or("")).as_bytes())
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
