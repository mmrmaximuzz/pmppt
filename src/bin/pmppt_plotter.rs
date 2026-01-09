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

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use plotly::layout::{Axis, GridPattern, LayoutGrid};
use plotly::{self, HeatMap, Layout, Plot, Scatter};
use serde::Serialize;
use subprocess::Exec;
use tempdir::TempDir;

use pmppt::common::{Result, emsg};
use pmppt::plotters::procfs::{Meminfo, NetDev};
use pmppt::plotters::sysstat::iostat::Iostat;
use pmppt::plotters::sysstat::mpstat::Mpstat;
use pmppt::plotters::{fio, procfs, sysstat};

// newtype to support Serialize trait for NaiveDateTime
#[derive(Clone)]
struct MyDateTime(NaiveDateTime);

impl Serialize for MyDateTime {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

fn plot_heatmaps(mpstat: Mpstat) -> Plot {
    let time: Vec<_> = mpstat.time.iter().map(|d| MyDateTime(*d)).collect();
    let cpus: Vec<_> = (0..mpstat.nr_cpus).collect();

    let maps = vec![
        mpstat.busy,
        mpstat.usr,
        mpstat.sys,
        mpstat.irq,
        mpstat.soft,
        mpstat.iowait,
    ];
    let names = vec!["busy", "usr", "sys", "irq", "soft", "iowait"];
    let xaxis = vec!["x", "x3", "x5", "x2", "x4", "x6"];
    let yaxis = vec!["y", "y3", "y5", "y2", "y4", "y6"];

    let mut plot = Plot::new();
    for (((map, name), x), y) in maps.into_iter().zip(names).zip(xaxis).zip(yaxis) {
        plot.add_trace(
            HeatMap::new(time.clone(), cpus.clone(), map)
                .x_axis(x)
                .y_axis(y)
                .name(name)
                .show_scale(false)
                .transpose(true),
        );
    }

    plot.set_layout(
        Layout::new()
            .grid(
                LayoutGrid::new()
                    .rows(3)
                    .columns(2)
                    .pattern(GridPattern::Independent),
            )
            .title("mpstat CPU loads")
            .y_axis(Axis::new().title("busy"))
            .y_axis3(Axis::new().title("usr"))
            .y_axis5(Axis::new().title("sys"))
            .y_axis2(Axis::new().title("hirq"))
            .y_axis4(Axis::new().title("sirq"))
            .y_axis6(Axis::new().title("iowait"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );

    plot
}

fn plot_meminfo(meminfo: Meminfo) -> Plot {
    let mut plot = Plot::new();
    for (item, data) in meminfo.items {
        plot.add_trace(Scatter::new(meminfo.time.clone(), data).name(item));
    }
    plot.set_layout(
        Layout::new()
            .title("/proc/meminfo data")
            .x_axis(Axis::new().title("Time"))
            .y_axis(Axis::new().title("Memory [GiB]"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );
    plot
}

fn plot_net_dev(net_dev: NetDev) -> Plot {
    let mut plot = Plot::new();

    if net_dev.bytes_stat.is_empty() {
        // if no bytes in bytes stat, nothing to show at all
        return plot;
    }

    // draw bytes statistic in the first chart
    for (item, data) in net_dev.bytes_stat {
        plot.add_trace(
            Scatter::new(net_dev.time.clone(), data)
                .name(item)
                .x_axis("x")
                .y_axis("y"),
        );
    }

    let mut max_plot_count = 1; // assume there is always bytes chart if any

    // draw the rest of data in the other grid positions
    let stype = [
        "packets",
        "errs",
        "drop",
        "fifo",
        "frame",
        "compressed",
        "multicast",
    ];
    let xaxis = ["x2", "x3", "x4", "x5", "x6", "x7", "x8"];
    let yaxis = ["y2", "y3", "y4", "y5", "y6", "y7", "y8"];
    for (i, ((s, x), y)) in stype.into_iter().zip(xaxis).zip(yaxis).enumerate() {
        for (item, data) in net_dev.count_stat.iter() {
            if !item.ends_with(s) {
                continue;
            }

            plot.add_trace(
                Scatter::new(net_dev.time.clone(), data.to_vec())
                    .name(item)
                    .x_axis(x)
                    .y_axis(y),
            );
            max_plot_count = i + 2;
        }
    }

    plot.set_layout(
        Layout::new()
            .grid(
                LayoutGrid::new()
                    .rows(max_plot_count.div_ceil(2))
                    .columns(2)
                    .pattern(GridPattern::Independent),
            )
            .title("/proc/net_dev data")
            .y_axis(Axis::new().title("Data rate [Mbps]"))
            .y_axis2(Axis::new().title("Packet rate [kpps]"))
            .y_axis3(Axis::new().title("Error rate [kerr/s]"))
            .y_axis4(Axis::new().title("Drop rate [kdrop/s]"))
            .y_axis5(Axis::new().title("Fifo rate [kevent/s]"))
            .y_axis6(Axis::new().title("Frame rate [kevent/s]"))
            .y_axis7(Axis::new().title("Compressed rate [kevent/s]"))
            .y_axis8(Axis::new().title("Multicast rate [kevent/s]"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );
    plot
}

fn plot_iostat(iostat: Iostat) -> Plot {
    let mut plot = Plot::new();
    let params = [
        ("riops", "x", "y"),
        ("wiops", "x2", "y2"),
        ("rMBs", "x3", "y3"),
        ("wMBs", "x4", "y4"),
        ("rsize", "x5", "y5"),
        ("wsize", "x6", "y6"),
        ("qlen", "x7", "y7"),
        ("util", "x8", "y8"),
    ];

    let mut disks: Vec<_> = iostat.disks.iter().collect();
    disks.sort();

    for (suffix, x, y) in params {
        for disk in &disks {
            let label = format!("{disk}_{suffix}");
            let values = iostat.stats.get(&label).unwrap(); // must be present
            plot.add_trace(
                Scatter::new(iostat.times.clone(), values.clone())
                    .name(label)
                    .x_axis(x)
                    .y_axis(y),
            );
        }
    }

    plot.set_layout(
        Layout::new()
            .grid(
                LayoutGrid::new()
                    .rows(4)
                    .columns(2)
                    .pattern(GridPattern::Independent),
            )
            .title("iostat data")
            .y_axis(Axis::new().title("Read rate [IOPS]"))
            .y_axis2(Axis::new().title("Write rate [IOPS]"))
            .y_axis3(Axis::new().title("Read speed [MB/s]"))
            .y_axis4(Axis::new().title("Write speed [MB/s]"))
            .y_axis5(Axis::new().title("Read avg size [KiB]"))
            .y_axis6(Axis::new().title("Write avg size [KiB]"))
            .y_axis7(Axis::new().title("Queue length"))
            .y_axis8(Axis::new().title("Util [%]"))
            .width(1900)
            .height(950)
            .auto_size(true),
    );

    plot
}

fn readfile(path: &Path) -> Result<String> {
    use std::io::Read;

    let mut buf = String::with_capacity(32 * 1024);
    File::open(path)
        .unwrap_or_else(|_| panic!("failed to open file {path:?}"))
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to open {path:?}: {e}"))?;
    Ok(buf)
}

type PlotInfo = (String, String, String, Option<String>);

fn read_mapping(path: &Path) -> Result<Vec<PlotInfo>> {
    let content = readfile(path)?;

    let mut res = vec![];
    for item in content.lines() {
        let parts: Vec<&str> = item.split_whitespace().collect();
        let num = parts[0];
        let name = parts[1];
        let param = parts.get(2).map(|s| s.to_string());
        let datasuffix = match name {
            "mpstat" => "out.log",
            "iostat" => "out.log",
            "netdev" => "poll.log",
            "meminfo" => "poll.log",
            "fio" => "out.log",
            "flamegraph" => "out.log",
            _ => continue,
        };
        res.push((
            name.to_string(),
            format!("{num}-{datasuffix}"),
            format!("{num}-data"),
            param,
        ));
    }
    Ok(res)
}

fn process_dir(outdir: PathBuf) -> Result<()> {
    if !outdir.is_dir() {
        return Err(format!("{outdir:?} is not a directory"));
    }

    let plotdir = TempDir::new_in(&outdir, "plotter")
        .map_err(|e| format!("failed to create tempdir for plotting: {e}"))?;

    // unpack the data array
    Exec::cmd("tar")
        .args(&[
            OsStr::new("-xf"),
            outdir.join("out.tgz").as_os_str(),
            OsStr::new("-C"),
            plotdir.path().as_os_str(),
            OsStr::new("--strip-components=2"),
        ])
        .join()
        .map_err(|e| format!("failed to unpack the data array: {e}"))?;

    // process the data
    for (name, filestr, datadir, options) in read_mapping(&outdir.join("out.map"))? {
        let content = readfile(&plotdir.path().join(filestr))?;
        let outfile = outdir.join(format!("{name}.html"));
        match name.as_str() {
            "mpstat" => plot_heatmaps(sysstat::mpstat::parse(&content)?).write_html(outfile),
            "iostat" => plot_iostat(sysstat::iostat::parse(&content)?).write_html(outfile),
            "netdev" => plot_net_dev(procfs::parse_net_dev(&content)?).write_html(outfile),
            "meminfo" => plot_meminfo(procfs::parse_meminfo(&content)?).write_html(outfile),
            "fio" => {
                if let Some(opts) = options {
                    fio::process(&content, &plotdir.path().join(datadir), &opts)
                        .write_html(outfile);
                }
            }
            "flamegraph" => {
                if let Some(svgpath) = options {
                    std::fs::rename(
                        plotdir.path().join(datadir).join(&svgpath),
                        outdir.join(&svgpath),
                    )
                    .unwrap();
                }
            }
            _ => unreachable!("{name}"),
        };
    }

    Ok(())
}

fn main_wrapper(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return emsg("usage: PROG PATH_TO_OUTPUT");
    }

    let outdir = PathBuf::from(&args[1]);
    process_dir(outdir)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(msg) = main_wrapper(&args) {
        eprintln!("Error: {msg}");
        std::process::exit(1);
    }
}
