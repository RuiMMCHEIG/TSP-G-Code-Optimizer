// Get position from a line of G-code
pub fn get_position(line: &str, current_position: (f64, f64, f64)) -> (f64, f64, f64) {
    let mut position = current_position;
    for part in line.split_whitespace() {
        match part.chars().next() {
            Some('X') => position.0 = part[1..].parse().unwrap(),
            Some('Y') => position.1 = part[1..].parse().unwrap(),
            Some('Z') => position.2 = part[1..].parse().unwrap(),
            _ => (),
        }
    }
    position
}

// Calculate distance between two points in 3D space
pub fn distance_3d(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

// Calculate distance between a point and the origin in 3D space
pub fn distance_to_origin(a: (f64, f64, f64)) -> f64 {
    (a.0.powi(2) + a.1.powi(2) + a.2.powi(2)).sqrt()
}