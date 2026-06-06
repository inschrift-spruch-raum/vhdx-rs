//! validation module facade.

use crate::constants::{BAT_REGION_GUID, METADATA_REGION_GUID, MIB};
use crate::error::{Error, Result, SignaturePosition};
use crate::file::{is_known_metadata_guid, is_known_region_guid};
use crate::header::{Header, HeaderStructure};
use crate::types::{Guid, StandardItems};
use crate::{bat::PayloadBlockState, bat::SectorBitmapState};

mod bat;
mod core;
mod header;
mod log;
mod metadata;
mod parent;
mod region;

pub use self::core::{SpecValidator, ValidationIssue};

#[cfg(test)]
mod tests;
