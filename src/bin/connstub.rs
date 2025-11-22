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

use std::{
    env,
    fs::File,
    io::{Read, Write},
};

use pmppt::{
    common::{
        Res,
        communication::{Id, Request, Response},
        emsg,
    },
    controller::connection::{ConnectionOps, tcpmsgpack},
};

fn poll<C: ConnectionOps>(conn: &mut C, pattern: &str) -> Res<Id> {
    conn.send(Request::Poll {
        pattern: pattern.to_string(),
    })
    .map_err(|e| format!("failed to send Poll for {pattern}: {e}"))?;

    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Poll response for {pattern}: {e}"))?;
    match recv {
        Response::Poll(Ok(id)) => Ok(id),
        Response::Poll(Err(e)) => Err(format!("failed to launch Poll for {pattern}: {e}")),
        _ => unreachable!("bad answer for Poll request {recv:?}"),
    }
}

fn main_wrapper() -> Res<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return emsg(&format!("usage: {} IPADDR:PORT", args[0]));
    }

    let endpoint = &args[1];
    let mut conn = tcpmsgpack::TcpMsgpackConnection::from_endpoint(endpoint)?;

    println!("Starting the pollers");
    poll(&mut conn, "/proc/stat")?;
    poll(&mut conn, "/proc/meminfo")?;
    poll(&mut conn, "/proc/net/dev")?;
    poll(&mut conn, "/proc/diskstats")?;

    println!("Press any key to stop collection");
    std::io::stdin()
        .read_exact(&mut [0u8])
        .expect("stdin is broken");

    println!("Stopping collection");
    conn.send(Request::StopAll)
        .map_err(|e| format!("failed to send Stop request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Stop response: {e}"))?;
    match &recv {
        Response::Stop(Ok(..)) => (),
        Response::Stop(Err(e)) => return emsg(&format!("failed to stop poll: {e}")),
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
    File::create_new("archive.tgz")
        .unwrap()
        .write_all(&data)
        .unwrap();
    drop(data);

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
