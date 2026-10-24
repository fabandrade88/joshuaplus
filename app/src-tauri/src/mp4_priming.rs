//! AAC priming in exported MP4 clips, signalled the way Apple's and FFmpeg's
//! muxers do it.
//!
//! An AAC encoder starts with some silence of its own (2112 samples for
//! Apple's). The studio's muxer writes an edit list that skips it, which
//! FFmpeg (YouTube, Instagram, VLC) honours. Apple's players only trust that
//! edit list when the track also declares an AAC `roll` sample group; without
//! it they skip the priming twice and the sound runs 44 ms early. This adds
//! the group to a finished file. The index (`moov`) is the last box of these
//! files, so it is rewritten in place and the media data never moves.

use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// A clip's index is small; anything bigger is not a file we wrote.
const MAX_MOOV: u64 = 64 * 1024 * 1024;

/// Adds the `roll` group to the AAC track of the MP4 at `path`. Returns
/// whether the file changed: files that are not shaped as expected (index
/// not last, no AAC track with an edit list, group already present) are left
/// untouched.
pub fn mark_aac_priming(path: &Path) -> io::Result<bool> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let mut at = 0u64;
    let mut last = None;
    while at + 8 <= len {
        f.seek(SeekFrom::Start(at))?;
        let mut h = [0u8; 16];
        f.read_exact(&mut h[..8])?;
        let mut size = u64::from(u32::from_be_bytes([h[0], h[1], h[2], h[3]]));
        if size == 1 {
            f.read_exact(&mut h[8..16])?;
            size = u64::from_be_bytes([h[8], h[9], h[10], h[11], h[12], h[13], h[14], h[15]]);
        } else if size == 0 {
            size = len - at;
        }
        if size < 8 || size > len - at {
            return Ok(false);
        }
        last = Some((at, size, [h[4], h[5], h[6], h[7]]));
        at += size;
    }
    let Some((moov_at, moov_size, kind)) = last else { return Ok(false) };
    if &kind != b"moov" || at != len || moov_size > MAX_MOOV {
        return Ok(false);
    }
    let mut moov = vec![0u8; moov_size as usize];
    f.seek(SeekFrom::Start(moov_at))?;
    f.read_exact(&mut moov)?;
    let Some(updated) = with_roll_group(&moov) else { return Ok(false) };
    f.seek(SeekFrom::Start(moov_at))?;
    f.write_all(&updated)?;
    f.set_len(moov_at + updated.len() as u64)?;
    f.sync_all()?;
    Ok(true)
}

/// Child boxes of `buf[from..to]`: (start, size, type). `None` on anything
/// malformed or on 64-bit sizes (an index never needs them).
fn children(buf: &[u8], from: usize, to: usize) -> Option<Vec<(usize, usize, [u8; 4])>> {
    let mut out = Vec::new();
    let mut at = from;
    while at < to {
        let h = buf.get(at..at + 8)?;
        let size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as usize;
        if size < 8 || size > to - at {
            return None;
        }
        out.push((at, size, [h[4], h[5], h[6], h[7]]));
        at += size;
    }
    Some(out)
}

fn child(list: &[(usize, usize, [u8; 4])], kind: &[u8; 4]) -> Option<(usize, usize)> {
    list.iter().find(|c| &c.2 == kind).map(|c| (c.0, c.1))
}

fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(buf.get(at..at + 4)?.try_into().ok()?))
}

