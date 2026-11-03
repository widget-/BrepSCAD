use monstertruck_topology::attributes::AttributeValue;
use monstertruck_topology::*;

/// Helper: build a unit-box solid with empty geometry.
fn make_unit_box() -> Solid<(), (), ()> {
    let v = Vertex::from_points([(); 8]);
    let edge = [
        Edge::new(&v[0], &v[1], ()),
        Edge::new(&v[1], &v[2], ()),
        Edge::new(&v[2], &v[3], ()),
        Edge::new(&v[3], &v[0], ()),
        Edge::new(&v[0], &v[4], ()),
        Edge::new(&v[1], &v[5], ()),
        Edge::new(&v[2], &v[6], ()),
        Edge::new(&v[3], &v[7], ()),
        Edge::new(&v[4], &v[5], ()),
        Edge::new(&v[5], &v[6], ()),
        Edge::new(&v[6], &v[7], ()),
        Edge::new(&v[7], &v[4], ()),
    ];

    let face0 = Face::new(vec![wire![&edge[0], &edge[1], &edge[2], &edge[3]]], ());
    let face1 = Face::new(
        vec![wire![
            &edge[4],
            &edge[8],
            &edge[5].inverse(),
            &edge[0].inverse(),
        ]],
        (),
    );
    let face2 = Face::new(
        vec![wire![
            &edge[5],
            &edge[9],
            &edge[6].inverse(),
            &edge[1].inverse(),
        ]],
        (),
    );
    let face3 = Face::new(
        vec![wire![
            &edge[6],
            &edge[10],
            &edge[7].inverse(),
            &edge[2].inverse(),
        ]],
        (),
    );
    let face4 = Face::new(
        vec![wire![
            &edge[7],
            &edge[11],
            &edge[4].inverse(),
            &edge[3].inverse(),
        ]],
        (),
    );
    let face5 = Face::new(
        vec![wire![
            &edge[11].inverse(),
            &edge[10].inverse(),
            &edge[9].inverse(),
            &edge[8].inverse(),
        ]],
        (),
    );

    let shell: Shell<(), (), ()> = vec![face0, face1, face2, face3, face4, face5].into();
    Solid::new(vec![shell])
}

#[test]
fn equivalent_solids_have_same_topology_hash() {
    let solid_a = make_unit_box();
    let solid_b = make_unit_box();
    assert_eq!(solid_a.topology_hash(), solid_b.topology_hash());
}

#[test]
fn topology_attribute_hash_tracks_selection_attrs() {
    let mut solid = make_unit_box();
    let before = solid.topology_attribute_hash();
    let face_id = solid.face_iter().next().unwrap().stable_id();
    solid
        .face_attributes_mut()
        .set("sel_face/test", face_id, AttributeValue::Bool(true));
    let after = solid.topology_attribute_hash();
    assert_ne!(before, after);
}

#[test]
fn topology_hash_ignores_attribute_only_changes() {
    let mut solid = make_unit_box();
    let before = solid.topology_hash();
    let face_id = solid.face_iter().next().unwrap().stable_id();
    solid
        .face_attributes_mut()
        .set("sel_face/test", face_id, AttributeValue::Bool(true));
    let after = solid.topology_hash();
    assert_eq!(before, after);
}

#[test]
fn topology_hash_stable_across_calls() {
    let solid = make_unit_box();
    let h1 = solid.topology_hash();
    let h2 = solid.topology_hash();
    assert_eq!(h1, h2);
}

#[test]
fn topology_attribute_hash_stable_across_calls() {
    let solid = make_unit_box();
    let h1 = solid.topology_attribute_hash();
    let h2 = solid.topology_attribute_hash();
    assert_eq!(h1, h2);
}

#[test]
fn content_hash_deterministic() {
    let solid_a = make_unit_box();
    let solid_b = make_unit_box();
    assert_eq!(solid_a.content_hash(), solid_b.content_hash());
}

#[test]
fn content_hash_changes_when_selection_attributes_change() {
    let mut solid = make_unit_box();
    let before = solid.content_hash();
    let face_id = solid.face_iter().next().unwrap().stable_id();
    solid
        .face_attributes_mut()
        .set("sel_face/test", face_id, AttributeValue::Bool(true));
    let after = solid.content_hash();
    assert_ne!(before, after);
}

// ---------------------------------------------------------------------------
// Round-trip and non-goal regression tests.
// ---------------------------------------------------------------------------

#[test]
fn compress_extract_preserves_topology_hash() {
    let solid = make_unit_box();
    let hash_before = solid.topology_hash();
    let compressed = solid.compress();
    let extracted = Solid::extract(compressed).unwrap();
    let hash_after = extracted.topology_hash();
    assert_eq!(hash_before, hash_after);
}

#[test]
fn compress_extract_preserves_topology_attribute_hash() {
    let mut solid = make_unit_box();
    let face_id = solid.face_iter().next().unwrap().stable_id();
    solid
        .face_attributes_mut()
        .set("sel_face/test", face_id, AttributeValue::Bool(true));
    let hash_before = solid.topology_attribute_hash();
    let compressed = solid.compress();
    let extracted = Solid::extract(compressed).unwrap();
    let hash_after = extracted.topology_attribute_hash();
    assert_eq!(hash_before, hash_after);
}

#[test]
fn same_topology_different_attrs_same_topology_hash_different_attr_hash() {
    let mut a = make_unit_box();
    let b = make_unit_box();
    let face_id = a.face_iter().next().unwrap().stable_id();
    a.face_attributes_mut()
        .set("sel_face/test", face_id, AttributeValue::Bool(true));
    // b has no attributes.
    assert_eq!(a.topology_hash(), b.topology_hash());
    assert_ne!(a.topology_attribute_hash(), b.topology_attribute_hash());
}

#[test]
fn hash_independent_of_sparse_attribute_insertion_order() {
    let mut a = make_unit_box();
    let mut b = make_unit_box();
    let id1 = StableId::new(1);
    let id2 = StableId::new(2);
    // Insert in opposite order.
    a.face_attributes_mut()
        .set("x", id1, AttributeValue::Bool(true));
    a.face_attributes_mut()
        .set("x", id2, AttributeValue::Bool(false));
    b.face_attributes_mut()
        .set("x", id2, AttributeValue::Bool(false));
    b.face_attributes_mut()
        .set("x", id1, AttributeValue::Bool(true));
    assert_eq!(a.topology_attribute_hash(), b.topology_attribute_hash());
}

#[test]
fn compress_extract_preserves_content_hash() {
    let solid = make_unit_box();
    let hash_before = solid.content_hash();
    let compressed = solid.compress();
    let extracted = Solid::extract(compressed).unwrap();
    let hash_after = extracted.content_hash();
    assert_eq!(hash_before, hash_after);
}
