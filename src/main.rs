use std::io::{Read, Write};

use clap::{Arg, Command};

const VERSION: &'static str = "v1";
const PATH_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/brightness";
const PATH_MAX_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/max_brightness";

fn main() {
    let argv0 = std::env::args().next().expect("argv[0]");
    let m = Command::new("eyebright")
        .author("Michael Greenberg <michael@greenberg.science>")
        .version(VERSION)
        .about("Manage backlight brightness on Intel displays")
        .arg(
            Arg::new("action")
                .required(false)
                .default_value("")
                .help("\n+10, -5%\trelative percentage change\n50, 75%\tabsolute percentage\n\nleave blank to get current brightness")
        )
        .get_matches();

    if let Err(e) = execute(&argv0, m) {
        eprintln!("{argv0}: {e}");
        std::process::exit(1);
    }
}

fn execute(argv0: &str, m: clap::ArgMatches) -> Result<(), Error> {
    let action = str::parse::<Action>(m.get_one::<String>("action").unwrap_or(&"".to_string()))?;

    // everybody needs this, so let's do it in advance
    let max_brightness = read_file_as_u32(PATH_MAX_BRIGHTNESS)?;

    let new_percentage = action.run(max_brightness, || read_file_as_u32(PATH_BRIGHTNESS))?;

    if let Some(percentage) = new_percentage {
        let clamped_percentage = percentage.clamp(0.0, 1.0);
        if clamped_percentage != percentage {
            eprintln!("{argv0}: {percentage} was out of range, clamped to {clamped_percentage}",)
        }

        let new_value = (f64::from(max_brightness) * percentage).round() as u32;

        write_file_from_u32(PATH_BRIGHTNESS, new_value)?;
    }

    Ok(())
}

/// Runs `action`, given the `max_brightness` and a function to get the current brightness
/// The `Option<f64>` is the new percentage of `max_brightness` to apply.
impl Action {
    fn run<F>(self, max_brightness: u32, get_brightness: F) -> Result<Option<f64>, Error>
    where
        F: FnOnce() -> Result<u32, Error>,
    {
        match self {
            Action::Set(change, SetMode::RelativeUp) => {
                let brightness = get_brightness()?;

                let current = f64::from(brightness) / f64::from(max_brightness);
                let delta = f64::from(change) / 100.0;

                Ok(Some(current + delta))
            }
            Action::Set(change, SetMode::RelativeDown) => {
                let brightness = get_brightness()?;

                let current = f64::from(brightness) / f64::from(max_brightness);
                let delta = -1.0 * f64::from(change) / 100.0;

                Ok(Some(current + delta))
            }
            Action::Set(percentage, SetMode::Absolute) => Ok(Some(f64::from(percentage) / 100.0)),
            Action::Get => {
                let brightness = get_brightness()?;
                println!(
                    "{:.0}%",
                    100.0 * (f64::from(brightness) / f64::from(max_brightness))
                );

                Ok(None)
            }
        }
    }
}

fn read_file_as_u32(path: &str) -> Result<u32, Error> {
    let mut buf = String::with_capacity(16);

    let _read_bytes = std::fs::OpenOptions::new()
        .read(true)
        .write(false)
        .open(path)
        .map_err(|cause| Error::with_cause(format!("could not read from {path}"), cause))?
        .read_to_string(&mut buf)
        .map_err(|cause| Error::with_cause(format!("invalid UTF-8 at {path}"), cause))?;

    let buf = buf.trim();
    str::parse(buf)
        .map_err(|cause| Error::with_cause(format!("could not parse '{buf}' as a number"), cause))
}

fn write_file_from_u32(path: &str, n: u32) -> Result<(), Error> {
    write!(
        std::fs::OpenOptions::new()
            .read(false)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|cause| Error::with_cause(
                format!("could not open {path} for writing; try using `sudo` or running `chmod u+s` on the command or `chmod +w on {path}"),
                cause
            ))?,
        "{n}"
    )
    .map_err(|cause| Error::with_cause(format!("could not write {n} to {path}"), cause))
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum Action {
    Set(u8, SetMode),
    Get,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(PartialEq))]
enum SetMode {
    Absolute,
    RelativeUp,
    RelativeDown,
}

