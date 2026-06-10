// Section display functions
// ---------------------------------------------------------------------------

pub(super) fn show_header(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let header = sections.header()?;

    // -- File Type Identifier --
    let ft = header.file_type();
    println!("=== File Type Identifier ===");
    println!(
        "  Signature: {:?}",
        std::str::from_utf8(ft.signature()).unwrap_or("<binary>")
    );
    println!("  Creator:    {} bytes", ft.creator().len());

    // -- Current Header --
    println!();
    println!("=== Current Header (index 0) ===");
    match header.header(0) {
        Ok(h) => {
            println!(
                "  Signature:       {:?}",
                std::str::from_utf8(h.signature()).unwrap_or("<binary>")
            );
            println!("  Sequence Number: {}", h.sequence_number());
            println!("  CRC-32C:         {}", h.checksum());
            println!("  Version:         {} (expected 1)", h.version());
            println!("  Log Version:     {} (expected 0)", h.log_version());
            println!("  Log Offset:      {}", h.log_offset());
            println!("  Log Length:      {}", h.log_length());
            println!("  File Write GUID: {}", h.file_write_guid());
            println!("  Data Write GUID: {}", h.data_write_guid());
            println!("  Log GUID:        {}", h.log_guid());
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }

    // -- Header 1 --
    println!();
    println!("=== Header 1 ===");
    print_header_summary(&header, 1);

    // -- Header 2 --
    println!();
    println!("=== Header 2 ===");
    print_header_summary(&header, 2);

    // -- Region Table 1 --
    println!();
    println!("=== Region Table 1 ===");
    print_region_table(&header, 1);

    // -- Region Table 2 --
    println!();
    println!("=== Region Table 2 ===");
    print_region_table(&header, 2);

    Ok(())
}

fn print_header_summary(header: &vhdx::section::Header<'_>, index: usize) {
    match header.header(index) {
        Ok(h) => {
            println!(
                "  Signature:       {:?}",
                std::str::from_utf8(h.signature()).unwrap_or("<binary>")
            );
            println!("  Sequence Number: {}", h.sequence_number());
            println!("  CRC-32C:         {}", h.checksum());
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }
}

fn print_region_table(header: &vhdx::section::Header<'_>, index: usize) {
    match header.region_table(index) {
        Ok(rt) => {
            let hdr = rt.header();
            println!(
                "  Signature:    {:?}",
                std::str::from_utf8(hdr.signature()).unwrap_or("<binary>")
            );
            println!("  Entry Count:  {}", hdr.entry_count());
            println!("  CRC-32C:      {}", hdr.checksum());
            for (i, entry) in rt.entries().enumerate() {
                println!("  Entry [{i}]:");
                println!("    GUID:     {}", entry.guid());
                println!("    Offset:   {}", entry.file_offset());
                println!("    Length:   {}", entry.length());
                println!("    Required: {}", entry.required());
            }
        }
        Err(ref e) => println!("  [Error: {e}]"),
    }
}

pub(super) fn show_bat(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    use vhdx::section::BatState;

    let bat = sections.bat()?;

    println!("=== Block Allocation Table (BAT) ===");

    let mut total = 0u64;
    let mut displayed = 0u64;
    for (i, entry) in bat.entries().enumerate() {
        total = i as u64 + 1;
        if displayed < 20 {
            if displayed == 0 {
                println!();
                println!("  First 20 entries:");
            }
            let state_str = match entry.state()? {
                BatState::Payload(s) => format!("Payload({s:?})"),
                BatState::SectorBitmap(s) => format!("SectorBitmap({s:?})"),
            };
            println!(
                "  [{i:>4}] state={state_str}, offset_mb={}",
                entry.file_offset_mb()
            );
            displayed += 1;
        }
    }

    println!("  Total Entries: {total}");
    if total > 20 {
        println!("  ... ({} entries omitted)", total - 20);
    }

    Ok(())
}

pub(super) fn show_metadata(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let meta = sections.metadata()?;
    let table = meta.table();
    let items = meta.items();

    println!("=== Metadata ===");
    if table.header().signature() == b"metadata" {
        println!("  Signature: metadata (valid)");
    } else {
        println!(
            "  Signature: [invalid: {:?}]",
            std::str::from_utf8(table.header().signature()).unwrap_or("<binary>")
        );
    }
    println!("  Entry Count: {}", table.header().entry_count());
    println!();

    // Show known metadata items
    println!("  Known Metadata Items:");
    if let Ok(fp) = items.file_parameters() {
        println!("    FileParameters:");
        println!("      Block Size:         {} bytes", fp.block_size());
        println!("      Has Parent:         {}", fp.has_parent());
        println!("      Leave Block Alloc:  {}", fp.leave_block_allocated());
    } else {
        println!("    FileParameters: not found");
    }
    if let Ok(size) = items.virtual_disk_size() {
        println!("    VirtualDiskSize:     {size} bytes");
    } else {
        println!("    VirtualDiskSize: not found");
    }
    if let Ok(id) = items.virtual_disk_id() {
        println!("    VirtualDiskId:       {id}");
    } else {
        println!("    VirtualDiskId: not found");
    }
    if let Ok(lss) = items.logical_sector_size() {
        println!("    LogicalSectorSize:   {lss}");
    } else {
        println!("    LogicalSectorSize: not found");
    }
    if let Ok(pss) = items.physical_sector_size() {
        println!("    PhysicalSectorSize:  {pss}");
    } else {
        println!("    PhysicalSectorSize: not found");
    }
    if let Ok(pl) = items.parent_locator() {
        println!("    ParentLocator:");
        println!("      Key-Value Entries: {}", pl.header().key_value_count());
        for (i, kv) in pl.entries().enumerate() {
            let key = kv
                .key(pl.key_value_data())
                .unwrap_or_else(|_| "<decode error>".into());
            let val = kv
                .value(pl.key_value_data())
                .unwrap_or_else(|_| "<decode error>".into());
            println!("      [{i}] \"{key}\" = \"{val}\"");
        }
    } else {
        println!("    ParentLocator: not found");
    }

    // Show all table entries (including unknown GUIDs)
    println!();
    println!("  Raw Table Entries:");
    for (i, entry) in table.entries().enumerate() {
        println!(
            "    [{i}] GUID={}, offset={}, length={}, flags={:#010x}",
            entry.item_id(),
            entry.offset(),
            entry.length(),
            entry.flags_bits(),
        );
    }

    Ok(())
}

pub(super) fn show_log(sections: &vhdx::section::Sections<'_>) -> vhdx::Result<()> {
    let log = sections.log()?;

    println!("=== Log ===");
    let entries: Vec<_> = log.entries().collect();
    println!("  Total Entries: {}", entries.len());
    println!();
    let display_count = entries.len().min(10);
    for (i, entry) in entries.iter().take(display_count).enumerate() {
        let hdr = entry.header();
        println!("  Entry [{i}]:");
        println!("    Sequence Number:  {}", hdr.sequence_number());
        println!("    Descriptor Count: {}", hdr.descriptor_count());
        println!("    Entry Length:     {} bytes", hdr.entry_length());
        println!("    Tail:             {}", hdr.tail());
        println!("    CRC-32C:          {}", hdr.checksum());
    }
    if entries.len() > 10 {
        println!("  ... ({} entries omitted)", entries.len() - 10);
    }

    Ok(())
}
