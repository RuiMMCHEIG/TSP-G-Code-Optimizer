use std::env;
use std::fs;
use std::collections::HashMap;
use std::io::Write;
use log::{info, warn};
use num_format::{Locale, ToFormattedString};

#[derive(PartialEq)]
enum CoordinatesMode {
    Absolute,
    Relative,
    NotSet
}

#[derive(PartialEq)]
enum UnitsMode {
    Millimeters,
    Inches, //TODO
    NotSet
}

fn main() {
    // Get the file path from command line arguments
    let args: Vec<String> = env::args().collect();
    let file_path = &args[1];

    // Check that file is a g-code file
    if !file_path.ends_with(".gcode") {
        println!("The file is not a g-code file");
        return;
    }

    // Read the file
    let contents = fs::read_to_string(file_path)
        .expect("Something went wrong reading the file");

    // Check that the file is not empty
    if contents.is_empty() {
        println!("The file is empty");
        return;
    }

    // Set log file but remove it if it already exists
    let log_file_path = format!("{}.log", file_path);
    if std::path::Path::new(&log_file_path).exists() {
        fs::remove_file(&log_file_path).unwrap();
    }
    let _ = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .chain(fern::log_file(log_file_path).unwrap())
        .apply();

    let optimized_file_path = format!("{}_optimized.gcode", file_path);
    let mut optimized_file = fs::File::create(&optimized_file_path).unwrap();

    // Process variables
    let mut line_number = 0;
    let mut last_position = (0.0, 0.0, 0.0);
    let mut position;
    let mut current_layer = 0;
    let mut current_z = 0.0;
    let mut last_extrusion = 0.0;
    let mut lkh_total_extrusion = 0.0;

    let mut lines_to_write: Vec<&str> = Vec::new();
    
    let mut position_mode = CoordinatesMode::NotSet;
    let mut extruder_mode = CoordinatesMode::NotSet;
    let mut units_mode = UnitsMode::NotSet;

    // LKH variables
    //let mut initial_tour_path = format!("{}.tour", current_layer);
    let mut parameters_path = format!("{}.par", current_layer);
    let mut tsp_path = format!("{}.tsp", current_layer);
    let mut result_path = format!("result_{}.tour", current_layer);
    
    let mut num_nodes = 1;
    let mut nodes = Vec::new();
    let mut mandatories = HashMap::new();
    let mut feedrates = HashMap::new();

    const MINIMUM_NODES: i32 = 5; // minimum nodes for a current_layer to be considered for optimization
    const DEFAULT_PRECISION: i32 = 1000; // for decimal places, 100 for 2, 1000 for 3, etc...
    const NUM_RUNS: i32 = 1;
    const TIME_LIMIT: i32 = 60; //seconds

    let lkh_program: &str;

    // Statistics variables
    let mut g0_count = 0;
    let mut g1_count = 0;
    let mut extrusion_distance = 0.0;
    let mut travel_distance = 0.0;
    let mut lkh_extrusion_distance = 0.0;
    let mut lkh_travel_distance = 0.0;

//---------------------------OS related section------------------------------//

    if env::consts::OS == "windows" {
        // Windows
        lkh_program = "./LKH-2.exe";
    } else if env::consts::OS == "linux" {
        // Linux
        lkh_program = "./LKH";
    } else {
        // Unsupported OS
        println!("Unsupported OS");
        return;
    }

//-----------------------------START OF PROGRAM------------------------------//

    nodes.push(last_position);

    // Parse the g-code file line by line
    for line in contents.lines() {
        line_number += 1;

        // Remove comment from line
        let line_w = line.split(';').next().unwrap();
        
        match line_w.split_whitespace().next() {

            /* G Commands */

            Some("G0") | Some("G1") => { // G0/G1 X0 Y0 Z0 E0 F0 S0, [rapid] move to position
                let command = line_w.split_whitespace().next().unwrap();

                if line_w.starts_with("G0") {
                    g0_count += 1;
                    
                } else {
                    g1_count += 1;
                }

                if position_mode == CoordinatesMode::NotSet {
                    warn!("{} command at line {} before positioning mode was set", command, line_number)
                }
                if units_mode == UnitsMode::NotSet {
                    warn!("{} command at line {} before units mode was set", command, line_number)
                }

                // Parse the line to get the position
                position = get_position(line_w, last_position);
                
                // Check if extrusion is enabled and process feed rate
                let mut extrudes = false;
                let mut extrusion = 0.0;
                let mut feed_rate = 0;

                for part in line.split_whitespace() {
                    if !extrudes && part.starts_with('E') && !part.starts_with("E-") {

                        extrudes = true;
                        extrusion = part[1..].parse().unwrap();
                        extrusion_distance += calculate_distance(last_position, position, &position_mode);
                    }

                    if part.starts_with('F') {
                        feed_rate = part.strip_prefix('F').unwrap().parse().unwrap();
                    }
                }

                // Calculate the distance between the last and current position
                if !extrudes {
                    travel_distance += calculate_distance(last_position, position, &position_mode);
                }

                // Process a change of layer and execute LKH
                if position.2 != current_z && extrudes {
                    if num_nodes > MINIMUM_NODES {

                        // Write parameters file
                        let parameters = format!(
                            "PROBLEM_FILE = {}\n\
                            TOUR_FILE = {}\n\
                            PRECISION = {}\n\
                            RUNS = {}\n\
                            TIME_LIMIT = {}\n",
                            tsp_path, result_path, DEFAULT_PRECISION, NUM_RUNS, TIME_LIMIT
                        );

                        fs::write(parameters_path.clone(), parameters).unwrap();

                        // Write TSP file
                        let mut tsp = format!(
                            "NAME: {}\n\
                            COMMENT: {}\n\
                            TYPE: TSP\n\
                            DIMENSION: {}\n\
                            EDGE_WEIGHT_TYPE: EUC_3D\n\
                            NODE_COORD_SECTION\n",
                            format_args!("Layer {}", current_layer),
                            format_args!("Print optimization for current_layer {}", current_layer),
                            num_nodes
                        );

                        // Write nodes
                        let mut i = 1;
                        for node in nodes.iter() {
                            tsp.push_str(&format!("{} {:.3} {:.3} {:.3}\n", i, node.0, node.1, node.2));
                            i += 1;
                        }

                        // Write mandatory edges
                        tsp.push_str("FIXED_EDGES_SECTION\n");
                        for mandatory in mandatories.iter() {
                            tsp.push_str(&format!("{} {}\n", mandatory.0, mandatory.0 + 1));
                        }
                        tsp.push_str(&format!("{} {}\n", num_nodes, 1));
                        tsp.push_str("-1\nEOF\n");

                        fs::write(&tsp_path, tsp).unwrap();

                        // Run LKH
                        std::process::Command::new(lkh_program)
                            .arg(parameters_path.clone())
                            .output()
                            .expect("Failed to run LKH");

                        // Parse the result file
                        let result = fs::read_to_string(result_path.clone()).unwrap();
                        
                        let mut process = false;
                        let mut prev_node = 1;

                        for line in result.lines() {
                            if process {
                                if line.starts_with("-1") {
                                    break;
                                }

                                // Gather next node position
                                let node = line.parse::<i32>().unwrap();
                                let n = nodes[node as usize - 1];

                                let mut x = n.0;
                                let mut y = n.1;
                                let mut z = n.2;

                                if position_mode == CoordinatesMode::Relative {
                                    let p = nodes[prev_node as usize - 1];

                                    x -= p.0;
                                    y -= p.1;
                                    z -= p.2;
                                }

                                // Prepare new g-code line
                                let mut text = format!("X{} Y{} Z{}",
                                    x, y, z
                                );

                                if (node - prev_node == 1 && mandatories.contains_key(&prev_node)) || 
                                    (node - prev_node == -1 && mandatories.contains_key(&node)) {
                                    lkh_extrusion_distance += calculate_distance(
                                        nodes[prev_node as usize - 1], 
                                        nodes[node as usize - 1], 
                                        &position_mode
                                    );

                                    // Take a change of direction into account
                                    let mut e = mandatories.get(
                                        if node - prev_node == 1 { &prev_node } 
                                        else { &node }
                                    ).unwrap();

                                    lkh_total_extrusion += e;
                                    if extruder_mode == CoordinatesMode::Absolute {
                                        e = &lkh_total_extrusion;
                                    }

                                    // Add extrusion to line
                                    text = format!("G0 {} E{:.5}", text, e);

                                } else {
                                    lkh_travel_distance += calculate_distance(
                                        nodes[prev_node as usize - 1], 
                                        nodes[node as usize - 1], 
                                        &position_mode
                                    );

                                    text = format!("G1 {}", text);
                                }

                                // Add feedrate if needed
                                let f = feedrates.get(
                                    if node - prev_node == 1 { &prev_node } 
                                    else { &node }
                                ).unwrap_or(&0);

                                if f > &0 {
                                    text = format!("{} F{}", text, f);
                                }

                                // Write to new g-code file
                                write_line(&mut optimized_file, text.as_str());

                                prev_node = node;

                            } else {
                                process = line.starts_with("TOUR_SECTION");
                            }
                        }

                        // Delete created files for this layer
                        fs::remove_file(parameters_path).unwrap();
                        fs::remove_file(tsp_path).unwrap();
                        fs::remove_file(result_path).unwrap();

                        // Write lines in the buffer
                        for line in lines_to_write.iter() {
                            write_line(&mut optimized_file, line);
                        }
                        lines_to_write.clear();
                    }

                    // Reset variables
                    current_layer += 1;

                    //initial_tour_path = format!("{}.tour", current_layer);
                    parameters_path = format!("{}.par", current_layer);
                    tsp_path = format!("{}.tsp", current_layer);
                    result_path = format!("result_{}.tour", current_layer);

                    num_nodes = 1;

                    nodes.clear();
                    nodes.push(last_position);
                    mandatories.clear();
                    feedrates.clear();

                    current_z = position.2;
                }

                // Add node to tour
                nodes.push(position);

                if feed_rate > 0 {
                    feedrates.insert(num_nodes, feed_rate);
                }

                // Mark edge as mandatory
                if extrudes {
                    let mut e = extrusion;
                    if position_mode != CoordinatesMode::Relative {
                        e -= last_extrusion;
                    }
                    mandatories.insert(num_nodes, e);
                } 

                num_nodes += 1;
                
                // Updates last position
                last_position = position;
                last_extrusion = extrusion;
            }
            Some("G4") => { // G4 P0, dwell
                info!("G4 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("G21") => { // G21, set to millimeters
                if units_mode != UnitsMode::NotSet {
                    warn!("G21 command at line {} after units mode was already set", line_number)
                }
                units_mode = UnitsMode::Millimeters;
                info!("G21 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("G28") => { // G28 X0 Y0 Z0, move to origin (Home)
                info!("G28 command at line {}", line_number);
                position = get_position(line_w, last_position);
                travel_distance += calculate_distance(last_position, position, &position_mode);
                last_position = position;
                write_line(&mut optimized_file, line);
            }
            Some("G29") => { // G29 S0, detailed z-probe
                info!("G29 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("G90") => { // G90, set to absolute positioning
                if position_mode != CoordinatesMode::NotSet {
                    warn!("G90 command at line {} after positioning mode was already set", line_number)
                }
                position_mode = CoordinatesMode::Absolute;
                info!("G90 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("G91") => { // G91, set to relative positioning
                if position_mode != CoordinatesMode::NotSet {
                    warn!("G91 command at line {} after positioning mode was already set", line_number)
                }
                position_mode = CoordinatesMode::Relative;
                info!("G91 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("G92") => { // G92 X0 Y0 Z0 E0, set current position
                info!("G92 command at line {}", line_number);
                last_position = get_position(line_w, last_position);
                write_line(&mut optimized_file, line);
            }

            /* M Commands */

            Some("M17") => { // M17, enable motors
                info!("M17 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M73") => { // M73 P0, set/get build percentage
                info!("M73 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M82") => { // M82, set extruder to absolute mode
                if extruder_mode != CoordinatesMode::NotSet {
                    warn!("M82 command at line {} after extruder mode was already set", line_number);
                }
                extruder_mode = CoordinatesMode::Absolute;
                info!("M82 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M83") => { // M83, set extruder to relative mode
                if extruder_mode != CoordinatesMode::NotSet {
                    warn!("M83 command at line {} after extruder mode was already set", line_number);
                }
                extruder_mode = CoordinatesMode::Relative;
                info!("M83 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M84") => { // M84, disable motors
                info!("M84 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M104") => { // M104 S0, set extruder temperature
                info!("M104 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M106") => { // M106, turn on fan
                info!("M106 command at line {}", line_number);
                lines_to_write.push(line);
                //write_line(&mut optimized_file, line);
            }
            Some("M107") => { // M107, turn off fan
                info!("M107 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M140") => { // M140 S0, set bed temperature
                info!("M140 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M190") => { // M190 S0, wait for bed temperature to reach target
                info!("M190 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M201") => { // M201 X0 Y0 Z0 E0, set max acceleration
                info!("M201 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M204") => { // M204 P0, set default acceleration
                info!("M204 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }
            Some("M74") | 
            Some("M109") | Some("M115") | Some("M142") | Some("M203") |
            Some("M205") | Some("M221") | Some("M302") | Some("M555") | 
            Some("M572") | Some("M593") | Some("M569") | Some("M862.1") | 
            Some("M862.3") | Some("M862.5") | Some("M862.6") | Some("M900") => { 
                let command = line.split_whitespace().next().unwrap();
                info!("{} command at line {}, applied default ignore behavior", command, line_number);
                write_line(&mut optimized_file, line);
            }

            /* Other Commands */

            Some("T0") => { // T0, select tool 0
                info!("T0 command at line {}", line_number);
                write_line(&mut optimized_file, line);
            }

            /* Unknown Commands */

            Some(command) => {
                // Ignore comments
                if !command.starts_with(";") {
                    // Log unknown commands
                    println!("Unknown command: {}", command);
                    warn!("Unknown command at line {}: {}", line_number, command);
                }
            }

            /* Empty Line */

            _ => {
                // Ignore empty lines
            }
        }
    }

    // Print statistics and log them
    println!("G0 commands: {}", g0_count.to_formatted_string(&Locale::en));
    info!("G0 commands: {}", g0_count.to_formatted_string(&Locale::en));
        
    println!("G1 commands: {}", g1_count.to_formatted_string(&Locale::en));
    info!("G1 commands: {}", g1_count.to_formatted_string(&Locale::en));

    let extrusion_dist = format!("{:.5} {}", extrusion_distance, match units_mode {
        UnitsMode::Millimeters => "mm",
        UnitsMode::Inches => "in",
        UnitsMode::NotSet => "units"
    });

    let travel_dist = format!("{:.3} {}", travel_distance, match units_mode {
        UnitsMode::Millimeters => "mm",
        UnitsMode::Inches => "in",
        UnitsMode::NotSet => "units"
    });

    let lkh_extrusion_dist = format!("{:.5} {}", lkh_extrusion_distance, match units_mode {
        UnitsMode::Millimeters => "mm",
        UnitsMode::Inches => "in",
        UnitsMode::NotSet => "units"
    });

    let lkh_travel_dist = format!("{:.3} {}", lkh_travel_distance, match units_mode {
        UnitsMode::Millimeters => "mm",
        UnitsMode::Inches => "in",
        UnitsMode::NotSet => "units"
    });

    println!("Extrusion distance: {}", extrusion_dist);
    info!("Extrusion distance: {}", extrusion_dist);

    println!("Travel distance: {}", travel_dist);
    info!("Travel distance: {}", travel_dist);

    println!("LKH extrusion distance: {}", lkh_extrusion_dist);
    println!("LKH travel distance: {}", lkh_travel_dist);
}

// Get position from G0 and G1 commands
fn get_position(line: &str, current_pos: (f64, f64, f64)) -> (f64, f64, f64) {
    let mut position = current_pos;
    for part in line.split_whitespace() {
        if part.starts_with("X") {
            position.0 = part[1..].parse().unwrap();
        } else if part.starts_with("Y") {
            position.1 = part[1..].parse().unwrap();
        } else if part.starts_with("Z") {
            position.2 = part[1..].parse().unwrap();
        }
    }
    position
}

// Calculate distance between two points
fn calculate_distance(origin: (f64, f64, f64), dest: (f64, f64, f64), mode: &CoordinatesMode) -> f64 {
    if *mode == CoordinatesMode::Relative {
        return (dest.0.powi(2) + dest.1.powi(2) + dest.2.powi(2)).sqrt();
    }
    (
        (origin.0 - dest.0).powi(2) + 
        (origin.1 - dest.1).powi(2) + 
        (origin.2 - dest.2).powi(2))
        .sqrt()
}

// Write a line to a file and add a new line
fn write_line(file: &mut fs::File, line: &str) {
    file.write_all(line.as_bytes()).unwrap();
    file.write_all(b"\n").unwrap();
}