mod config;
mod gcode;
mod quick_math;

use std::time::Instant;
use std::{env, fs};
use std::path::Path;
use quick_math::distance_3d;

/*
TODO (problems) :
- Treat PrusaSlicer's commands (Low feedrate : issue related to acceleration commands)
*/

/*
TODO (optimizations) :
- Merge extrusion movements
- Multi-threading
- Usage of Z-hops only
- Problems separation according to size
- Multiple layers
- Deletion of negligible movements
- LKH parameters (Initial tour for LKH, other parameters, etc...)
- Usage of LKH via source code instead of calling the program
*/

struct Optimizer {
    config: config::Config,

    base_gcode: gcode::GCode,
    optimized_gcode: gcode::GCode,

    last_position: (f64, f64, f64),
    current_layer: u32,
    last_extrusion: f64,
}

impl Optimizer {
    fn set_units(&mut self) {
        self.optimized_gcode.stats.units_mode = self.base_gcode.stats.units_mode;
    }

    fn optimize(&mut self) {
        // Start of file
        self.optimized_gcode.contents.push_str(";Generated with TSP G-code optimizer V0.1\n");
        self.optimized_gcode.contents.push_str(&format!(";Original file: {}\n", self.base_gcode.file_path));
        self.optimized_gcode.contents.push_str("G28\n");
        match self.optimized_gcode.stats.units_mode {
            gcode::UnitsMode::Millimeters => self.optimized_gcode.contents.push_str("G21\n"),
            gcode::UnitsMode::Inches => self.optimized_gcode.contents.push_str("G20\n"),
            _ => (),
        }
        match self.optimized_gcode.position_mode {
            gcode::CoordinatesMode::Absolute => self.optimized_gcode.contents.push_str("G90\n"),
            gcode::CoordinatesMode::Relative => self.optimized_gcode.contents.push_str("G91\n"),
            _ => (),
        }
        match self.optimized_gcode.extruder_mode {
            gcode::CoordinatesMode::Absolute => self.optimized_gcode.contents.push_str("M82\n"),
            gcode::CoordinatesMode::Relative => self.optimized_gcode.contents.push_str("M83\n"),
            _ => (),
        }
        self.optimized_gcode.contents.push_str(&self.base_gcode.start_commands);
        self.optimized_gcode.contents.push_str("G92 E0\n");

        // Optimize G-code
        let layers = self.base_gcode.layers.to_vec();
        for layer in layers.iter() {

            if layer.nodes.len() as u32 > 3 {
                println!("Solving layer {}/{} ({} nodes)", self.current_layer, self.base_gcode.layers.len() - 1, layer.nodes.len());

                let parameters_path = format!("{}.par", self.current_layer);
                let tsp_path = format!("{}.tsp", self.current_layer);
                let result_path = format!("result_{}.tour", self.current_layer);

                // Write parameters file
                self.write_parameters_file(&parameters_path, &tsp_path, &result_path);

                // Write TSP file
                self.write_tsp_file(&tsp_path, layer);

                // Run TSP solver
                std::process::Command::new(&self.config.program)
                    .arg(&parameters_path)
                    .output()
                    .expect("Failed to run TSP solver");

                // Read result file
                let result = fs::read_to_string(&result_path)
                    .unwrap_or_else(|_| panic!("Unable to read file {}", result_path));

                self.read_optimized_tour(&result, layer);

                // Clean up
                fs::remove_file(&parameters_path).unwrap();
                fs::remove_file(&tsp_path).unwrap();
                fs::remove_file(&result_path).unwrap();
            } else {
                println!("Skipping layer {}/{} ({} node-s)", self.current_layer, self.base_gcode.layers.len() - 1, layer.nodes.len());

                self.add_line(layer, 1, 1);
                for i in 2..layer.nodes.len() as i32 {
                    self.add_line(layer, i - 1, i);
                }
            }

            // Write buffer
            self.optimized_gcode.contents.push_str(&layer.end_commands);

            // Update current position
            self.current_layer += 1;
        }

        // End of file
        self.optimized_gcode.contents.push_str("M107\n");
        self.optimized_gcode.contents.push_str(&self.base_gcode.end_commands);
    }

    fn write_parameters_file(&self, path: &str, tsp_path: &str, result_path: &str) {
        let parameters = format!(
            "PROBLEM_FILE = {}\n\
            TOUR_FILE = {}\n\
            PRECISION = {}\n\
            RUNS = {}\n\
            CANDIDATE_SET_TYPE = POPMUSIC\n",
            tsp_path, 
            result_path, 
            self.config.precision, 
            self.config.num_runs
        );

        fs::write(path, parameters)
            .unwrap_or_else(|_| panic!("Unable to write file {}", path));
    }

    fn write_tsp_file(&self, path: &str, layer: &gcode::GCodeLayer) {
        let mut tsp = format!(
            "NAME: {}\n\
            COMMENT: {}\n\
            TYPE: TSP\n\
            DIMENSION: {}\n\
            EDGE_WEIGHT_TYPE: EUC_3D\n\
            NODE_COORD_SECTION\n",
            format_args!("Layer {}", self.current_layer),
            format_args!("Print optimization for current_layer {}", self.current_layer),
            layer.nodes.len()
        );

        // Write nodes
        for (i, node) in layer.nodes.iter().enumerate() {
            tsp.push_str(&format!("{} {:.3} {:.3} {:.3}\n", i + 1, node.0, node.1, node.2));
        }

        // Write mandatory edges
        tsp.push_str("FIXED_EDGES_SECTION\n");
        for edge in layer.extrusions.iter() {
            tsp.push_str(&format!("{} {}\n", edge.0, edge.0 + 1));
        }
        tsp.push_str(&format!("{} {}\n", layer.nodes.len(), 1));
        tsp.push_str("-1\nEOF\n");

        fs::write(path, tsp)
            .unwrap_or_else(|_| panic!("Unable to write file {}", path));
    }

