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
        activity::{ActivityConfig, ActivityCreator, default_activities},
        connection::ConnectionOps,
        storage::Storage,
    },
};

const BW_FILE_NAME: &str = "bw";
const IOPS_FILE_NAME: &str = "iops";
const LAT_FILE_NAME: &str = "lat";

const DEVICE_ARTIFACT_NAME: &str = "LOOP_DEVICES";

fn create_connection(endpoint: &str) -> Res<impl ConnectionOps> {
    use pmppt::controller::connection::tcpmsgpack::TcpMsgpackConnection;
    TcpMsgpackConnection::from_endpoint(endpoint)
}

fn main_wrapper() -> Res<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return emsg(&format!("usage: {} IPADDR:PORT OUTPUT_ARCHIVE", args[0]));
    }
    let endpoint = &args[1];
    let output_path = PathBuf::from(&args[2]);

    let mut conn = create_connection(endpoint)?;
    let mut stor = Storage::default();

    let mpstat: &ActivityCreator = &default_activities::mpstat_creator;
    let netdev: &ActivityCreator = &default_activities::proc_net_dev_creator;
    let meminf: &ActivityCreator = &default_activities::proc_meminfo_creator;
    let fgraph: &ActivityCreator = &default_activities::flamegraph_creator;
    let lookup: &ActivityCreator = &default_activities::lookup_creator;
    let iostat: &ActivityCreator = &default_activities::iostat_creator;
    let sleeper: &ActivityCreator = &default_activities::sleeper_creator;

    let mpstat = mpstat(ActivityConfig::new())?;
    let netdev = netdev(ActivityConfig::new())?;
    let meminf = meminf(ActivityConfig::new())?;
    let fgraph = fgraph(ActivityConfig::new())?;
    let sleeper = sleeper(ActivityConfig::with_time(Duration::from_secs(2)))?;
    let lookup_loopdevs =
        lookup(ActivityConfig::with_str("/dev/loop0").artifact_out("paths", DEVICE_ARTIFACT_NAME))?;

    let iostat = iostat(ActivityConfig::new().artifact_in("devices", DEVICE_ARTIFACT_NAME))?;

    let fio = default_activities::launch_fio(vec![
        String::from("--name=iouring-large-write-verify-loopdev-over-tmpfs"),
        String::from("--ioengine=sync"),
        String::from("--iodepth=1"),
        String::from("--direct=1"),
        String::from("--filename=/dev/loop0"),
        String::from("--rw=readwrite"),
        String::from("--blocksize=4K"),
        String::from("--loops=1"),
        String::from("--rate_iops=750"),
        format!("--write_bw_log={BW_FILE_NAME}"),
        format!("--write_iops_log={IOPS_FILE_NAME}"),
        format!("--write_lat_log={LAT_FILE_NAME}"),
        String::from("--log_avg_msec=500"),
        String::from("--numjobs=2"),
    ]);

    let mut activities = [
        (lookup_loopdevs, "loopdevs", None, None),
        (mpstat, "mpstat", None, None),
        (iostat, "iostat", None, None),
        (netdev, "netdev", None, None),
        (meminf, "meminfo", None, None),
        (sleeper, "sleeper", None, None),
        (
            fio,
            "fio",
            None,
            Some(format!("{BW_FILE_NAME}:{IOPS_FILE_NAME}:{LAT_FILE_NAME}")),
        ),
        (fgraph, "flamegraph", None, None),
    ];

    println!("starting scenario");
    for item in &mut activities {
        let res = item.0.start(&mut conn, &mut stor)?;
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

        f.write_all(format!("{id:03} {name} {}\n", args.unwrap_or("".to_string())).as_bytes())
            .unwrap();
    }

    println!("Terminating session");
    conn.send(Request::End)
        .map_err(|e| format!("failed to send End request: {e}"))?;
    conn.close();

    dbg!(stor);

    Ok(())
}

fn main() {
    if let Err(msg) = main_wrapper() {
        eprintln!("Error occured while running PMPTT controller stub: {msg}.");
        std::process::exit(1);
    }
}
