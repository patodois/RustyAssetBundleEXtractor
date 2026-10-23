//! Reads files that start and end inside LZ4 blocks of a bundle built here.

use std::io::Cursor;

use rabex::files::bundlefile::{BundleFileReader, ExtractionConfig};

const BLOCK: usize = 100;
const BLOCKS: usize = 4;

fn content() -> Vec<u8> {
    (0..BLOCK * BLOCKS).map(|i| (i * 7 % 251) as u8).collect()
}

/// A version 7 bundle with three files in four LZ4 blocks of 100 bytes: `a` is bytes 0..150,
/// `b` is 150..390 (it starts and ends inside a block) and `c` is 390..400.
fn bundle() -> Vec<u8> {
    bundle_with(&[("a", 0i64, 150i64), ("b", 150, 240), ("c", 390, 10)])
}

/// The same four blocks with the given directory, which a hostile file may fill with any sizes.
fn bundle_with(files: &[(&str, i64, i64)]) -> Vec<u8> {
    let content = content();
    let blocks: Vec<Vec<u8>> = content
        .chunks(BLOCK)
        .map(lz4_flex::block::compress)
        .collect();

    let mut info = vec![0u8; 16];
    info.extend_from_slice(&(BLOCKS as i32).to_be_bytes());
    for b in &blocks {
        info.extend_from_slice(&(BLOCK as u32).to_be_bytes());
        info.extend_from_slice(&(b.len() as u32).to_be_bytes());
        info.extend_from_slice(&2u16.to_be_bytes()); // LZ4
    }
    info.extend_from_slice(&(files.len() as i32).to_be_bytes());
    for &(name, offset, size) in files {
        info.extend_from_slice(&offset.to_be_bytes());
        info.extend_from_slice(&size.to_be_bytes());
        info.extend_from_slice(&0u32.to_be_bytes());
        info.extend_from_slice(name.as_bytes());
        info.push(0);
    }
    let compressed_info = lz4_flex::block::compress(&info);

    let mut out = Vec::new();
    out.extend_from_slice(b"UnityFS\0");
    out.extend_from_slice(&7u32.to_be_bytes());
    out.extend_from_slice(b"5.x.x\0");
    out.extend_from_slice(b"2022.3.18f1\0");
    let size_at = out.len();
    out.extend_from_slice(&0i64.to_be_bytes());
    out.extend_from_slice(&(compressed_info.len() as u32).to_be_bytes());
    out.extend_from_slice(&(info.len() as u32).to_be_bytes());
    out.extend_from_slice(&0x42u32.to_be_bytes()); // LZ4, block info combined with the directory
    while out.len() % 16 != 0 {
        out.push(0);
    }
    out.extend_from_slice(&compressed_info);
    for b in &blocks {
        out.extend_from_slice(b);
    }
    let total = out.len() as i64;
    out[size_at..size_at + 8].copy_from_slice(&total.to_be_bytes());
    out
}

#[test]
fn files_that_start_and_end_inside_blocks_are_read_whole() {
    let reader =
        BundleFileReader::from_reader(Cursor::new(bundle()), &ExtractionConfig::default()).unwrap();
    let content = content();
    assert_eq!(reader.read_at("a").unwrap().unwrap(), content[..150]);
    assert_eq!(reader.read_at("b").unwrap().unwrap(), content[150..390]);
    assert_eq!(reader.read_at("c").unwrap().unwrap(), content[390..]);
}

/// The directory of a bundle is data from the file: a size of 2^60 must not become a reservation
/// that aborts the process.
#[test]
fn a_hostile_directory_size_does_not_abort() {
    let bytes = bundle_with(&[("big", 0, 1 << 60)]);
    let reader =
        BundleFileReader::from_reader(Cursor::new(bytes), &ExtractionConfig::default()).unwrap();
    // The blocks end long before the claimed size, so what the bundle holds comes back.
    assert_eq!(reader.read_at("big").unwrap().unwrap(), content());
}
