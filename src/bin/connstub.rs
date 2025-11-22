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

use std::{env, fs::File, io::Write, thread::sleep, time::Duration};

use pmppt::{
    common::{
        Res,
        communication::{Request, Response},
        emsg,
    },
    controller::connection::{ConnectionOps, tcpmsgpack},
};

fn main_wrapper() -> Res<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        return emsg(&format!("usage: {} IPADDR:PORT", args[0]));
    }

    let endpoint = &args[1];
    let mut conn = tcpmsgpack::TcpMsgpackConnection::from_endpoint(endpoint)?;

    conn.send(Request::Poll {
        pattern: "/proc/diskstats".to_string(),
    })
    .map_err(|e| format!("failed to send poll request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv poll response: {e}"))?;
    let id = match recv {
        Response::Poll(Ok(id)) => id,
        Response::Poll(Err(e)) => return emsg(&format!("failed to spawn poll: {e}")),
        _ => unreachable!("bad protocol response for Poll request from agent"),
    };

    sleep(Duration::from_secs(3));

    conn.send(Request::Stop { id })
        .map_err(|e| format!("failed to send Stop request: {e}"))?;
    let recv = conn
        .recv()
        .map_err(|e| format!("failed to recv Stop response: {e}"))?;
    match &recv {
        Response::Stop(Ok(..)) => (),
        Response::Stop(Err(e)) => return emsg(&format!("failed to stop poll: {e}")),
        _ => unreachable!("bad protocol response for Stop request from agent"),
    };

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

    File::create_new("archive.tgz")
        .unwrap()
        .write_all(&data)
        .unwrap();
    drop(data);

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