    fn read_optimized_tour(&mut self, result: &str, layer: &gcode::GCodeLayer) {
        let mut process = false;
        let mut prev_node = 1;

        for line in result.lines() {
            if process {

                // Gather next node position
                let node = line.parse::<i32>().unwrap();
                if node == -1 {
                    break;
                }

                self.add_line(layer, prev_node, node);

                // Update previous node
                prev_node = node;

            } else {
                process = line.starts_with("TOUR_SECTION");
            }
        }
    }

    fn add_line(&mut self, layer: &gcode::GCodeLayer, origin: i32, destination: i32) {
        let pno = origin as u32;
        let no = destination as u32;
        
        let n = layer.nodes[destination as usize - 1];

        let mut x = n.0;
        let mut y = n.1;
        let mut z = n.2;

        if self.optimized_gcode.position_mode == gcode::CoordinatesMode::Relative {
            let p = layer.nodes[origin as usize - 1];

            x -= p.0;
            y -= p.1;
            z -= p.2;
        }

        // Prepare new g-code line
        let mut text = format!("X{} Y{} Z{}", x, y, z);

        if (destination - origin == 1 && layer.extrusions.contains_key(&pno)) ||
            (destination - origin == -1 && layer.extrusions.contains_key(&no)) {
            
            // Take a change of direction into account
            let mut e = layer.extrusions.get(
                if destination - origin == 1 { &pno }
                else { &no }
            ).unwrap();
            
            let extr = e + self.last_extrusion;
            if self.optimized_gcode.extruder_mode == gcode::CoordinatesMode::Absolute {
                e = &extr;
            }
            
            self.last_extrusion = *e;

            text = format!("G1 {} E{:.5}", text, e);
            self.optimized_gcode.stats.increment_extrusion(distance_3d(self.last_position, n));
        } else {
            text = format!("G0 {}", text);
            self.optimized_gcode.stats.increment_travel(distance_3d(self.last_position, n));
        }

        // Add feedrate if needed
        let f = layer.feedrates.get(
            if destination - origin == 1 { &pno }
            else if destination - origin == -1 { &no }
            else { &0 } // Will give default travel feedrate, this is used for new travel movements
        );

        if f > Some(&0.0) {
            text = format!("{} F{:.3}", text, f.unwrap());
        }

        // Add new line to optimized G-code
        self.optimized_gcode.contents.push_str(&text);
        self.optimized_gcode.contents.push('\n');

        // Update previous node
        self.last_position = n;
    }
}

fn main() {
    let now = Instant::now();

    // Get both file paths from command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        panic!("Usage: {} <config file> <G-code file>", args[0]);
    }

    let config_path = &args[1];
    let gcode_path = &args[2];

    // Read the configuration file
    let config = config::read_config(config_path);

    let path_gcode = Path::new(gcode_path);

    // Check that the G-code file exists
    if !path_gcode.exists() {
        panic!("File {} does not exist", gcode_path);
    }

    // Check that file has a .gcode extension
    if path_gcode.extension().unwrap_or_default() != "gcode" {
        panic!("File {} does not have a .gcode extension", gcode_path);
    }

    // Read contents of G-code file
    let contents = fs::read_to_string(gcode_path)
        .unwrap_or_else(|_| panic!("Unable to read file {}", gcode_path));

    // Check that G-code file is not empty
    if contents.is_empty() {
        panic!("File {} is empty", gcode_path);
    }

    // Set log file
    let log_path = format!("{}.log", gcode_path);
    if Path::new(&log_path).exists() {
        fs::remove_file(&log_path)
            .unwrap_or_else(|_| panic!("Unable to replace {}", log_path));
    }
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .chain(fern::log_file(&log_path).unwrap())
        .apply()
        .unwrap_or_else(|_| panic!("Unable to set log file {}", log_path));

    // Setup optimizer
    let optimized_file = format!("{}_optimized.gcode", gcode_path);

    let mut optimizer = Optimizer {
        config,
        base_gcode: gcode::GCode::read(gcode_path),
        optimized_gcode: gcode::GCode::new(&optimized_file,
            gcode::CoordinatesMode::Absolute,
            gcode::CoordinatesMode::Relative),
        last_position: (0.0, 0.0, 0.0),
        current_layer: 0,
        last_extrusion: 0.0,
    };

    optimizer.set_units();

    optimizer.optimize();

    optimizer.optimized_gcode.write();

    // Display stats
    println!("\nBase G-code stats:");
    optimizer.base_gcode.stats.display();
    println!("\nOptimized G-code stats:");
    optimizer.optimized_gcode.stats.display();

    println!("\nOptimization completed in {}", elapsed_time(now));
}

fn elapsed_time(now: Instant) -> String {
    let elapsed = now.elapsed();
    let secs = elapsed.as_secs();
    let millis = elapsed.subsec_millis();

    if secs > 60 {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{}m {}s", mins, secs)
    } else if secs > 0 {
        format!("{}s {}ms", secs, millis)
    } else {
        format!("{}ms", millis)
    }
}