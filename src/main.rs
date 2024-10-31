use std::io::{Read, Write};

const PATH_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/brightness";
const PATH_MAX_BRIGHTNESS: &'static str = "/sys/class/backlight/intel_backlight/max_brightness";

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let argv0 = &args[0];

    if args.len() > 2 {
        usage(argv0);
    }

    let action = match args.get(1) {
        Some(action) => {
            if action == "--help" || action == "-h" {
                usage(argv0);
            }

            match str::parse(action) {
                Ok(action) => action,
                Err(e) => {
                    eprintln!("{argv0}: {e}");
                    usage(argv0);
                }
            }
        }
        None => Action::Get,
    };

    if let Err(e) = action.execute() {
        eprintln!("{argv0}: {e}");
        std::process::exit(1);
    }
}

fn usage(argv0: &str) -> ! {
    eprintln!(
        "Usage: {argv0} [ACTION]

  ACTION can be:
    +N           increases brightness by N%
    -N           decreases brightness by N%
    N            set brightness to N%
  if no ACTION is given, displays the current brightness level
  any number N may optionally have a % sign after it"
    );

    std::process::exit(2);
}

impl Action {
    /// Executes an action on the system.
    fn execute(self) -> Result<(), Error> {
        let max_brightness = read_file_as_u32(PATH_MAX_BRIGHTNESS)?;

        if let Some(percentage) =
            self.calculate_new_percentage(max_brightness, || read_file_as_u32(PATH_BRIGHTNESS))?
        {
            let percentage = percentage.clamp(0.0, 1.0);
            let new_value = (f64::from(max_brightness) * percentage).round() as u32;

            write_file_from_u32(PATH_BRIGHTNESS, new_value)?;
        }

        Ok(())
    }

    /// Calculates the new percentage of the maximum brightness `action`, given the `max_brightness` and a function to get the current brightness (to allow for testing).
    /// The `Option<f64>` is the new percentage of `max_brightness` to apply; it should be in the range `0.0..=1.0``.
    fn calculate_new_percentage<F>(
        self,
        max_brightness: u32,
        get_brightness: F,
    ) -> Result<Option<f64>, Error>
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
                let delta = f64::from(change) / 100.0;

                Ok(Some(current - delta))
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
            match action.calculate_new_percentage(max_brightness, || Ok(brightness)) {
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
