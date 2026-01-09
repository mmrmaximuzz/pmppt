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

pub mod poller;
pub mod proto_impl;

use std::ffi::OsStr;
use std::fs::create_dir;
use std::io::Read;
use std::sync::atomic::Ordering;
use std::{
    collections::HashMap,
    fs::File,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    thread::JoinHandle,
};

use log::{error, info, warn};
use subprocess::{Exec, Popen};

use crate::common::Result;
use crate::common::communication::{Id, IdOrError, OutOrError, Request, Response, SpawnMode};

/// Generic transport protocol interface.
pub trait AgentOps {
    fn recv_request(&mut self) -> Option<Request>;
    fn send_response(&mut self, response: Response) -> Option<()>;
}

struct Poll {
    stop: Arc<AtomicBool>,
    thrd: JoinHandle<()>,
    name: String,
}

struct Proc {
    popen: Popen,
    wait4: bool,
    name: String,
}

/// PMPPT Agent instance.
///
/// This structure is generic over [`AgentOps`] trait, allowing different implementation of message
/// transport between agent and controllers. Agent communicates with its controllers and executes
/// performance measurement scenario, keeping all allocated resources inside this structure.
pub struct Agent<P: AgentOps> {
    proto: P,
    count: u32,
    outdir: PathBuf,
    polls: HashMap<Id, Poll>,
    procs: HashMap<Id, Proc>,
}

