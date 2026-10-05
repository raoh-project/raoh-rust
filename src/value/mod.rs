//! The values the value model of the Raoh Specification has and Rust's standard library does not:
//! floats compared as the model compares them, decimals that keep their scale, temporal values,
//! UUIDs and URIs.

pub(crate) mod decimal;
pub(crate) mod float;
pub(crate) mod same;
pub(crate) mod temporal;
pub(crate) mod uri;
pub(crate) mod uuid;
