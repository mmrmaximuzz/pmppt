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

use crate::common::Res;

fn split_header(content: &str) -> Res<(&str, &str)> {
    let (header, rest) = content.split_once('\n').ok_or("header issue")?;
    Ok((header.trim(), rest.trim()))
}

fn split_chunks(content: &str) -> Res<Vec<&str>> {
    split_chunks_custom(content, "\n\n")
}

fn split_chunks_iostat(content: &str) -> Res<Vec<&str>> {
    split_chunks_custom(content, "\n\n\n")
}

fn split_chunks_custom<'a>(content: &'a str, pattern: &str) -> Res<Vec<&'a str>> {
    // remove the last entry of mpstat, it is not useful for us
    let (chunks, _) = content.rsplit_once(pattern).ok_or("not enough records")?;

    Ok(chunks.trim().split(pattern).collect())
}

pub mod mpstat {
    use std::cell::OnceCell;

    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    use crate::{
        common::Res,
        plotters::sysstat::{split_chunks, split_header},
    };

    struct MpstatHeader {
        kernel: String,
        date: NaiveDate,
        nr_cpus: usize,
    }

    pub fn parse(content: &str) -> Res<Mpstat> {
        let (header, rest) = split_header(content)?;
        let header = parse_mpstat_header(header)?;
        let chunks = split_chunks(rest)?;
        process_chunks(chunks, header)
    }

    fn parse_mpstat_header(hdr: &str) -> Res<MpstatHeader> {
        let parts: Vec<&str> = hdr.split_ascii_whitespace().collect();
        let kernel = parts[1].to_string();
        let datestr = parts[3];
        let cpus_str = parts[5];

        let date = NaiveDate::parse_from_str(datestr, "%m/%d/%Y")
            .map_err(|e| format!("bad mpstat header - failed to parse date {datestr}: {e}"))?;
        let nr_cpus: usize = cpus_str
            .matches(char::is_numeric)
            .collect::<String>()
            .parse()
            .map_err(|e| format!("failed to parse number of CPUS in '{cpus_str}: {e}"))?;

        Ok(MpstatHeader {
            kernel,
            date,
            nr_cpus,
        })
    }

    #[derive(Debug)]
    enum MpstatColumn {
        Time,
        Cpu,
        Usr,
        Nice,
        Sys,
        Iowait,
        Irq,
        Soft,
        Idle,
    }

    impl MpstatColumn {
        fn guess_from_str(col: &str) -> Option<MpstatColumn> {
            match col {
                "CPU" => MpstatColumn::Cpu,
                "%usr" => MpstatColumn::Usr,
                "%nice" => MpstatColumn::Nice,
                "%sys" => MpstatColumn::Sys,
                "%iowait" => MpstatColumn::Iowait,
                "%irq" => MpstatColumn::Irq,
                "%soft" => MpstatColumn::Soft,
                "%idle" => MpstatColumn::Idle,
                _ => return None,
            }
            .into()
        }
    }

    fn initialize_column_map(chunks: &[&str]) -> Res<Vec<Option<MpstatColumn>>> {
        let first = chunks[0];
        let col_line = first
            .lines()
            .next()
            .ok_or("failed to get mpstat columns line from first chunk")?;

        // explicitly skip the first column as it should be Time but mpstat shows different
        let col_iter = col_line
            .split_ascii_whitespace()
            .skip(1)
            .map(MpstatColumn::guess_from_str);

        // push Time column in the front manually
        Ok(vec![Some(MpstatColumn::Time)]
            .into_iter()
            .chain(col_iter)
            .collect())
    }

    #[derive(Debug, Default)]
    pub struct Mpstat {
        pub time: Vec<NaiveDateTime>,
        pub usr: Vec<Vec<f64>>,
        pub sys: Vec<Vec<f64>>,
        pub irq: Vec<Vec<f64>>,
        pub soft: Vec<Vec<f64>>,
        pub busy: Vec<Vec<f64>>,
        pub iowait: Vec<Vec<f64>>,
        pub kernel: String,
        pub nr_cpus: usize,
    }

    fn get_cell<T: Copy>(cell: &OnceCell<T>) -> Res<T> {
        Ok(*cell.get().ok_or("cannot get once cell".to_string())?)
    }

