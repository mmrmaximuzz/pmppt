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

use std::{collections::HashMap, env, fs::File, io::Write, path::PathBuf, time::Duration};

use pmppt::{
    common::{
        Result,
        communication::{Request, Response},
    },
    controller::{
        activity::{Activity, ActivityConfig, ActivityCreator, ActivityCreatorFn},
        connection::ConnectionOps,
        storage::Storage,
    },
    types::IniLike,
};

fn create_connection(endpoint: &str) -> Result<impl ConnectionOps> {
    use pmppt::controller::connection::tcpmsgpack::TcpMsgpackConnection;
    TcpMsgpackConnection::from_endpoint(endpoint)
}

type ActivityDatabase = HashMap<String, ActivityCreator>;

fn collect_activity_database() -> ActivityDatabase {
    use pmppt::controller::activity::default_activities::*;

    let creators_info: &[(&str, ActivityCreatorFn)] = &[
        ("sleep", sleeper_creator),
        ("mpstat", mpstat_creator),
        ("iostat", iostat_creator),
        ("netdev", proc_net_dev_creator),
        ("meminfo", proc_meminfo_creator),
        ("lookup_paths", lookup_creator),
        ("flamegraph", flamegraph_creator),
        ("fio", fio_creator),
    ];

    let mut db: ActivityDatabase = HashMap::new();
    for (name, func) in creators_info {
        db.insert(name.to_string(), Box::new(func));
    }
    db
}

fn configure_pipeline(db: &ActivityDatabase) -> Result<Vec<(String, Box<dyn Activity>)>> {
    let pipeline_info = [
        ("mpstat", ActivityConfig::new()),
        ("netdev", ActivityConfig::new()),
        ("meminfo", ActivityConfig::new()),
        (
            "lookup_paths",
            ActivityConfig::with_str("/dev/loop0").artifact_out("paths", "LOOP_DEVS"),
        ),
        (
            "iostat",
            ActivityConfig::new().artifact_in("devices", "LOOP_DEVS"),
        ),
        ("sleep", ActivityConfig::with_time(Duration::from_secs(2))),
        (
            "fio",
            ActivityConfig::with_ini(
                IniLike::with_global(&[
                    "ioengine=sync",
                    "direct=1",
                    "rate_iops=750",
                    "filename=/dev/loop0",
                    "loops=1",
                    "log_avg_msec=500",
                ])
                .section(
                    "reader",
                    &[
                        "rw=read",
                        "write_bw_log=read-bw",
                        "write_iops_log=read-iops",
                        "write_lat_log=read-lat",
                    ],
                )
                .section(
                    "writer",
                    &[
                        "rw=write",
                        "write_bw_log=write-bw",
                        "write_iops_log=write-iops",
                        "write_lat_log=write-lat",
                    ],
                ),
            ),
        ),
        ("flamegraph", ActivityConfig::new()),
    ];

    let mut pipeline = vec![];
    for (name, conf) in pipeline_info.into_iter() {
        let factory = db.get(name).ok_or(format!(
            "failed to find ActivityCreator for activity '{name}'"
        ))?;
        let activity = factory(conf).map_err(|e| format!("failed to construct '{name}': {e}"))?;
        pipeline.push((name.to_string(), activity));
    }
    Ok(pipeline)
}

fn main_wrapper() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        return Err(format!("usage: {} IPADDR:PORT OUTPUT_ARCHIVE", args[0]));
    }
    let endpoint = &args[1];
    let output_path = PathBuf::from(&args[2]);

    println!("Collecting activities");
    let creators = collect_activity_database();

    println!("Configuring pipeline");
    let mut pipeline = configure_pipeline(&creators)?;

    println!("Connecting to agent");
    let mut conn = create_connection(endpoint)?;
    let stor = Storage::default();

    println!("Running pipeline");
    for (name, activity) in &mut pipeline {
        println!("starting '{name}'");
        activity
            .start(&mut conn, &stor)
            .map_err(|e| format!("err ({name}): {e}"))?;
    }

    println!("Waiting the load to end");
    let mut postprocessing = vec![];
    pipeline.reverse();
    for (name, activity) in &mut pipeline {
        println!("stopping {name}");
        if let Some(id) = activity
            .stop(&mut conn, &stor)
            .map_err(|e| format!("error while stopping '{name}': {e}"))?
        {
            postprocessing.push((name, id));
        }
    }
    postprocessing.reverse();

    conn.send(Request::StopAll)
        .map_err(|e| format!("failed to send Stop request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Stop response: {e}"))?;
    match &recv {
        Response::StopAll(Ok(..)) => (),
        Response::StopAll(Err(e)) => return Err(format!("failed to stop poll: {e}")),
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
        Response::Collect(Err(e)) => return Err(format!("failed to collect results: {e}")),
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
    for (name, (id, hint)) in postprocessing {
        let hint = hint.unwrap_or(String::new());
        f.write_all(format!("{id:03} {name} {hint}\n").as_bytes())
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