impl<P> Agent<P>
where
    P: AgentOps,
{
    pub fn new(proto: P, outdir: PathBuf) -> Self {
        Self {
            proto,
            count: 0,
            outdir,
            polls: HashMap::default(),
            procs: HashMap::default(),
        }
    }

    pub fn serve(mut self) {
        info!("agent started");

        let is_abnormal = loop {
            match self.proto.recv_request() {
                None => {
                    error!("failed to get correct message, stop serving agent");
                    break true;
                }
                Some(Request::Abort) => {
                    warn!("got 'abort' request, emergency stop");
                    break true;
                }
                Some(Request::End) => {
                    info!("got 'end' request, stopping running activities");
                    break false;
                }
                Some(msg) => self.handle_message(msg),
            }
        };

        // stop itself before Drop
        self.stop_all(is_abnormal, false);
    }

    fn get_next_id(&mut self) -> Id {
        self.count += 1;
        Id::from(self.count)
    }

    fn spawn_poller(&mut self, paths: &[PathBuf], name: &str) -> IdOrError {
        let id = self.get_next_id();
        let path_out = self.outdir.join(format!("{id:03}-poll.log"));
        let paths = paths.to_owned(); // full clone to send to thread

        let stop_flag_agent = Arc::new(AtomicBool::default());
        let stop_flag_thread = stop_flag_agent.clone();
        let poll_thread =
            std::thread::spawn(move || poller::poll(paths, path_out, stop_flag_thread));

        let res = self.polls.insert(
            id,
            Poll {
                stop: stop_flag_agent,
                thrd: poll_thread,
                name: name.to_owned(),
            },
        );
        assert!(res.is_none(), "got duplicate poll/proc on {id}");

        info!("Poller:   id={id}, path='{name}'");

        // TODO: add checks for failures in poller spawning
        Ok(id)
    }

    fn spawn_process_foreground(&mut self, cmd: String, args: Vec<String>) -> OutOrError {
        let id = self.get_next_id();
        let outpath = self.outdir.join(format!("{id:03}-out.log"));
        let errpath = self.outdir.join(format!("{id:03}-err.log"));
        let cwdpath = self.outdir.join(format!("{id:03}-data"));

        let file_out = File::create_new(&outpath).unwrap();
        let file_err = File::create_new(&errpath).unwrap();
        create_dir(&cwdpath).unwrap();

        let cmd = Exec::cmd(&cmd)
            .args(&args)
            .stdout(file_out)
            .stderr(file_err)
            .cwd(cwdpath);

        // collect the name before spawning the process
        let name = cmd.to_cmdline_lossy();

        info!("FG spawn: id={id}, name='{name}'");

        let status = cmd.join().map_err(|e| {
            let msg = format!("failed to spawn fg process: {e}");
            error!("{msg}");
            msg
        })?;

        info!("FG spawn: id={id}, name='{name}', success={status:?}");

        // collect the results
        let mut stdout = Vec::with_capacity(4096);
        let mut stderr = Vec::with_capacity(4096);
        File::open(outpath)
            .unwrap()
            .read_to_end(&mut stdout)
            .expect("cannot read stdout file");
        File::open(errpath)
            .unwrap()
            .read_to_end(&mut stderr)
            .expect("cannot read stderr file");

        Ok((id, stdout, stderr))
    }

    fn spawn_process_background(
        &mut self,
        cmd: String,
        args: Vec<String>,
        wait4: bool,
    ) -> IdOrError {
        let id = self.get_next_id();
        let outpath = self.outdir.join(format!("{id:03}-out.log"));
        let errpath = self.outdir.join(format!("{id:03}-err.log"));
        let cwdpath = self.outdir.join(format!("{id:03}-data"));

        let file_out = File::create_new(&outpath).unwrap();
        let file_err = File::create_new(&errpath).unwrap();
        create_dir(&cwdpath).unwrap();

        let cmd = Exec::cmd(&cmd)
            .args(&args)
            .stdout(file_out)
            .stderr(file_err)
            .cwd(cwdpath);

        let name = cmd.to_cmdline_lossy();
        let popen = cmd.popen().map_err(|e| {
            let msg = format!("failed to spawn bg process: {e}");
            error!("{msg}");
            msg
        })?;

        let res = self.procs.insert(
            id,
            Proc {
                popen,
                wait4,
                name: name.clone(),
            },
        );
        assert!(res.is_none(), "got duplicate poll/proc on {id}");

        info!("BG spawn: id={id}, name='{name}', wait4={wait4}");

        Ok(id)
    }

    fn spawn_process(&mut self, cmd: String, args: Vec<String>, mode: SpawnMode) -> Response {
        match mode {
            SpawnMode::Foreground => Response::SpawnFg(self.spawn_process_foreground(cmd, args)),
            SpawnMode::BackgroundWait => {
                Response::SpawnBg(self.spawn_process_background(cmd, args, true))
            }
            SpawnMode::BackgroundKill => {
                Response::SpawnBg(self.spawn_process_background(cmd, args, false))
            }
        }
    }

    fn lookup_paths(pattern: &str) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::default();

        // expand braces and interpret each expansion as a glob
        for part in brace_expand::brace_expand(pattern) {
            match glob::glob(&part) {
                Ok(ps) => {
                    let old_len = paths.len();

                    for p in ps {
                        match p {
                            Ok(p) => paths.push(p),
                            Err(e) => return Err(format!("cannot lookup {part}: {e}")),
                        }
                    }

                    // interpret empty glob match for a single brace expansion variant as an error
                    if paths.len() == old_len {
                        return Err(format!("in '{pattern}' got empty search result for {part}"));
                    }
                }
                Err(e) => return Err(format!("bad pattern {pattern}: {e}")),
            }
        }

        if !paths.is_empty() {
            Ok(paths)
        } else {
            Err(format!("got empty search result on expanding '{pattern}'"))
        }
    }

    fn handle_message(&mut self, msg: Request) {
        match msg {
            Request::LookupPaths { pattern } => {
                self.proto
                    .send_response(Response::LookupPaths(Self::lookup_paths(&pattern)));
            }
            Request::Poll { pattern } => {
                let res =
                    Self::lookup_paths(&pattern).and_then(|p| self.spawn_poller(&p, &pattern));
                self.proto.send_response(Response::Poll(res));
            }
            Request::Spawn { cmd, args, mode } => {
                let res = self.spawn_process(cmd, args, mode);
                self.proto.send_response(res);
            }
            Request::Stop { id } => self.stop_task(id),
            Request::StopAll => self.stop_all(false, true),
            Request::Collect => self.collect_data(),
            Request::End => unreachable!("End must be already processed outside"),
            Request::Abort => unreachable!("Abort must be already processed outside"),
        }
    }

    fn stop_all(&mut self, abnormal: bool, from_stopall: bool) {
        let mode = if abnormal { "emergency" } else { "graceful" };
        info!("stopping agent in {mode} mode");

        // stop in reverse order
        for id in (1..=self.count).rev().map(Id::from) {
            match (self.procs.remove(&id), self.polls.remove(&id)) {
                (Some(proc), None) => stop_process(id, proc, abnormal),
                (None, Some(poll)) => stop_poller(id, poll),
                // OK, it was FG process or it has been stopped already by the pmppt controller
                (None, None) => (),
                _ => unreachable!("found both process and poller for id={id}"),
            }
        }

        // sanity checks
        assert!(self.polls.is_empty());
        assert!(self.procs.is_empty());

        if from_stopall {
            self.proto.send_response(Response::StopAll(Ok(())));
        }
    }

    fn stop_task(&mut self, id: Id) {
        match (self.procs.remove(&id), self.polls.remove(&id)) {
            (Some(proc), None) => stop_process(id, proc, false),
            (None, Some(poll)) => stop_poller(id, poll),
            (None, None) => {
                self.proto
                    .send_response(Response::Stop(Err(format!("activity {id} not found"))));
                return;
            }
            _ => unreachable!("found both process and poller for id={id}"),
        }

        self.proto.send_response(Response::Stop(Ok(id)));
    }

    fn collect_data(&mut self) {
        // sanity checks
        assert!(self.polls.is_empty());
        assert!(self.procs.is_empty());

        let res = Exec::cmd("tar")
            .args(&[
                OsStr::new("-c"),
                OsStr::new("-z"),
                OsStr::new("-f"),
                OsStr::new("-"),
                self.outdir.as_os_str(),
            ])
            .capture()
            .map(|d| d.stdout)
            .map_err(|e| format!("failed to collect data: {e}"));

        self.proto.send_response(Response::Collect(res));
    }
}

fn stop_poller(id: Id, poll: Poll) {
    info!("stopping poller  id={id}, name='{}'", poll.name);
    poll.stop.store(true, Ordering::Release);
    poll.thrd
        .join()
        .unwrap_or_else(|_| panic!("cannot join polling thread: {id}"));
}

fn stop_process(id: Id, mut proc: Proc, force: bool) {
    info!("stopping process id={id}, name='{}'", proc.name);
    if !proc.wait4 || force {
        // send the signal to terminate it now
        proc.popen
            .terminate()
            .unwrap_or_else(|_| panic!("failed to terminate process {id}"));
    }

    proc.popen
        .wait()
        .unwrap_or_else(|_| panic!("failed to wait for the process {id}"));
}
