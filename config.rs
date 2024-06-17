use std::{fs::File, io::BufReader, path::Path};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub program: String,
    pub minimum_nodes: u32,
    pub precision: u32,
    pub num_runs: u32
}

pub fn read_config(path: &str) -> Config {
    let file = File::open(path)
        .unwrap_or_else(|_| panic!("Unable to open file {}", path));
    let reader = BufReader::new(file);

    // Check that file contains JSON
    let config: Config = serde_json::from_reader(reader)
        .unwrap_or_else(|_| panic!("Unable to parse JSON in file {}", path));

    // Check that program is set and exists
    if config.program.is_empty() {
        panic!("Program not set in configuration file");
    }
    
    if !Path::new(&config.program).exists() {
        panic!("Program {} does not exist", config.program);
    }

    config
}