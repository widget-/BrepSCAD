use anyhow::Result;
use brepscad_eval::{EvaluationOptions, evaluate_file_with_options, evaluate_with_options};
use brepscad_kernel_truck::TruckKernel;

const DEFAULT_SOURCE: &str = include_str!("../../../assets/sample.scad");
const USAGE: &str = "usage: brepscad [--legacy-centering] [file.scad]";

#[derive(Debug, PartialEq, Eq)]
struct CliArgs {
    path: Option<String>,
    legacy_centering: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseOutcome {
    Help,
    Run(CliArgs),
}

fn main() -> Result<()> {
    let args = match parse_args(std::env::args().skip(1)).map_err(anyhow::Error::msg)? {
        ParseOutcome::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        ParseOutcome::Run(args) => args,
    };

    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering: args.legacy_centering,
    };
    let mut kernel = TruckKernel::new();
    let evaluation = match &args.path {
        Some(path) => evaluate_file_with_options(path, &mut kernel, options)?,
        None => evaluate_with_options(DEFAULT_SOURCE, &mut kernel, options)?,
    };
    for message in &evaluation.messages {
        eprintln!("{message}");
    }
    let meshes = evaluation
        .shapes
        .into_iter()
        .map(|shape| shape.mesh)
        .collect::<Vec<_>>();

    println!("meshes: {}", meshes.len());
    for (index, mesh) in meshes.iter().enumerate() {
        println!(
            "mesh {index}: {} triangles, {} vertices",
            mesh.triangle_count(),
            mesh.positions.len()
        );
    }
    Ok(())
}

fn parse_args<I>(args: I) -> std::result::Result<ParseOutcome, String>
where
    I: IntoIterator<Item = String>,
{
    let mut path = None;
    let mut legacy_centering = false;

    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            "--legacy-centering" => legacy_centering = true,
            value if value.starts_with('-') => {
                return Err(format!("unknown option {value}"));
            }
            value => {
                if path.is_some() {
                    return Err("expected at most one file path".to_owned());
                }
                path = Some(value.to_owned());
            }
        }
    }

    Ok(ParseOutcome::Run(CliArgs {
        path,
        legacy_centering,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> std::result::Result<ParseOutcome, String> {
        parse_args(values.iter().map(|value| value.to_string()))
    }

    #[test]
    fn parses_the_legacy_centering_flag() {
        assert_eq!(
            parse(&["--legacy-centering", "model.scad"]),
            Ok(ParseOutcome::Run(CliArgs {
                path: Some("model.scad".to_owned()),
                legacy_centering: true,
            }))
        );
        assert_eq!(
            parse(&["model.scad", "--legacy-centering"]),
            Ok(ParseOutcome::Run(CliArgs {
                path: Some("model.scad".to_owned()),
                legacy_centering: true,
            }))
        );
    }

    #[test]
    fn parses_the_built_in_sample() {
        assert_eq!(
            parse(&[]),
            Ok(ParseOutcome::Run(CliArgs {
                path: None,
                legacy_centering: false,
            }))
        );
    }

    #[test]
    fn prints_help() {
        assert_eq!(parse(&["-h"]), Ok(ParseOutcome::Help));
        assert_eq!(parse(&["--help"]), Ok(ParseOutcome::Help));
    }

    #[test]
    fn rejects_unknown_options_and_extra_paths() {
        assert!(parse(&["--wat"]).is_err());
        assert!(parse(&["a.scad", "b.scad"]).is_err());
    }
}
