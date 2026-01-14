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

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset};

use crate::common::Result;

pub struct Meminfo {
    pub time: Vec<String>,
    pub items: HashMap<String, Vec<f64>>,
}

fn cut_poller_header(content: &str) -> Result<&str> {
    let (_, data) = content.split_once("\n").ok_or("failed to cut header")?;
    Ok(data.trim_ascii())
}

fn to_chunks(content: &str) -> Vec<&str> {
    content.split("\n\n").collect()
}

fn guess_coeff(value: &str) -> Option<f64> {
    match value {
        "kB" => Some(1024.0),
        _ => None,
    }
}

fn remove_nonchanging_data<T: PartialEq>(map: &mut HashMap<String, Vec<T>>) {
    let mut to_remove = vec![];
    for (name, values) in map.iter() {
        let startval = &values[0];

        let mut all_the_same = true;
        for x in values {
            if *x != *startval {
                all_the_same = false;
                break;
            }
        }
        if all_the_same {
            to_remove.push(name.clone());
        }
    }

    for name in to_remove {
        map.remove(&name).unwrap();
    }
}

fn handle_chunk(
    chunk: &str,
) -> Result<(String, DateTime<FixedOffset>, impl Iterator<Item = &str>)> {
    let (timeline, data) = chunk
        .split_once("\n")
        .ok_or_else(|| format!("bad chunk: {chunk}"))?;

    let tstamp = DateTime::parse_from_rfc3339(timeline)
        .map_err(|e| format!("bad time '{timeline}': {e}"))?;

    Ok((timeline.to_string(), tstamp, data.split("\n")))
}

fn process_meminfo_chunks(chunks: &[&str]) -> Result<Meminfo> {
    let mut time = vec![];
    let mut map: HashMap<String, Vec<f64>> = HashMap::default();

    for chunk in chunks {
        // just verify the time format, we dont need the tstamp value itself
        let (timeline, _, items) = handle_chunk(chunk)?;

        time.push(timeline.to_string());
        for item in items {
            let (name, valueline) = item
                .split_once(":")
                .ok_or_else(|| format!("failed to split by colon: {item}"))?;
            let values: Vec<_> = valueline.trim_ascii().split_ascii_whitespace().collect();
            if values.len() != 2 {
                // this item has no multiplier, ignore it as it is usually not useful
                continue;
            }

            let value = values[0]
                .parse::<f64>()
                .map_err(|e| format!("bad value '{item}': {e}"))?;
            let coeff = guess_coeff(values[1]).ok_or_else(|| format!("bad coeff in {item}"))?;

            let value = value * coeff / 1073741824.0; // measure all in GiBs

            match map.get_mut(name) {
                Some(v) => v.push(value),
                None => {
                    map.insert(name.to_string(), vec![value]);
                }
            }
        }
    }

    remove_nonchanging_data(&mut map);
    Ok(Meminfo { time, items: map })
}

pub fn parse_meminfo(content: &str) -> Result<Meminfo> {
    let data = cut_poller_header(content)?;
    let chunks = to_chunks(data);
    process_meminfo_chunks(&chunks)
}

pub struct NetDev {
    pub time: Vec<String>,
    pub bytes_stat: HashMap<String, Vec<f64>>,
    pub count_stat: HashMap<String, Vec<f64>>,
}

fn get_diff(old: &mut Option<u64>, newval: u64, dt: f64) -> f64 {
    match old.replace(newval) {
        Some(oldval) => (newval - oldval) as f64 / dt,
        None => 0.0,
    }
}

fn process_net_dev_chunks(chunks: &[&str]) -> Result<NetDev> {
    let mut time = vec![];
    let mut bytes_stat: HashMap<String, Vec<f64>> = HashMap::default();
    let mut count_stat: HashMap<String, Vec<f64>> = HashMap::default();

    let mut last_tstamp = None;
    let mut last_stats: HashMap<String, [Option<u64>; 16]> = HashMap::default();

    for chunk in chunks {
        let (timeline, tstamp, items) = handle_chunk(chunk)?;
        time.push(timeline.to_string());

        let dt = if let Some(oldtime) = last_tstamp.replace(tstamp) {
            (tstamp - oldtime).as_seconds_f64()
        } else {
            1.0 // the actual value does not matter, just not zero
        };

        // ignore 2-line text header of /proc/net/dev
        for item in items.skip(2) {
            let (ifname, valueline) = item
                .split_once(":")
                .ok_or_else(|| format!("failed to split by colon: {item}"))?;

            let values: Vec<_> = valueline
                .trim_ascii()
                .split_ascii_whitespace()
                .map(|s| s.parse::<u64>().unwrap())
                .collect();
            if values.len() != 16 {
                return Err(format!("bad value vector for /proc/net/dev {item}"));
            }

            // extract last stats for this interface
            let last_if_stats = match last_stats.get_mut(ifname) {
                Some(stats) => stats,
                None => {
                    last_stats.insert(ifname.to_string(), [None; 16]);
                    last_stats.get_mut(ifname).unwrap()
                }
            };

            for (dir, range) in [("rx", 0..8), ("tx", 8..16)] {
                let label = format!("{ifname}_{dir}_bytes");

                let items = &values[range.clone()];
                let last_stats = &mut last_if_stats[range.clone()];

                // collect bytes stat separately
                let bandwidth_mbps = get_diff(&mut last_stats[0], items[0], dt) * 8.0 / 1e6;
                match bytes_stat.get_mut(&label) {
                    Some(v) => v.push(bandwidth_mbps),
                    None => {
                        bytes_stat.insert(label, vec![bandwidth_mbps]);
                    }
                }

                // collect the rest counts
                for ((value, old_stat), valname) in
                    items[1..].iter().zip(&mut last_stats[1..]).zip([
                        "packets",
                        "errs",
                        "drop",
                        "fifo",
                        "frame",
                        "compressed",
                        "multicast",
                    ])
                {
                    let label = format!("{ifname}_{dir}_{valname}");
                    let cnt_diff_kilo = get_diff(old_stat, *value, dt) / 1e3;
                    match count_stat.get_mut(&label) {
                        Some(v) => v.push(cnt_diff_kilo),
                        None => {
                            count_stat.insert(label, vec![cnt_diff_kilo]);
                        }
                    }
                }
            }
        }
    }

    remove_nonchanging_data(&mut bytes_stat);
    remove_nonchanging_data(&mut count_stat);
    Ok(NetDev {
        time,
        bytes_stat,
        count_stat,
    })
}

pub fn parse_net_dev(content: &str) -> Result<NetDev> {
    let data = cut_poller_header(content)?;
    let chunks = to_chunks(data);
    process_net_dev_chunks(&chunks)
}
