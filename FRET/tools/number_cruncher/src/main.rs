use clap::parser::ValueSource;
use clap::Parser;
use itertools::Group;
use itertools::Itertools;
use rayon::iter::ParallelBridge;
use rayon::prelude::*;
use rayon::result;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::path::PathBuf;
use rusqlite::{params, Connection, Result};
use std::collections::HashMap;

#[derive(clap::ValueEnum, Clone, PartialEq)]
enum Endpoint {
    AllMin,
    ToolMin,
    ToolMax,
    Max
}

#[derive(Parser)]
struct Config {
    /// Input
    #[arg(short, long, value_name = "DIR")]
    input: PathBuf,

    /// Output
    #[arg(short, long, value_name = "FILE", default_value = "out.sqlite")]
    output: PathBuf,

    /// End each group after the first termination
    #[arg(short, long, default_value = "max")]
    end_early: Endpoint,
}
fn visit_dirs(
    dir: &Path,
    results: &mut Vec<(PathBuf, String, String, String)>,
) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, results)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some("time") {
                if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                    let re = regex::Regex::new(r".*#[0-9]+\.time$").unwrap();
                    if re.is_match(file_name) {
                        if let Some(dir_name) = path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|s| s.to_str())
                        {
                            {
                                let mut file_stem =
                                    path.file_stem().unwrap().to_str().unwrap().split("#");
                                let case_name = file_stem.next().unwrap();
                                let case_number = file_stem.next().unwrap();
                                results.push((
                                    path.clone(),
                                    dir_name.to_string(),
                                    case_name.to_string(),
                                    case_number.to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn maxpoints_of_file(file_path: &Path) -> io::Result<Vec<(usize, usize)>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut results = Vec::new();
    let mut watermark = 0;
    let mut last_timestamp = 0;

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split(',');

        if let (Some(first_str), Some(second_str)) = (parts.next(), parts.next()) {
            let first: usize = first_str.trim().parse().unwrap();
            let second: usize = second_str.trim().parse().unwrap();

            if first > watermark {
                results.push((first, second));
                watermark = first;
            }
            last_timestamp = second;
        }
    }
    if results.len() > 1 {
        results[0].1 = 0;
        results.push((results[results.len() - 1].0, last_timestamp));
    }
    if results.len() == 0 {
        results.push((0, 0));
        results.push((0, last_timestamp));
    }

    Ok(results)
}

fn sample_maxpoints(points: &Vec<(usize, usize)>, samples: &Vec<usize>) -> Vec<(usize, usize)> {
    let mut todo = samples.iter().peekable();
    let mut ret = Vec::new();
    for i in 0..points.len() {
        if todo.peek().is_none() {
            // Done
            break;
        }
        while let Some(&&peek) = todo.peek() {
            if peek >= points[i].1 && (i+1 >= points.len() || peek < points[i+1].1) {
                // End or inside the interval
                ret.push((points[i].0, peek));
                todo.next();
            } else if peek < points[i].1 {
                if i == 0 {
                    // Before the first interval, just take the first
                    ret.push((points[i].0, peek));
                    todo.next();
                } else {
                    // Already passed
                    eprintln!("WARNING Skipped: {}", todo.next().unwrap());
                }
            } else {
                // Not yet
                break;
            }
        }
    }
    ret
}

// https://rust-lang-nursery.github.io/rust-cookbook/science/mathematics/statistics.html
fn mean(data: &[usize]) -> Option<f64> {
    let sum = data.iter().sum::<usize>() as f64;
    let count = data.len();

    match count {
        positive if positive > 0 => Some(sum / count as f64),
        _ => None,
    }
}

fn median(data: &[usize]) -> Option<f64> {
    let mut data = data.to_vec();
    data.sort();
    let size = data.len();
    if size == 0 {
        return None;
    }

    match size {
        even if even % 2 == 0 => {
            let fst_med = data[(even / 2) - 1];
            let snd_med = data[even / 2];

            fst_med.checked_add(snd_med).map(|x| x as f64 / 2.0)
        },
        odd => data.get(odd / 2).map(|x| *x as f64)
    }
}

// https://rust-lang-nursery.github.io/rust-cookbook/science/mathematics/statistics.html
fn std_deviation(data: &[usize]) -> Option<f64> {
    match (mean(data), data.len()) {
        (Some(data_mean), count) if count > 0 => {
            let variance = data
                .iter()
                .map(|value| {
                    let diff = data_mean - (*value as f64);

                    diff * diff
                })
                .sum::<f64>()
                / count as f64;

            Some(variance.sqrt())
        }
        _ => None,
    }
}

fn main() {
    let conf = Config::parse();

    let mut results = Vec::new();

    if let Err(e) = visit_dirs(&conf.input, &mut results) {
        eprintln!("Error reading directories: {}", e);
    }

    println!("Files: {:?}", results);
    let mut connection = Connection::open(conf.output).unwrap();
    connection.execute("DROP TABLE IF EXISTS combos", ()).unwrap();
    connection.execute("CREATE TABLE IF NOT EXISTS combos (casename TEXT, toolname TEXT, fullname TEXT PRIMARY KEY)", ()).unwrap();

    let mut points: Vec<_> = results
        .par_iter()
        .map(|(path, fuzzer, case, n)| {
            (
                case,
                fuzzer,
                n.parse::<usize>().unwrap(),
                maxpoints_of_file(path).unwrap(),
            )
        })
        .collect();
    let mut last_common_point = points.iter().map(|x| x.3.last().expect(&format!("Missing maxpoint for {}", x.0)).1).min().unwrap();
    points.sort_by_key(|x| x.0); // by case for grouping
    for (case, casegroup) in &points.into_iter().chunk_by(|x| x.0) {
        let casegroup = casegroup.collect::<Vec<_>>();
        let last_case_point = casegroup.iter().map(|x| x.3.last().unwrap().1).min().unwrap();
        println!("Processing case {}: {}", case, casegroup.len());
        let mut timestamps = Vec::new();
        for (_, _, _, points) in &casegroup {
            timestamps.extend(points.iter().map(|(_, t)| *t));
        }
        timestamps.sort();
        if matches!(conf.end_early, Endpoint::AllMin) {
            // Dont' sample anything after the shortest run
            timestamps = timestamps.into_iter().filter(|x| x<=&last_common_point).collect();
        }
        let least_runtime_per_tool = casegroup.iter().map(|g| (g.1, g.2, g.3.last().unwrap().1)).sorted_by_key(|x| x.0).chunk_by(|x| x.0).into_iter().map(|(tool, toolgroup)| (tool, toolgroup.min_by_key(|y| y.2))).collect::<HashMap<_,_>>();
        let longest_runtime_per_tool = casegroup.iter().map(|g| (g.1, g.2, g.3.last().unwrap().1)).sorted_by_key(|x| x.0).chunk_by(|x| x.0).into_iter().map(|(tool, toolgroup)| (tool, toolgroup.max_by_key(|y| y.2))).collect::<HashMap<_,_>>();
        timestamps.dedup();
        let mut maxpoints_per_tool = casegroup
            .par_iter()
            .map(|g| (g.0, g.1, g.2, sample_maxpoints(&g.3, &timestamps)))
            .collect::<Vec<_>>();
        maxpoints_per_tool.sort_by_key(|x| x.1); // by tool
        for (tool, toolgroup) in &maxpoints_per_tool.into_iter().chunk_by(|x| x.1) {
            let toolgroup = toolgroup.collect::<Vec<_>>();
            println!("Processing tool {}: {}", tool, toolgroup.len());
            let mut lowest_common_length = toolgroup
                .iter()
                .map(|(_, _, _, points)| points.len())
                .min()
                .unwrap();
            if conf.end_early == Endpoint::ToolMin {
                lowest_common_length = timestamps.binary_search(&least_runtime_per_tool[tool].unwrap().2).unwrap();
            }
            if conf.end_early == Endpoint::ToolMax {
                lowest_common_length = std::cmp::min(lowest_common_length, timestamps.binary_search(&longest_runtime_per_tool[tool].unwrap().2).unwrap());
            }
            let time_min_max_med_mean_sdiv : Vec<(usize,usize,usize,f64,f64,f64)> = (0..lowest_common_length)
                .into_par_iter()
                .map(|i| {
                    let slice = toolgroup.iter().map(|(_, _, _, p)| p[i].0).collect::<Vec<_>>();
                    assert_eq!(slice.len(), toolgroup.len());
                    (
                        toolgroup[0].3[i].1,
                        *slice.iter().min().unwrap_or(&0),
                        *slice.iter().max().unwrap_or(&0),
                        median(&slice).unwrap_or(0.0),
                        mean(&slice).unwrap_or(0.0),
                        std_deviation(&slice).unwrap_or(0.0),
                    )
                })
                .collect::<Vec<_>>();

            // Save to db
            connection.execute("INSERT INTO combos (casename, toolname, fullname) VALUES (?, ?, ?)", (case, tool, format!("{}${}",case, tool))).unwrap();
            connection.execute(&format!("DROP TABLE IF EXISTS {}${}", case, tool), ()).unwrap();
            connection.execute(&format!("CREATE TABLE IF NOT EXISTS {}${} (timestamp INTEGER PRIMARY KEY, min INTEGER, max INTEGER, median REAL, mean REAL, sdiv REAL)", case, tool), ()).unwrap();

            // Start a transaction
            let transaction = connection.transaction().unwrap();

            let mut stmt = transaction.prepare(&format!(
                "INSERT INTO {}${} (timestamp , min , max , median , mean , sdiv ) VALUES (?, ?, ?, ?, ?, ?)",
                case, tool
            )).unwrap();

            for (timestamp, min, max, median, mean, sdiv) in time_min_max_med_mean_sdiv {
                stmt.execute([(timestamp as i64).to_string(), (min as i64).to_string(), (max as i64).to_string(), median.to_string(), mean.to_string(), sdiv.to_string()]).unwrap();
            }
            drop(stmt);

            // Commit the transaction
            transaction.commit().unwrap();
        }
    }
}
