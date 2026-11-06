use anyhow::{Result, anyhow};
use brepscad_eval::{
    EvaluationOptions, environment_library_paths, evaluate_file_with_options, evaluate_with_options,
};
use brepscad_kernel_api::Mesh;
use brepscad_kernel_truck::TruckKernel;
use brepscad_syntax::parse;
use std::collections::{HashMap, HashSet};

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
    let mut combined = Mesh {
        positions: Vec::new(),
        normals: Vec::new(),
        indices: Vec::new(),
    };
    for shape in evaluation.shapes {
        let offset = combined.positions.len() as u32;
        combined.positions.extend_from_slice(&shape.mesh.positions);
        combined
            .indices
            .extend(shape.mesh.indices.iter().map(|index| index + offset));
    }
    let meshes = if combined.positions.is_empty() {
        Vec::new()
    } else {
        vec![combined]
    };

    #[cfg(feature = "provenance")]
    monstertruck_core::provenance::dump_if_requested();

    println!("meshes: {}", meshes.len());
    for (index, mesh) in meshes.iter().enumerate() {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for position in &mesh.positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        let volume = mesh
            .indices
            .chunks_exact(3)
            .map(|triangle| {
                let a = mesh.positions[triangle[0] as usize];
                let b = mesh.positions[triangle[1] as usize];
                let c = mesh.positions[triangle[2] as usize];
                a[0] * (b[1] * c[2] - b[2] * c[1])
                    + a[1] * (b[2] * c[0] - b[0] * c[2])
                    + a[2] * (b[0] * c[1] - b[1] * c[0])
            })
            .sum::<f64>()
            / 6.0;
        let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
        for triangle in mesh.indices.chunks_exact(3) {
            for (start, end) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                *edges.entry(edge).or_default() += 1;
            }
        }
        let boundary_edges = edges.values().filter(|count| **count == 1).count();
        let non_manifold_edges = edges.values().filter(|count| **count > 2).count();
        let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
        let mut welded_positions: HashMap<u32, [f64; 3]> = HashMap::new();
        let mut welded_edges: HashMap<(u32, u32), usize> = HashMap::new();
        for triangle in mesh.indices.chunks_exact(3) {
            let mut welded_triangle = [0u32; 3];
            for (slot, index) in triangle.iter().enumerate() {
                let position = mesh.positions[*index as usize];
                let key = (
                    (position[0] * 1.0e5).round() as i64,
                    (position[1] * 1.0e5).round() as i64,
                    (position[2] * 1.0e5).round() as i64,
                );
                let next = welded.len() as u32;
                let index = *welded.entry(key).or_insert(next);
                welded_positions.entry(index).or_insert(position);
                welded_triangle[slot] = index;
            }
            for (start, end) in [
                (welded_triangle[0], welded_triangle[1]),
                (welded_triangle[1], welded_triangle[2]),
                (welded_triangle[2], welded_triangle[0]),
            ] {
                let edge = if start < end {
                    (start, end)
                } else {
                    (end, start)
                };
                *welded_edges.entry(edge).or_default() += 1;
            }
        }
        let welded_boundary = welded_edges.values().filter(|count| **count == 1).count();
        let welded_non_manifold = welded_edges.values().filter(|count| **count > 2).count();
        println!(
            "mesh {index}: {} triangles, {} vertices, bounds {min:?}..{max:?}, volume {volume}, boundary edges {boundary_edges} ({welded_boundary} welded), non-manifold edges {non_manifold_edges} ({welded_non_manifold} welded)",
            mesh.triangle_count(),
            mesh.positions.len()
        );
        let mut boundary_adj: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&(a, b), count) in &welded_edges {
            if *count == 1 {
                boundary_adj.entry(a).or_default().push(b);
                boundary_adj.entry(b).or_default().push(a);
            }
        }
        let mut visited = HashSet::new();
        let mut loops = Vec::new();
        for &start in boundary_adj.keys() {
            if !visited.insert(start) {
                continue;
            }
            let mut stack = vec![start];
            let mut loop_min = [f64::INFINITY; 3];
            let mut loop_max = [f64::NEG_INFINITY; 3];
            let mut area_vec = [0.0f64; 3];
            let mut count = 0;
            while let Some(vertex) = stack.pop() {
                count += 1;
                if let Some(position) = welded_positions.get(&vertex) {
                    for axis in 0..3 {
                        loop_min[axis] = loop_min[axis].min(position[axis]);
                        loop_max[axis] = loop_max[axis].max(position[axis]);
                    }
                }
                for &neighbour in boundary_adj.get(&vertex).into_iter().flatten() {
                    if vertex < neighbour
                        && let (Some(a), Some(b)) = (
                            welded_positions.get(&vertex),
                            welded_positions.get(&neighbour),
                        )
                    {
                        area_vec[0] += a[1] * b[2] - a[2] * b[1];
                        area_vec[1] += a[2] * b[0] - a[0] * b[2];
                        area_vec[2] += a[0] * b[1] - a[1] * b[0];
                    }
                    if visited.insert(neighbour) {
                        stack.push(neighbour);
                    }
                }
            }
            let area = 0.5
                * (area_vec[0] * area_vec[0]
                    + area_vec[1] * area_vec[1]
                    + area_vec[2] * area_vec[2])
                    .sqrt();
            loops.push((count, loop_min, loop_max, area, area_vec));
        }
        loops.sort_by_key(|(count, _, _, _, _)| std::cmp::Reverse(*count));
        for (loop_index, (count, loop_min, loop_max, area, area_vec)) in
            loops.iter().take(10).enumerate()
        {
            let normal = if *area > 0.0 {
                [
                    area_vec[0] / (2.0 * area),
                    area_vec[1] / (2.0 * area),
                    area_vec[2] / (2.0 * area),
                ]
            } else {
                [0.0; 3]
            };
            println!(
                "  boundary loop {loop_index}: {count} vertices, bounds {loop_min:?}..{loop_max:?}, area {area}, normal {normal:?}"
            );
        }
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
