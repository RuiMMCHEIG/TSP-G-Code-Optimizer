use std::collections::HashMap;
use log::{info, warn};
use crate::quick_math::{get_position, distance_3d, distance_to_origin};

#[derive(PartialEq)]
pub enum CoordinatesMode {
    Absolute,
    Relative,
    NotSet
}

#[derive(PartialEq, Clone, Copy)]
pub enum UnitsMode {
    Millimeters,
    Inches,
    NotSet
}

pub struct GCode {
    pub file_path: String,
    pub contents: String,

    pub position_mode: CoordinatesMode,
    pub extruder_mode: CoordinatesMode,

    pub start_commands: String,
    pub end_commands: String,

    pub layers: Vec<GCodeLayer>,

    travel_count: u32,
    extrude_count: u32,
    pub stats: GCodeStats,
}

pub struct GCodeStats {
    extrusion_distance: f64,
    travel_distance: f64,
    pub units_mode: UnitsMode,
}

#[derive(Clone)]
pub struct GCodeLayer {
    pub nodes: Vec<(f64, f64, f64)>,
    pub extrusions: HashMap<u32, f64>,
    pub feedrates: HashMap<u32, f64>,
    pub end_commands: String,
}

impl GCode {
    // Reads a G-code file
    pub fn read(file_path: &str) -> GCode {
        let mut gcode = GCode {
            file_path: file_path.to_string(),
            contents: std::fs::read_to_string(file_path)
                .unwrap_or_else(|_| panic!("Unable to read file {}", file_path)),

            position_mode: CoordinatesMode::NotSet,
            extruder_mode: CoordinatesMode::NotSet,

            start_commands: String::new(),
            end_commands: String::new(),

            layers: Vec::new(),

            travel_count: 0,
            extrude_count: 0,
            stats: GCodeStats {
                extrusion_distance: 0.0,
                travel_distance: 0.0,
                units_mode: UnitsMode::NotSet,
            },
        };

        gcode.layers.push(GCodeLayer {
            nodes: Vec::new(),
            extrusions: HashMap::new(),
            feedrates: HashMap::new(),
            end_commands: String::new(),
        });

        // Processing variables
        let mut line_num = 0;
        let mut last_position = (0.0, 0.0, 0.0);
        let mut current_position: (f64, f64, f64);
        let mut current_layer: u32 = 0;
        let mut current_z = 0.0;
        let mut current_feedrate = 1500.0; // Default feedrate (1500 = 25 mm/s, safe value)
        let mut last_extrusion = 0.0;
        let mut last_travel_position = (0.0, 0.0, 0.0);
        let mut last_loop_travel = false;

        for line in gcode.contents.lines() {
            line_num += 1;
            let line = line.split(';').next().unwrap();
            
            match line.split_whitespace().next() {
                Some("G0") | Some("G1") => {
                    current_position = get_position(line, last_position);
                    
                    // Process extrusion and feed rate
                    let mut extrudes = false;
                    let mut extrusion = 0.0;
                    let mut feedrate: f64 = 0.0;

                    for part in line.split_whitespace() {
                        match part.chars().next() {
                            Some('E') => {
                                extrusion = part[1..].parse().unwrap();

                                if gcode.extruder_mode != CoordinatesMode::Relative {
                                    extrusion -= last_extrusion;
                                }

                                extrudes = extrusion > 0.0;
                            },
                            Some('F') => feedrate = part[1..].parse().unwrap(),
                            _ => (),
                        }
                    }

                    // Process stats
                    let distance = if gcode.position_mode != CoordinatesMode::Relative {
                        distance_3d(current_position, last_position)
                    } else {
                        distance_to_origin(current_position)
                    };

                    if extrudes {
                        gcode.extrude_count += 1;
                        gcode.stats.increment_extrusion(distance);
                    } else {
                        gcode.travel_count += 1;
                        gcode.stats.increment_travel(distance);
                    }

                    // Process a change of layer
                    if current_position.2 != current_z && extrudes {
                        if last_loop_travel {
                            last_loop_travel = false;
                        }
                        current_layer += 1;
                        current_z = current_position.2;

                        gcode.layers.push(GCodeLayer {
                            nodes: Vec::new(),
                            extrusions: HashMap::new(),
                            feedrates: HashMap::new(),
                            end_commands: String::new(),
                        });

                        gcode.layers[current_layer as usize].nodes.push(last_position);
                        gcode.layers[current_layer as usize].feedrates.insert(0, 9000.0); // Default travel feedrate (150 mm/s)
                    }

                    // nodes
                    let layer = &mut gcode.layers[current_layer as usize];
                    if extrudes {
                        if last_loop_travel {
                            layer.nodes.push(last_travel_position);
                            last_loop_travel = false;
                        }
                        layer.nodes.push(current_position);
                    } else if gcode.position_mode != CoordinatesMode::Relative {
                        last_travel_position = current_position;
                    } else {
                        last_travel_position = 
                            (last_travel_position.0 + current_position.0, 
                            last_travel_position.1 + current_position.1, 
                            last_travel_position.2 + current_position.2);
                    }

                    // extrusions
                    if extrudes {
                        layer.extrusions.insert(layer.nodes.len() as u32 - 1, extrusion);
                    } else {
                        last_loop_travel = true;
                    }

                    // feedrates
                    let n = layer.nodes.len() as u32 - if last_loop_travel { 0 } else { 1 };
                    if feedrate > 0.0 {
                        layer.feedrates.insert(n, feedrate);
                        current_feedrate = feedrate;
                    } else {
                        layer.feedrates.insert(n, current_feedrate);
                    }

                    // Update last position, extrusion and feedrate
                    if gcode.position_mode != CoordinatesMode::Relative {
                        last_position = current_position;
                    }

                    if gcode.extruder_mode != CoordinatesMode::Relative {
                        last_extrusion += extrusion;
                    } else {
                        last_extrusion = extrusion;
                    }
                },
                // Units mode: inches
                Some("G20") => {
                    if gcode.stats.units_mode != UnitsMode::NotSet {
                        warn!("G20 command at line {} after units mode was already set", line_num);
                    }
                    gcode.stats.units_mode = UnitsMode::Inches;
                },
                // Units mode: millimeters
                Some("G21") => {
                    if gcode.stats.units_mode != UnitsMode::NotSet {
                        warn!("G21 command at line {} after units mode was already set", line_num);
                    }
                    gcode.stats.units_mode = UnitsMode::Millimeters;
                },
                // Home all axes
                Some("G28") => {
                    current_position = get_position(line, (0.0, 0.0, 0.0));
                    gcode.stats.increment_travel(distance_3d(current_position, last_position));
                    last_position = current_position;

                    gcode.layers[current_layer as usize].nodes.push(current_position);
                },
                // Position mode: absolute
                Some("G90") => {
                    if gcode.position_mode != CoordinatesMode::NotSet {
                        warn!("G90 command at line {} after position mode was already set", line_num);
                    }
                    gcode.position_mode = CoordinatesMode::Absolute;
                },
                // Position mode: relative
                Some("G91") => {
                    if gcode.position_mode != CoordinatesMode::NotSet {
                        warn!("G91 command at line {} after position mode was already set", line_num);
                    }
                    gcode.position_mode = CoordinatesMode::Relative;
                },
                // Set current position
                Some("G92") => {
                    last_position = get_position(line, last_position);
                },
                // Extruder mode: absolute
                Some("M82") => {
                    if gcode.extruder_mode != CoordinatesMode::NotSet {
                        warn!("M82 command at line {} after extruder mode was already set", line_num);
                    }
                    gcode.extruder_mode = CoordinatesMode::Absolute;
                },
                // Extruder mode: relative
                Some("M83") => {
                    if gcode.extruder_mode != CoordinatesMode::NotSet {
                        warn!("M83 command at line {} after extruder mode was already set", line_num);
                    }
                    gcode.extruder_mode = CoordinatesMode::Relative;
                },
                // Bed temperature and other configuration commands
                Some("M84") | Some("M104") | Some("M107") | Some("M109") | Some("M140") | Some("M190") | Some("T0")
                | Some("G4") | Some("M593") | Some("M572") | Some("M142") | Some("M900") | Some("M221") | Some("M569")
                | Some("G29") | Some("M302") | Some("M555") | Some("M115") | Some("M17") | Some("M203") | Some("M205")
                | Some("M862.1") | Some("M862.3") | Some("M862.5") | Some("M862.6") => {
                    if current_layer == 0 {
                        gcode.start_commands.push_str(&format!("{}\n", line));
                    } else {
                        gcode.end_commands.push_str(&format!("{}\n", line));
                    }
                },
                // M106 : Turn on fan
                // M201 : Set max acceleration
                Some("M106") | Some("M201") => {
                    gcode.layers[current_layer as usize].end_commands.push_str(&format!("{}\n", line));
                },
                // M204 : Set default acceleration
                Some("M204") => {
                    // TODO : Handle M204 command
                    info!("Command {} not treated yet", line);
                },
                // M73 : Set/Get build percentage
                // M74 : Set weight on print bed
                Some("M73") | Some("M74") => {
                    //Ignore
                },
                // Unknown commands
                Some(command) => {
                    if !command.starts_with(';') {
                        println!("Unknown command {}", command);
                        warn!("Unknown command {} at line {}", command, line_num);
                    }
                },
                // Empty line
                _ => (),
            }
        }

        gcode
    }

