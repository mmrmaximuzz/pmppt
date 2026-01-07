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

fn parse_fio_log(
    content: &str,
    f: impl Fn(u64) -> f64,
) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut rd_time = vec![];
    let mut rd_vals = vec![];
    let mut wr_time = vec![];
    let mut wr_vals = vec![];
    for line in content.lines() {
        let items: Vec<_> = line
            .split(", ")
            .map(|i| i.parse::<u64>().unwrap())
            .collect();
        let time = items[0] as f64 / 1000.0; // msec -> sec
        let value = f(items[1]);
        let ddir = items[2];

        match ddir {
            0 => {
                rd_time.push(time);
                rd_vals.push(value);
            }
            1 => {
                wr_time.push(time);
                wr_vals.push(value);
            }
            otherwise => unreachable!("bad data dir: {otherwise}"),
        }
    }

    (rd_time, rd_vals, wr_time, wr_vals)
}

fn match_prefix(prefixes: &[String], s: &str) -> bool {
    for prefix in prefixes {
        if s.starts_with(prefix) {
            return true;
        }
    }
    false
}

pub fn process(_content: &str, datadir: &Path, options: &str) -> Plot {
    let options: Vec<&str> = options.split(",").collect();
    assert_eq!(options.len(), 3);

    let bw_prefixes: Vec<_> = options[0]
        .split(":")
        .map(|s| format!("{}_bw.", s))
        .collect();
    let iops_prefixes: Vec<_> = options[1]
        .split(":")
        .map(|s| format!("{}_iops.", s))
        .collect();

    let lat_prefixes: Vec<_> = options[2] // use only total latency values
        .split(":")
        .map(|s| format!("{}_lat.", s))
        .collect();

    // filter files by type
    let mut bwfiles = vec![];
    let mut iopsfiles = vec![];
    let mut latfiles = vec![];
    for path in datadir.read_dir().unwrap().flatten().map(|e| e.path()) {
        let name = match path.file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => continue,
        };

        if match_prefix(&bw_prefixes, &name) {
            bwfiles.push((name, path));
            continue;
        }

        if match_prefix(&iops_prefixes, &name) {
            iopsfiles.push((name, path));
            continue;
        }

        if match_prefix(&lat_prefixes, &name) {
            latfiles.push((name, path));
            continue;
        }
    }

    let ftypes: &[(_, &dyn Fn(u64) -> f64, _)] = &[
        (bwfiles, &|x| x as f64 / 1024.0, (("x", "y"), ("x2", "y2"))),
        (
            iopsfiles,
            &|x| x as f64 / 1000.0,
            (("x3", "y3"), ("x4", "y4")),
        ),
        (latfiles, &|x| x as f64 / 1e6, (("x5", "y5"), ("x6", "y6"))),
    ];

    let mut plot = Plot::new();
    for (files, f, ((xr, yr), (xw, yw))) in ftypes {
        for (name, path) in files {
            let mut buf = String::with_capacity(32 * 1024);
            File::open(path).unwrap().read_to_string(&mut buf).unwrap();
            let (read_time, read_vals, write_time, write_vals) = parse_fio_log(&buf, f);
            plot.add_trace(
                Scatter::new(read_time, read_vals)
                    .name(format!("{name}-read"))
                    .x_axis(xr)
                    .y_axis(yr),
            );
            plot.add_trace(
                Scatter::new(write_time, write_vals)
                    .name(format!("{name}-write"))
                    .x_axis(xw)
                    .y_axis(yw),
            );
        }
    }

    plot.set_layout(
        Layout::new()
            .grid(
                LayoutGrid::new()
                    .rows(3)
                    .columns(2)
                    .pattern(GridPattern::Independent),
            )
            .title("FIO job results")
            .y_axis(Axis::new().title("Read bandwidth [MiB/s]"))
            .y_axis2(Axis::new().title("Write bandwidth [MiB/s]"))
            .y_axis3(Axis::new().title("Read IOPS [kIO/s]"))
            .y_axis4(Axis::new().title("Write IOPS [kIO/s]"))
            .y_axis5(Axis::new().title("Read avg latency [ms]"))
            .y_axis6(Axis::new().title("Write avg latency [ms]"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );

    plot
}
