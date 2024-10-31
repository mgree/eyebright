use std::io::Read;

const PATH_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/brightness";
const PATH_MAX_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/max_brightness";

fn main() {
    let brightness = read_file_as_u32(&PATH_BRIGHTNESS).expect("brightness");
    let max_brightness = read_file_as_u32(&PATH_MAX_BRIGHTNESS).expect("max brightness");

    println!(
        "{:.1}% ({brightness}/{max_brightness})",
        100.0 * (f64::from(brightness) / f64::from(max_brightness))
    );
}

fn read_file_as_u32(path: &str) -> Result<u32, Error> {
    let mut buf = String::with_capacity(16);

    let _read_bytes = std::fs::OpenOptions::new()
        .read(true)
        .write(false)
        .open(path)
        .map_err(Error::IOError)?
        .read_to_string(&mut buf)
        .map_err(Error::IOError)?;

    str::parse(buf.trim()).map_err(Error::ParseError)
}

#[derive(Debug)]
enum Error {
    IOError(std::io::Error),
    ParseError(std::num::ParseIntError),
}
