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

use std::{fs::File, io::Read, path::Path};

use plotly::{
    Layout, Plot, Scatter,
    layout::{Axis, GridPattern, LayoutGrid},
};

fn parse_bw_log(content: &str) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut read_time = vec![];
    let mut read_bw = vec![];
    let mut write_time = vec![];
    let mut write_bw = vec![];
    for line in content.lines() {
        let items: Vec<_> = line
            .split(", ")
            .map(|i| i.parse::<u64>().unwrap())
            .collect();
        let time = items[0] as f64 / 1000.0; // ms -> s
        let bw = items[1] as f64 / 1024.0; // KiB/s -> MiB/s
        let ddir = items[2];

        match ddir {
            0 => {
                read_time.push(time);
                read_bw.push(bw);
            }
            1 => {
                write_time.push(time);
                write_bw.push(bw);
            }
            otherwise => unreachable!("bad data dir: {otherwise}"),
        }
    }

    (read_time, read_bw, write_time, write_bw)
}

fn compose_graph(
    jobname: &str,
    read_time: Vec<f64>,
    read_bw: Vec<f64>,
    write_time: Vec<f64>,
    write_bw: Vec<f64>,
) -> Plot {
    let mut plot = Plot::new();

    plot.add_trace(
        Scatter::new(read_time, read_bw)
            .name("read")
            .x_axis("x")
            .y_axis("y"),
    );
    plot.add_trace(
        Scatter::new(write_time, write_bw)
            .name("write")
            .x_axis("x2")
            .y_axis("y2"),
    );

    plot.set_layout(
        Layout::new()
            .grid(
                LayoutGrid::new()
                    .rows(1)
                    .columns(2)
                    .pattern(GridPattern::Independent),
            )
            .title(jobname)
            .y_axis(Axis::new().title("Read bandwidth [MiB/s]"))
            .y_axis2(Axis::new().title("Write bandwidth [MiB/s]"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );
    plot
}

pub fn process(_content: &str, datadir: &Path, options: &str) -> Vec<(String, Plot)> {
    let bw_prefix = options;

    // filter bw_log files
    let mut bwfiles = vec![];
    for path in datadir.read_dir().unwrap().flatten().map(|e| e.path()) {
        let name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        if name.starts_with(bw_prefix) {
            bwfiles.push((name, path));
        }
    }

    let mut graphs = vec![];

    // process bw_log files
    for (name, path) in bwfiles {
        let mut buf = String::with_capacity(32 * 1024);
        File::open(&path).unwrap().read_to_string(&mut buf).unwrap();
        let (read_time, read_bw, write_time, write_bw) = parse_bw_log(&buf);
        let g = compose_graph(&name, read_time, read_bw, write_time, write_bw);
        graphs.push((name, g));
    }

    graphs
}
