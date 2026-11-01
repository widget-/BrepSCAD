use anyhow::{Result, anyhow};
use brepscad_eval::{
    EvaluationOptions, environment_library_paths, evaluate_file_with_options, evaluate_with_options,
};
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::parse;

const DEFAULT_SOURCE: &str = include_str!("../../../assets/sample.scad");
const USAGE: &str =
    "usage: brepscad [--legacy-centering] [--parse-only] [--library-path DIR] [file.scad]";

#[derive(Debug, PartialEq, Eq)]
struct CliArgs {
    path: Option<String>,
    legacy_centering: bool,
    parse_only: bool,
    library_paths: Vec<String>,
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

    if args.parse_only {
        return parse_only(args.path.as_deref());
    }

    let mut library_paths = environment_library_paths();
    library_paths.extend(args.library_paths.iter().map(std::path::PathBuf::from));
    let options = EvaluationOptions {
        tolerance: 0.01,
        legacy_centering: args.legacy_centering,
        library_paths,
        analytic_scene: false,
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

fn parse_only(path: Option<&str>) -> Result<()> {
    let (source, label) = match path {
        Some(path) => (
            std::fs::read_to_string(path)
                .map_err(|error| anyhow!("source error: failed to read {path}: {error}"))?,
            path.to_owned(),
        ),
        None => (DEFAULT_SOURCE.to_owned(), "<built-in sample>".to_owned()),
    };
    let parsed = parse(&source);
    if parsed.errors().is_empty() {
        println!("parse ok");
        return Ok(());
    }
    let messages = parsed
        .errors()
        .iter()
        .map(|error| format!("{} at offset {}", error.message, error.offset))
        .collect::<Vec<_>>()
        .join("; ");
    Err(anyhow!("parse error: {label}: {messages}"))
}

fn parse_args<I>(args: I) -> std::result::Result<ParseOutcome, String>
where
    I: IntoIterator<Item = String>,
{
    let mut path = None;
    let mut legacy_centering = false;
    let mut parse_only = false;
    let mut library_paths = Vec::new();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(ParseOutcome::Help),
            "--legacy-centering" => legacy_centering = true,
            "--parse-only" => parse_only = true,
            "--library-path" | "-I" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--library-path requires a directory".to_owned())?;
                library_paths.push(value);
            }
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
        parse_only,
        library_paths,
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
                parse_only: false,
                library_paths: Vec::new(),
            }))
        );
        assert_eq!(
            parse(&["model.scad", "--legacy-centering"]),
            Ok(ParseOutcome::Run(CliArgs {
                path: Some("model.scad".to_owned()),
                legacy_centering: true,
                parse_only: false,
                library_paths: Vec::new(),
            }))
        );
    }

    #[test]
    fn parses_the_parse_only_flag() {
        assert_eq!(
            parse(&["--parse-only", "model.scad"]),
            Ok(ParseOutcome::Run(CliArgs {
                path: Some("model.scad".to_owned()),
                legacy_centering: false,
                parse_only: true,
                library_paths: Vec::new(),
            }))
        );
    }

    #[test]
    fn parses_library_paths() {
        assert_eq!(
            parse(&["--library-path", "lib", "-I", "more", "model.scad"]),
            Ok(ParseOutcome::Run(CliArgs {
                path: Some("model.scad".to_owned()),
                legacy_centering: false,
                parse_only: false,
                library_paths: vec!["lib".to_owned(), "more".to_owned()],
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
                parse_only: false,
                library_paths: Vec::new(),
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