    fn process_chunks(chunks: Vec<&str>, header: MpstatHeader) -> Res<Mpstat> {
        let colmap = initialize_column_map(&chunks)?;
        let mut stat = Mpstat {
            kernel: header.kernel,
            nr_cpus: header.nr_cpus,
            ..Default::default()
        };

        for chunk in chunks {
            let mut lines = chunk.lines();
            let _ = lines.next().ok_or("failed to skip mpstat column line")?;
            let _ = lines.next().ok_or("failed to skip mpstat all CPU line")?;

            // catch the chunk time
            let current_time = OnceCell::new();

            // prepare the arrays for CPU loads
            let mut usr = vec![f64::NAN; header.nr_cpus];
            let mut sys = vec![f64::NAN; header.nr_cpus];
            let mut irq = vec![f64::NAN; header.nr_cpus];
            let mut soft = vec![f64::NAN; header.nr_cpus];
            let mut busy = vec![f64::NAN; header.nr_cpus];
            let mut iowait = vec![f64::NAN; header.nr_cpus];

            for cpu_line in lines {
                let current_cpu = OnceCell::new();
                for (item, coltype) in cpu_line.split_ascii_whitespace().zip(&colmap) {
                    match coltype {
                        Some(MpstatColumn::Time) => {
                            let time = item
                                .parse::<NaiveTime>()
                                .map_err(|e| format!("bad time {item}: {e}"))?;

                            let timestamp = NaiveDateTime::new(header.date, time);
                            if *current_time.get_or_init(|| timestamp) != timestamp {
                                return Err(format!("time changed: {time}"));
                            }
                        }
                        Some(MpstatColumn::Cpu) => {
                            let cpu = item
                                .parse::<usize>()
                                .map_err(|e| format!("bad cpu {item}: {e}"))?;

                            // there must be only one CPU column in the line
                            current_cpu.set(cpu).map_err(|e| {
                                format!("CPU {cpu} column found several times: {e}")
                            })?;
                        }
                        Some(MpstatColumn::Idle) => {
                            let idle = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad idle {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            busy[cpu] = 100.0 - idle;
                        }
                        Some(MpstatColumn::Usr) => {
                            let value = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad usr {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            usr[cpu] = value;
                        }
                        Some(MpstatColumn::Sys) => {
                            let value = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad sys {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            sys[cpu] = value;
                        }
                        Some(MpstatColumn::Irq) => {
                            let value = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad irq {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            irq[cpu] = value;
                        }
                        Some(MpstatColumn::Soft) => {
                            let value = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad soft {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            soft[cpu] = value;
                        }
                        Some(MpstatColumn::Iowait) => {
                            let value = item
                                .parse::<f64>()
                                .map_err(|e| format!("bad iowait {item}: {e}"))?;
                            let cpu = get_cell(&current_cpu)?;
                            iowait[cpu] = value;
                        }
                        _ => continue,
                    }
                }
            }

            stat.time.push(
                *current_time
                    .get()
                    .ok_or("failed to find time column".to_string())?,
            );
            stat.busy.push(busy);
            stat.usr.push(usr);
            stat.sys.push(sys);
            stat.irq.push(irq);
            stat.soft.push(soft);
            stat.iowait.push(iowait);
        }

        // FIXME: find the other way to normalize colorbar
        stat.busy[0][0] = 100.0;
        stat.usr[0][0] = 100.0;
        stat.sys[0][0] = 100.0;
        stat.irq[0][0] = 100.0;
        stat.soft[0][0] = 100.0;
        stat.iowait[0][0] = 100.0;
        Ok(stat)
    }

    #[cfg(test)]
    mod test {
        use chrono::NaiveDate;

        use super::parse_mpstat_header;

        #[test]
        fn mpstat_header() {
            let hdr = "Linux 6.17.4 (hostname) 	10/20/2025 	_x86_64_	(6 CPU)";
            let hdr = parse_mpstat_header(hdr).unwrap();
            assert_eq!(hdr.kernel, "6.17.4");
            assert_eq!(hdr.date, NaiveDate::from_ymd_opt(2025, 10, 20).unwrap());
            assert_eq!(hdr.nr_cpus, 6);
        }
    }
}

pub mod iostat {
    use std::collections::{HashMap, HashSet};

    use chrono::NaiveDateTime;

    use crate::common::Res;

    use super::{split_chunks_iostat, split_header};

    enum IostatCol {
        ReadRate,
        ReadRateMBytes,
        ReadAvgSize,
        WriteRate,
        WriteRateMBytes,
        WriteAvgSize,
        QueueAvgLength,
        Utilization,
    }

    impl IostatCol {
        fn from_str(s: &str) -> Option<Self> {
            match s {
                "r/s" => Self::ReadRate,
                "rMB/s" => Self::ReadRateMBytes,
                "rareq-sz" => Self::ReadAvgSize,
                "w/s" => Self::WriteRate,
                "wMB/s" => Self::WriteRateMBytes,
                "wareq-sz" => Self::WriteAvgSize,
                "aqu-sz" => Self::QueueAvgLength,
                "%util" => Self::Utilization,
                _ => return None,
            }
            .into()
        }
    }

    #[derive(Default, Debug)]
    pub struct Iostat {
        pub times: Vec<String>,
        pub disks: HashSet<String>,
        pub stats: HashMap<String, Vec<f64>>,
    }

    pub fn parse(content: &str) -> Res<Iostat> {
        let mut iostat = Iostat::default();

        let (_, content) = split_header(content)?; // we dont need iostat header
        let chunks = split_chunks_iostat(content)?;

        // parse the column types
        let columns: Vec<_> = {
            let fstchunk = chunks[0];
            fstchunk
                .lines()
                .nth(1)
                .ok_or_else(|| format!("bad first chunk: {fstchunk}"))?
                .split_ascii_whitespace()
                .skip(1) // skip the first column as it is device name
                .map(IostatCol::from_str)
                .collect()
        };

        for chunk in chunks {
            let (time, lines) = chunk
                .split_once('\n')
                .ok_or_else(|| format!("bad chunk {chunk}"))?;
            let tstamp = NaiveDateTime::parse_from_str(time, "%m/%d/%Y %I:%M:%S %p")
                .map_err(|e| format!("failed to parse time {time}: {e}"))?;
            iostat.times.push(tstamp.to_string());

            // skip the first line (it is header that we already parsed)
            for line in lines.lines().skip(1) {
                let mut items = line.split_ascii_whitespace();

                // explicitly extract the disk name
                let disk = items
                    .next()
                    .ok_or_else(|| format!("bad iostat line: {line}"))?;

                iostat.disks.insert(disk.to_string());

                // then extract items
                for (item, item_type) in items.zip(columns.iter()) {
                    let value = item
                        .parse::<f64>()
                        .map_err(|e| format!("bad read rate {item}: {e}"))?;
                    let label = match item_type {
                        Some(IostatCol::ReadRate) => format!("{disk}_riops"),
                        Some(IostatCol::ReadRateMBytes) => format!("{disk}_rMBs"),
                        Some(IostatCol::ReadAvgSize) => format!("{disk}_rsize"),
                        Some(IostatCol::WriteRate) => format!("{disk}_wiops"),
                        Some(IostatCol::WriteRateMBytes) => format!("{disk}_wMBs"),
                        Some(IostatCol::WriteAvgSize) => format!("{disk}_wsize"),
                        Some(IostatCol::QueueAvgLength) => format!("{disk}_qlen"),
                        Some(IostatCol::Utilization) => format!("{disk}_util"),
                        None => continue,
                    };

                    match iostat.stats.get_mut(&label) {
                        Some(v) => v.push(value),
                        None => {
                            iostat.stats.insert(label, vec![value]);
                        }
                    };
                }
            }
        }
        Ok(iostat)
    }
}

#[cfg(test)]
mod test {
    use crate::plotters::sysstat::split_header;

    #[test]
    fn dont_parse_empty_content() {
        let content = "";
        assert!(split_header(content).is_err());
    }

    #[test]
    fn dont_parse_no_newline() {
        let content = "some string";
        assert!(split_header(content).is_err());
    }

    #[test]
    fn parse_ok() {
        let content = "header\n\n\nrest";
        let (hdr, rest) = split_header(content).unwrap();
        assert_eq!(hdr, "header");
        assert_eq!(rest, "rest");
    }
}