    // Creates a new G-code file without content
    pub fn new(file_path: &str, 
            position_mode: CoordinatesMode, 
            extruder_mode: CoordinatesMode) -> GCode {

        GCode {
            file_path: file_path.to_string(),
            contents: String::new(),

            position_mode,
            extruder_mode,

            start_commands: String::new(),
            end_commands: String::new(),

            layers: Vec::new(),

            travel_count: 0,
            extrude_count: 0,
            stats: GCodeStats {
                extrusion_distance: 0.0,
                travel_distance: 0.0,
                units_mode: UnitsMode::NotSet,
            },
        }
    }

    // Writes contents to G-code file
    pub fn write(&self) {
        std::fs::write(&self.file_path, &self.contents)
            .unwrap_or_else(|_| panic!("Unable to write to file {}", self.file_path));
    }
}

impl GCodeStats {
    pub fn display(&self) {
        let units = match self.units_mode {
            UnitsMode::Millimeters => "mm",
            UnitsMode::Inches => "in",
            UnitsMode::NotSet => "units",
        };
        println!("Extrusion distance: {:.2} {}", self.extrusion_distance, units);
        println!("Travel distance: {:.2} {}", self.travel_distance, units);
    }

    pub fn log(&self, info: String) {
        let units = match self.units_mode {
            UnitsMode::Millimeters => "mm",
            UnitsMode::Inches => "in",
            UnitsMode::NotSet => "units",
        };
        info!("{}, extrusion distance: {:.2} {}", info, self.extrusion_distance, units);
        info!("{}, travel distance: {:.2} {}", info, self.travel_distance, units);
    }

    pub fn increment_extrusion(&mut self, distance: f64) {
        self.extrusion_distance += distance;
    }

    pub fn increment_travel(&mut self, distance: f64) {
        self.travel_distance += distance;
    }
}