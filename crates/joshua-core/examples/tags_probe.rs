//! Prints the tags and pictures Symphonia finds in a file.
#![allow(clippy::unwrap_used)]
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let mss = MediaSourceStream::new(Box::new(std::fs::File::open(&path).unwrap()), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .unwrap();
    let mut md = format.metadata();
    let mut n = 0;
    loop {
        if let Some(rev) = md.current() {
            for t in &rev.media.tags {
                println!("tag {:?} = {:?}", t.std, t.raw.key);
            }
            for v in &rev.media.visuals {
                println!("visual {:?} {:?} {} bytes", v.usage, v.media_type, v.data.len());
            }
        }
        n += 1;
        if md.is_latest() || n > 10 {
            break;
        }
        md.pop();
    }
}
