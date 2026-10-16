//! Hand-read command line of the harness: two subcommands and three options, which does not
//! justify a parsing dependency. Pure, so that it is tested without a process.

use std::path::PathBuf;

/// The usage text printed with every argument error.
pub const USAGE: &str = "usage:\n  superview-harness probe\n  superview-harness convert <input> <output> [--encoder E] [--bitrate N] [--squeeze]";

/// What the harness was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Report ffmpeg's capabilities and the encoder probe, as JSON.
    Probe,
    /// Convert one file.
    Convert(ConvertArgs),
}

/// The arguments of `convert`.
#[derive(Debug, PartialEq, Eq)]
pub struct ConvertArgs {
    pub input: PathBuf,
    pub output: PathBuf,
    /// "" means the source's codec.
    pub encoder: String,
    /// Bits per second; 0 means the source's.
    pub bitrate: i64,
    pub squeeze: bool,
}

/// Parses the arguments that follow the program name. The error is the message to print.
pub fn parse<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    match args.next().as_deref() {
        None => Err("missing subcommand".into()),
        Some("probe") => match args.next() {
            None => Ok(Command::Probe),
            Some(extra) => Err(format!("unexpected argument for probe: {extra}")),
        },
        Some("convert") => parse_convert(args).map(Command::Convert),
        Some(other) => Err(format!("unknown subcommand: {other}")),
    }
}

fn parse_convert(mut args: impl Iterator<Item = String>) -> Result<ConvertArgs, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut encoder = String::new();
    let mut bitrate = 0_i64;
    let mut squeeze = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--squeeze" => squeeze = true,
            "--encoder" => {
                encoder = args.next().ok_or("--encoder needs a value")?;
                if encoder.starts_with("--") {
                    return Err(format!("--encoder needs a value, not the option {encoder}"));
                }
            }
            "--bitrate" => {
                let value = args.next().ok_or("--bitrate needs a value")?;
                bitrate = value
                    .parse::<i64>()
                    .ok()
                    .filter(|n| *n >= 0)
                    .ok_or_else(|| format!("invalid bitrate (bits per second): {value}"))?;
            }
            option if option.starts_with("--") => {
                return Err(format!("unknown option: {option}"));
            }
            _ => positional.push(arg),
        }
    }
    let mut files = positional.into_iter();
    let (Some(input), Some(output), None) = (files.next(), files.next(), files.next()) else {
        return Err("convert needs exactly <input> and <output>".into());
    };
    Ok(ConvertArgs {
        input: input.into(),
        output: output.into(),
        encoder,
        bitrate,
        squeeze,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_takes_no_argument() {
        assert_eq!(parse(["probe"]), Ok(Command::Probe));
        assert!(parse(["probe", "x"]).is_err());
    }

    #[test]
    fn convert_with_every_option() {
        let parsed = parse([
            "convert",
            "in.mp4",
            "--encoder",
            "libx265",
            "out.mp4",
            "--bitrate",
            "5000000",
            "--squeeze",
        ])
        .unwrap();
        assert_eq!(
            parsed,
            Command::Convert(ConvertArgs {
                input: "in.mp4".into(),
                output: "out.mp4".into(),
                encoder: "libx265".into(),
                bitrate: 5_000_000,
                squeeze: true,
            })
        );
    }

    #[test]
    fn convert_defaults() {
        let Command::Convert(args) = parse(["convert", "a", "b"]).unwrap() else {
            panic!("not a convert");
        };
        assert_eq!(
            (args.encoder.as_str(), args.bitrate, args.squeeze),
            ("", 0, false)
        );
    }

    #[test]
    fn wrong_arguments_are_refused() {
        let none: [&str; 0] = [];
        assert!(parse(none).is_err());
        assert!(parse(["frobnicate"]).is_err());
        assert!(parse(["convert"]).is_err());
        assert!(parse(["convert", "a"]).is_err());
        assert!(parse(["convert", "a", "b", "c"]).is_err());
        assert!(parse(["convert", "a", "b", "--nope"]).is_err());
        assert!(parse(["convert", "a", "b", "--bitrate"]).is_err());
        assert!(parse(["convert", "a", "b", "--bitrate", "abc"]).is_err());
        assert!(parse(["convert", "a", "b", "--bitrate", "-5"]).is_err());
        assert!(parse(["convert", "a", "b", "--encoder"]).is_err());
        assert!(parse(["convert", "a", "b", "--encoder", "--squeeze"]).is_err());
        assert!(parse(["convert", "a", "b", "--bitrate", "--squeeze"]).is_err());
    }
}