impl std::str::FromStr for Action {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(Action::Get);
        }

        let (mut s, mode) = match s.chars().next() {
            None => return Ok(Action::Get), // should be unreachable, but belt and suspenders
            Some('+') => (&s[1..], SetMode::RelativeUp),
            Some('-') => (&s[1..], SetMode::RelativeDown),
            Some(_) => (s, SetMode::Absolute),
        };

        // drop % at the end
        if s.ends_with('%') {
            s = &s[..s.len() - 1];
        }

        let percentage = str::parse::<u8>(s)
            .map_err(|cause| Error::with_cause(format!("could not parse '{s}'"), cause))?;

        if percentage > 100 {
            return Err(Error::msg(format!("'{percentage}' is greater than 100%")));
        }

        Ok(Action::Set(percentage, mode))
    }
}

#[derive(Debug)]
struct Error {
    message: String,
    cause: Option<String>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(cause) = &self.cause {
            write!(f, " ({cause})")?;
        }
        Ok(())
    }
}

impl Error {
    fn msg(message: String) -> Self {
        Error {
            message,
            cause: None,
        }
    }

    fn with_cause<E: ToString>(message: String, cause: E) -> Self {
        Error {
            message: message,
            cause: Some(cause.to_string()),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_action_fits_in_usize() {
        assert!(std::mem::size_of::<Action>() <= std::mem::size_of::<usize>());
    }

    #[test]
    fn test_action_parser() {
        // we'll compute every valid case and just check things exhaustively
        let mut valid_cases = Vec::with_capacity(601);

        valid_cases.push(("".to_string(), Action::Get));
        for i in 0u8..=100 {
            valid_cases.push((format!("{i}"), Action::Set(i, SetMode::Absolute)));
            valid_cases.push((format!("{i}%"), Action::Set(i, SetMode::Absolute)));
            valid_cases.push((format!("+{i}"), Action::Set(i, SetMode::RelativeUp)));
            valid_cases.push((format!("+{i}%"), Action::Set(i, SetMode::RelativeUp)));
            valid_cases.push((format!("-{i}"), Action::Set(i, SetMode::RelativeDown)));
            valid_cases.push((format!("-{i}%"), Action::Set(i, SetMode::RelativeDown)));
        }

        for (input, expected) in valid_cases {
            match str::parse::<Action>(&input) {
                Err(e) => panic!("expected {expected:?} from {input}, got error {e:?}"),
                Ok(got) => assert_eq!(
                    got, expected,
                    "expected {expected:?} from '{input}', got {got:?}"
                ),
            }
        }

        // these should error out in some way
        let invalid_inputs = vec![
            "hi", "max", "min", "101", "101%", "1.2", "+1.2", "-1.2", "5.3%", "+5.3%", "-5.3%",
        ];

        for input in invalid_inputs {
            match str::parse::<Action>(input) {
                Err(_) => (),
                Ok(got) => panic!("expected an error from '{input}', got {got:?}"),
            }
        }
    }

    #[test]
    fn test_action_runner() {
        let mut cases = Vec::with_capacity(10504);
        let max_brightness = 100;

        for i in 0u8..=100 {
            cases.push((u32::from(i), Action::Get, u32::from(i)));

            for brightness in 0u8..=100 {
                cases.push((
                    u32::from(brightness),
                    Action::Set(i, SetMode::Absolute),
                    u32::from(i),
                ));
            }
        }

        for brightness in 0u8..=90 {
            cases.push((
                u32::from(brightness),
                Action::Set(10, SetMode::RelativeUp),
                u32::from(brightness + 10),
            ));
        }
        for clamped_brightness in 91u8..=100 {
            cases.push((
                u32::from(clamped_brightness),
                Action::Set(10, SetMode::RelativeUp),
                100,
            ));
        }

        for brightness in 10u8..=100 {
            cases.push((
                u32::from(brightness),
                Action::Set(10, SetMode::RelativeDown),
                u32::from(brightness - 10),
            ));
        }
        for clamped_brightness in 0u8..=9 {
            cases.push((
                u32::from(clamped_brightness),
                Action::Set(10, SetMode::RelativeDown),
                0,
            ));
        }

        assert_eq!(cases.len(), 10504);

        for (brightness, action, expected) in cases {
            match action.run(max_brightness, || Ok(brightness)) {
                Err(e) => panic!("expected {expected:?} from {action:?} on {brightness}/{max_brightness}, got error {e:?}"),
                Ok(got) => {
                    let new_brightness = match got {
                        Some(percentage) => {
                            let clamped_percentage = percentage.clamp(0.0, 1.0);
                            (clamped_percentage * f64::from(max_brightness)).round() as u32
                        }
                        None => brightness,
                    };
                    assert_eq!(new_brightness, expected, "expected {expected} from {action:?} on {brightness}/{max_brightness}, got {new_brightness} (via {got:?})");
                }
            }
        }
    }
}
