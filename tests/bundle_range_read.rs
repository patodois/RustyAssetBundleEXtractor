//! `read_at_entry_range` on a bundle built here: one file spread over several LZ4 blocks.

use std::io::Cursor;

use rabex::files::bundlefile::{BundleFileReader, ExtractionConfig};

const BLOCK: usize = 100;
const BLOCKS: usize = 4;

fn content() -> Vec<u8> {
    (0..BLOCK * BLOCKS).map(|i| (i * 7 % 251) as u8).collect()
}

/// A version 7 bundle with one file `data.resS` stored in four LZ4 blocks of 100 bytes.
fn bundle() -> Vec<u8> {
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
    info.extend_from_slice(&1i32.to_be_bytes());
    info.extend_from_slice(&0i64.to_be_bytes());
    info.extend_from_slice(&(content.len() as i64).to_be_bytes());
    info.extend_from_slice(&0u32.to_be_bytes());
    info.extend_from_slice(b"data.resS\0");
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

fn reader() -> BundleFileReader<Cursor<Vec<u8>>> {
    BundleFileReader::from_reader(Cursor::new(bundle()), &ExtractionConfig::default()).unwrap()
}

#[test]
fn ranges_match_the_whole_file() {
    let reader = reader();
    let entry = reader.file("data.resS").unwrap().clone();
    let whole = reader.read_at_entry(&entry).unwrap();
    assert_eq!(whole, content());
    // Inside one block, across a block boundary, across several, from the start, to the end
    for (start, len) in [
        (10, 20),
        (95, 10),
        (50, 300),
        (0, 1),
        (399, 1),
        (0, 400),
        (100, 100),
        (250, 0),
    ] {
        let part = reader.read_at_entry_range(&entry, start, len).unwrap();
        assert_eq!(
            part,
            whole[start as usize..start as usize + len],
            "{start}+{len}"
        );
    }
}

#[test]
fn a_range_past_the_end_is_an_error() {
    let reader = reader();
    let entry = reader.file("data.resS").unwrap().clone();
    assert!(reader.read_at_entry_range(&entry, 390, 11).is_err());
    assert!(reader.read_at_entry_range(&entry, 401, 0).is_err());
    assert!(reader.read_at_entry_range(&entry, u64::MAX, 2).is_err());
}
