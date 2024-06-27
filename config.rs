use std::{fs::File, io::BufReader, path::Path};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub program: String,
    pub precision: u32,
    pub num_runs: u32,
    pub max_merge_length: f64,
}

pub fn read_config(path: &str) -> Config {
    let file = File::open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path));
    let reader = BufReader::new(file);

    // Check that file contains JSON
    let mut config: Config = serde_json::from_reader(reader)
        .unwrap_or_else(|_| panic!("Unable to parse JSON in file {}", path));

    // Check that program is set and exists
    if config.program.is_empty() {
        panic!("Program not set in configuration file");
    }
    
    if !Path::new(&config.program).exists() {
        panic!("Program {} does not exist", config.program);
    }

    if config.max_merge_length == 0.0 {
        config = Config {
            program: config.program,
            precision: config.precision,
            num_runs: config.num_runs,
            max_merge_length: f64::INFINITY,
        };
    }

    config
}