/// The `moov` box with a `roll` group (distance −1, one AAC frame) covering
/// every sample of its AAC track; `None` when there is nothing to do.
fn with_roll_group(moov: &[u8]) -> Option<Vec<u8>> {
    for (trak, trak_size, kind) in children(moov, 8, moov.len())? {
        if &kind != b"trak" {
            continue;
        }
        let in_trak = children(moov, trak + 8, trak + trak_size)?;
        // Only a track whose priming is skipped by an edit list.
        let Some((edts, edts_size)) = child(&in_trak, b"edts") else { continue };
        let (elst, _) = child(&children(moov, edts + 8, edts + edts_size)?, b"elst")?;
        let skipped = match moov.get(elst + 8)? {
            0 => i64::from(read_u32(moov, elst + 20)? as i32),
            _ => i64::from_be_bytes(moov.get(elst + 24..elst + 32)?.try_into().ok()?),
        };
        let (mdia, mdia_size) = child(&in_trak, b"mdia")?;
        let in_mdia = children(moov, mdia + 8, mdia + mdia_size)?;
        let (hdlr, _) = child(&in_mdia, b"hdlr")?;
        if moov.get(hdlr + 16..hdlr + 20)? != b"soun" || skipped <= 0 {
            continue;
        }
        let (minf, minf_size) = child(&in_mdia, b"minf")?;
        let (stbl, stbl_size) = child(&children(moov, minf + 8, minf + minf_size)?, b"stbl")?;
        let in_stbl = children(moov, stbl + 8, stbl + stbl_size)?;
        let (stsd, _) = child(&in_stbl, b"stsd")?;
        if moov.get(stsd + 20..stsd + 24)? != b"mp4a" || child(&in_stbl, b"sgpd").is_some() {
            return None;
        }
        let (stsz, _) = child(&in_stbl, b"stsz")?;
        let samples = read_u32(moov, stsz + 16)?;

        let mut group = Vec::with_capacity(54);
        // sgpd v1: grouping 'roll', default length 2, one entry: −1.
        group.extend_from_slice(&26u32.to_be_bytes());
        group.extend_from_slice(b"sgpd\x01\0\0\0roll");
        group.extend_from_slice(&2u32.to_be_bytes());
        group.extend_from_slice(&1u32.to_be_bytes());
        group.extend_from_slice(&(-1i16).to_be_bytes());
        // sbgp v0: every sample belongs to entry 1.
        group.extend_from_slice(&28u32.to_be_bytes());
        group.extend_from_slice(b"sbgp\0\0\0\0roll");
        group.extend_from_slice(&1u32.to_be_bytes());
        group.extend_from_slice(&samples.to_be_bytes());
        group.extend_from_slice(&1u32.to_be_bytes());

        let end = stbl + stbl_size;
        let mut out = Vec::with_capacity(moov.len() + group.len());
        out.extend_from_slice(&moov[..end]);
        out.extend_from_slice(&group);
        out.extend_from_slice(&moov[end..]);
        for at in [0, trak, mdia, minf, stbl] {
            let size = read_u32(&out, at)? + group.len() as u32;
            out[at..at + 4].copy_from_slice(&size.to_be_bytes());
        }
        return Some(out);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.extend_from_slice(body);
        v
    }

    fn track(handler: &[u8; 4], codec: &[u8; 4], skip: i32) -> Vec<u8> {
        let mut elst = vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0x10, 0];
        elst.extend_from_slice(&skip.to_be_bytes());
        elst.extend_from_slice(&[0, 1, 0, 0]);
        let mut hdlr = vec![0; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0; 12]);
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(bx(codec, &[0; 8]));
        let mut stsz = vec![0; 8];
        stsz.extend_from_slice(&284u32.to_be_bytes());
        let stbl = [bx(b"stsd", &stsd), bx(b"stsz", &stsz)].concat();
        let minf = bx(b"minf", &bx(b"stbl", &stbl));
        let mdia = bx(b"mdia", &[bx(b"hdlr", &hdlr), minf].concat());
        bx(b"trak", &[bx(b"edts", &bx(b"elst", &elst)), mdia].concat())
    }

    fn clip(moov_tracks: &[Vec<u8>]) -> Vec<u8> {
        let mut file = bx(b"ftyp", b"isom");
        file.extend(bx(b"mdat", &[7; 100]));
        file.extend(bx(b"moov", &moov_tracks.concat()));
        file
    }

    fn write(dir: &tempfile::TempDir, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.path().join("clip.mp4.part");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn the_aac_track_gets_a_roll_group_and_the_media_stays_put() {
        let dir = tempfile::tempdir().unwrap();
        let mut video = track(b"vide", b"avc1", 0);
        // A video track without an edit list must not stop the search.
        video = bx(b"trak", &video[8 + 36..]); // drop the edts box
        let original = clip(&[video, track(b"soun", b"mp4a", 2112)]);
        let p = write(&dir, &original);
        assert!(mark_aac_priming(&p).unwrap());
        let out = std::fs::read(&p).unwrap();
        assert_eq!(out.len(), original.len() + 54);
        let moov_at = 12 + 108; // after ftyp and mdat
        assert_eq!(&out[..moov_at], &original[..moov_at], "ftyp and mdat untouched");
        // Every enclosing size grew by the new boxes; the tree still parses.
        let top = children(&out, 0, out.len()).unwrap();
        assert_eq!(top.last().unwrap().2, *b"moov");
        let roll = out.windows(4).position(|w| w == b"sbgp").unwrap();
        assert_eq!(read_u32(&out, roll + 8).unwrap(), u32::from_be_bytes(*b"roll"));
        assert_eq!(read_u32(&out, roll + 16).unwrap(), 284);
        // Running it again changes nothing.
        assert!(!mark_aac_priming(&p).unwrap());
        assert_eq!(std::fs::read(&p).unwrap(), out);
    }

    #[test]
    fn files_of_any_other_shape_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let no_priming = clip(&[track(b"soun", b"mp4a", 0)]);
        let not_aac = clip(&[track(b"soun", b"Opus", 312)]);
        let mut moov_first = bx(b"moov", &track(b"soun", b"mp4a", 2112));
        moov_first.extend(bx(b"mdat", &[0; 10]));
        let mut truncated = clip(&[track(b"soun", b"mp4a", 2112)]);
        truncated.truncate(truncated.len() - 3);
        for bytes in [no_priming, not_aac, moov_first, truncated, b"junk".to_vec()] {
            let p = write(&dir, &bytes);
            assert!(!mark_aac_priming(&p).unwrap());
            assert_eq!(std::fs::read(&p).unwrap(), bytes);
        }
    }
}
