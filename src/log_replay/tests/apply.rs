use super::*;

// -----------------------------------------------------------------------
// ReplayOverlay::apply_to_region() tests
// -----------------------------------------------------------------------

/// Data sector fully inside region → bytes copied, rest unchanged.
#[test]
fn apply_region_data_sector_fully_inside() {
    let mut sectors = HashMap::new();
    sectors.insert(0x1000, vec![0xAAu8; 4096]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut region = vec![0xFFu8; 0x3000]; // [0, 0x3000)
    overlay.apply_to_region(&mut region, 0);

    // Bytes [0x1000..0x2000) should be 0xAA
    assert!(region[0x1000..0x2000].iter().all(|&b| b == 0xAA));
    // Bytes before sector unchanged
    assert!(region[0..0x1000].iter().all(|&b| b == 0xFF));
    // Bytes after sector unchanged
    assert!(region[0x2000..0x3000].iter().all(|&b| b == 0xFF));
}

/// Data sector partially overlapping region start.
#[test]
fn apply_region_data_sector_partial_overlap() {
    let mut sectors = HashMap::new();
    sectors.insert(0x0000, vec![0xBBu8; 4096]); // sector [0, 0x1000)
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut region = vec![0xFFu8; 0x800]; // [0x800, 0x1000)
    overlay.apply_to_region(&mut region, 0x800);

    // Region [0x800..0x1000) → sector bytes at offsets 0x800..0x1000
    assert!(region.iter().all(|&b| b == 0xBB));
}

/// Data sector entirely outside region → no change.
#[test]
fn apply_region_data_sector_outside() {
    let mut sectors = HashMap::new();
    sectors.insert(0x5000, vec![0xCCu8; 4096]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut region = vec![0xFFu8; 0x1000]; // [0, 0x1000)
    overlay.apply_to_region(&mut region, 0);

    // No bytes should change
    assert!(region.iter().all(|&b| b == 0xFF));
}

/// Zero region applies zeros to the region.
#[test]
fn apply_region_zero_fills_zeros() {
    let overlay = ReplayOverlay::from_raw(HashMap::new(), vec![(0x2000, 0x1000)]);

    let mut region = vec![0xFFu8; 0x5000]; // [0, 0x5000)
    overlay.apply_to_region(&mut region, 0);

    // Bytes [0x2000..0x3000) should be zero
    assert!(region[0x2000..0x3000].iter().all(|&b| b == 0));
    // Bytes before zero region unchanged
    assert!(region[0..0x2000].iter().all(|&b| b == 0xFF));
    // Bytes after zero region unchanged
    assert!(region[0x3000..0x5000].iter().all(|&b| b == 0xFF));
}

/// Data sector takes priority over zero region at same offset.
#[test]
fn apply_region_data_priority_over_zero() {
    let mut sectors = HashMap::new();
    sectors.insert(0x1000, vec![0xDDu8; 4096]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![(0x1000, 0x1000)]);

    let mut region = vec![0xFFu8; 0x3000]; // [0, 0x3000)
    overlay.apply_to_region(&mut region, 0);

    // Data sector wins: [0x1000..0x2000) should be 0xDD, NOT zero
    assert!(region[0x1000..0x2000].iter().all(|&b| b == 0xDD));
    // Byte at 0x2000 should still be 0xFF (zero region was [0x1000, 0x1000), ends at 0x2000)
    assert_eq!(region[0x2000], 0xFF);
}

/// Multiple data sectors and one zero region handled correctly.
#[test]
fn apply_region_multiple_overlapping_entries() {
    let mut sectors = HashMap::new();
    sectors.insert(0x1000, vec![0x11u8; 4096]);
    sectors.insert(0x3000, vec![0x22u8; 4096]);
    let overlay = ReplayOverlay::from_raw(
        sectors,
        vec![(0x2000, 0x2000)], // zero region [0x2000, 0x4000)
    );

    let mut region = vec![0xFFu8; 0x5000]; // [0, 0x5000)
    overlay.apply_to_region(&mut region, 0);

    // First data sector: [0x1000..0x2000) = 0x11
    assert!(region[0x1000..0x2000].iter().all(|&b| b == 0x11));
    // Zero region [0x2000..0x3000) = 0 (not covered by data)
    assert!(region[0x2000..0x3000].iter().all(|&b| b == 0));
    // Second data sector: [0x3000..0x4000) = 0x22 (overrides zero)
    assert!(region[0x3000..0x4000].iter().all(|&b| b == 0x22));
    // [0x4000..0x5000) unchanged
    assert!(region[0x4000..0x5000].iter().all(|&b| b == 0xFF));
}

/// Empty overlay → no change to region.
#[test]
fn apply_region_empty_overlay() {
    let overlay = ReplayOverlay::from_raw(HashMap::new(), vec![]);

    let mut region = vec![0xFFu8; 0x1000];
    overlay.apply_to_region(&mut region, 0);

    assert!(region.iter().all(|&b| b == 0xFF));
}

/// Single byte overlap between sector and region.
#[test]
fn apply_region_single_byte_overlap() {
    let mut sectors = HashMap::new();
    sectors.insert(0x1000, vec![0xEEu8; 4096]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut region = vec![0xFFu8; 2]; // [0xFFF, 0x1001)
    overlay.apply_to_region(&mut region, 0xFFF);

    // region[0] = file offset 0xFFF (before sector) → unchanged
    assert_eq!(region[0], 0xFF);
    // region[1] = file offset 0x1000 (start of sector) → gets 0xEE
    assert_eq!(region[1], 0xEE);
}

/// Region offset exactly at sector boundary.
#[test]
fn apply_region_offset_at_sector_boundary() {
    let mut sectors = HashMap::new();
    sectors.insert(0x1000, vec![0x77u8; 4096]);
    let overlay = ReplayOverlay::from_raw(sectors, vec![]);

    let mut region = vec![0xFFu8; 0x1000]; // [0x1000, 0x2000)
    overlay.apply_to_region(&mut region, 0x1000);

    // Entire region should be filled with sector data
    assert!(region.iter().all(|&b| b == 0x77));
}
