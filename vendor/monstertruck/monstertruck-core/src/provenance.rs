//! Lineage of geometry through the operations that produced it.
//!
//! Compiled only under the `provenance` feature. Without it this module does not
//! exist, so a release build carries no field, no allocation and no call site.
//! It exists to answer "which operation made this face look like that", which is
//! otherwise guesswork once several booleans have been chained.
//!
//! Entities are identified by the address of the `Arc` that holds their
//! geometry. That is process-local and not stable across runs, which is
//! acceptable for a debugging session and is the same key the crate's own
//! `Hash` impls already use.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Identity of a geometry object, as produced by [`key_of`].
pub type Key = usize;

/// One operation that touched a geometry object.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The operation, e.g. `"and"`, `"divide"`, `"sew"`.
    pub operation: &'static str,
    /// What happened, in the operation's own words.
    pub detail: String,
    /// The other geometry objects involved.
    pub inputs: Vec<Key>,
}

thread_local! {
    static LOG: RefCell<HashMap<Key, Vec<Entry>>> = RefCell::new(HashMap::new());
    static LABELS: RefCell<HashMap<Key, String>> = RefCell::new(HashMap::new());
}

/// Turns the address of an `Arc` into a key.
///
/// Callers pass `Arc::as_ptr(&entity.surface_or_curve) as *const ()`.
pub fn key_of(pointer: *const ()) -> Key { pointer as Key }

/// Names a key so a dump reads as `face#3` rather than a bare address.
pub fn label(key: Key, name: &str) {
    LABELS.with_borrow_mut(|labels| {
        labels.insert(key, name.to_string());
    });
}

/// Records that `operation` produced or modified `key` from `inputs`.
pub fn record(key: Key, operation: &'static str, detail: impl Into<String>, inputs: &[Key]) {
    LOG.with_borrow_mut(|log| {
        log.entry(key).or_default().push(Entry {
            operation,
            detail: detail.into(),
            inputs: inputs.to_vec(),
        });
    });
}

fn name_of(key: Key) -> String {
    LABELS.with_borrow(|labels| match labels.get(&key) {
        Some(name) => format!("{name}#{key:x}"),
        None => format!("#{key:x}"),
    })
}

/// Renders every recorded lineage, one block per object, oldest entry first.
///
/// Inputs are named but not expanded, so a long chain stays readable and each
/// step can be followed by looking up the block it names.
pub fn dump() -> String {
    LOG.with_borrow(|log| {
        let mut keys: Vec<Key> = log.keys().copied().collect();
        keys.sort_unstable();
        let mut out = String::new();
        for key in keys {
            let _ = writeln!(out, "{}", name_of(key));
            for entry in &log[&key] {
                let inputs = match entry.inputs.is_empty() {
                    true => String::new(),
                    false => {
                        let names: Vec<String> = entry.inputs.iter().map(|k| name_of(*k)).collect();
                        format!(" <- {}", names.join(", "))
                    }
                };
                let _ = writeln!(
                    out,
                    "    {}: {}{inputs}",
                    entry.operation, entry.detail
                );
            }
        }
        out
    })
}

/// Prints [`dump`] to stderr when `BREPSCAD_PROVENANCE` is set.
///
/// An environment variable rather than a second feature: the recording cost is
/// what has to be compiled out, and this only decides when to print.
pub fn dump_if_requested() {
    if std::env::var_os("BREPSCAD_PROVENANCE").is_some() {
        eprint!("{}", dump());
    }
}
