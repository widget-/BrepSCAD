use anyhow::anyhow;
use monstertruck_core::assert_near;
use monstertruck_mesh::*;
use stl::{IntoStlIterator, StlFace, StlReader, StlType};
type Result<T> = std::result::Result<T, errors::Error>;

fn resource_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/stl")
        .join(name)
}

fn read_resource(name: &str) -> Option<Vec<u8>> {
    let path = resource_path(name);
    std::fs::read(path).ok()
}

#[test]
fn stl_oi_test() {
    let mesh = vec![
        StlFace {
            normal: [0.0, 0.0, 1.0],
            vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        },
        StlFace {
            normal: [0.0, 1.0, 1.0],
            vertices: [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]],
        },
        StlFace {
            normal: [1.0, 0.0, 1.0],
            vertices: [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        },
    ];
    let mut ascii = Vec::new();
    stl::write(mesh.iter().cloned(), &mut ascii, StlType::Ascii).unwrap();
    let mut binary = Vec::new();
    stl::write(mesh.iter().cloned(), &mut binary, StlType::Binary).unwrap();
    let amesh = StlReader::<&[u8]>::new(&ascii, StlType::Automatic)
        .unwrap()
        .collect::<Result<Vec<_>>>()
        .unwrap();
    let bmesh = StlReader::<&[u8]>::new(&binary, StlType::Automatic)
        .unwrap()
        .collect::<Result<Vec<_>>>()
        .unwrap();
    assert_eq!(mesh, amesh);
    assert_eq!(mesh, bmesh);
}

#[test]
fn stl_io_test() {
    let Some(ascii_bunny) = read_resource("bunny_ascii.stl") else {
        eprintln!("skipping: bunny_ascii.stl not found");
        return;
    };
    let Some(binary_bunny) = read_resource("bunny_binary.stl") else {
        eprintln!("skipping: bunny_binary.stl not found");
        return;
    };
    let amesh = StlReader::new(ascii_bunny.as_slice(), StlType::Automatic)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let bmesh = StlReader::new(binary_bunny.as_slice(), StlType::Automatic)
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(amesh, bmesh);
    let mut bytes = Vec::<u8>::new();
    stl::write(bmesh.iter().cloned(), &mut bytes, StlType::Binary).unwrap();
    // Binary data is free from notational distortions except for the headers.
    assert_eq!(&bytes[80..], &binary_bunny[80..]);
}

#[test]
fn ascii_stl_header_includes_trailing_space() -> anyhow::Result<()> {
    let mesh = [StlFace {
        normal: [0.0, 0.0, 1.0],
        vertices: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    }];
    let mut bytes = Vec::new();
    stl::write(mesh.iter().cloned(), &mut bytes, StlType::Ascii)?;
    let header = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .ok_or_else(|| anyhow!("ASCII STL output is empty."))?;
    assert_eq!(
        header, b"solid ",
        "the ASCII STL header must end with a space so that lenient readers \
         that require `solid <name>` accept it even when the name is empty.",
    );
    Ok(())
}

#[test]
fn through_polymesh() {
    let Some(binary_bunny) = read_resource("bunny_binary.stl") else {
        eprintln!("skipping: bunny_binary.stl not found");
        return;
    };
    let iter = StlReader::<&[u8]>::new(binary_bunny.as_slice(), StlType::Automatic).unwrap();
    let polymesh: PolygonMesh = iter.map(|face| face.unwrap()).collect();
    let mesh: Vec<StlFace> = polymesh.into_iter().collect();
    let iter = StlReader::<&[u8]>::new(binary_bunny.as_slice(), StlType::Automatic).unwrap();
    for (face0, face1) in mesh.iter().zip(iter) {
        let face1 = face1.unwrap();
        assert_near!(face0.vertices[0][0] as f64, face1.vertices[0][0] as f64);
        assert_near!(face0.vertices[0][1] as f64, face1.vertices[0][1] as f64);
        assert_near!(face0.vertices[0][2] as f64, face1.vertices[0][2] as f64);
        assert_near!(face0.vertices[1][0] as f64, face1.vertices[1][0] as f64);
        assert_near!(face0.vertices[1][1] as f64, face1.vertices[1][1] as f64);
        assert_near!(face0.vertices[1][2] as f64, face1.vertices[1][2] as f64);
        assert_near!(face0.vertices[2][0] as f64, face1.vertices[2][0] as f64);
        assert_near!(face0.vertices[2][1] as f64, face1.vertices[2][1] as f64);
        assert_near!(face0.vertices[2][2] as f64, face1.vertices[2][2] as f64);
        // This is not assert_near, since VTK is single precision.
        assert!(f32::abs(face0.normal[0] - face1.normal[0]) < 5.0e-4);
        assert!(f32::abs(face0.normal[1] - face1.normal[1]) < 5.0e-4);
        assert!(f32::abs(face0.normal[2] - face1.normal[2]) < 5.0e-4);
    }
}
