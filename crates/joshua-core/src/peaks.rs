//! Waveform overview data for the UI.

use crate::audio::StereoBuffer;

/// Upper bound on buckets requested through the IPC boundary.
pub const MAX_BUCKETS: usize = 8_192;

/// Peak amplitude (max of `|L|`, `|R|`) for `buckets` equal slices of the
/// buffer, quantised to `0..=255` to keep IPC payloads small.
#[must_use]
pub fn compute_peaks(buffer: &StereoBuffer, buckets: usize) -> Vec<u8> {
    let buckets = buckets.clamp(1, MAX_BUCKETS);
    let frames = buffer.frames();
    if frames == 0 {
        return vec![0; buckets];
    }
    (0..buckets)
        .map(|b| {
            let lo = b * frames / buckets;
            let hi = ((b + 1) * frames / buckets).max(lo + 1).min(frames);
            let peak = buffer.left()[lo..hi]
                .iter()
                .zip(&buffer.right()[lo..hi])
                .fold(0.0f32, |m, (l, r)| m.max(l.abs()).max(r.abs()));
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let q = (peak.min(1.0) * 255.0).round() as u8;
            q
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_capture_maxima_per_bucket() {
        let mut left = vec![0.0; 100];
        left[10] = 1.0;
        left[60] = -0.5;
        let buf = StereoBuffer::new(8_000, left, vec![0.0; 100]).unwrap();
        let p = compute_peaks(&buf, 4);
        assert_eq!(p, vec![255, 0, 128, 0]);
    }

    #[test]
    fn handles_degenerate_sizes() {
        let empty = StereoBuffer::silent(8_000, 0).unwrap();
        assert_eq!(compute_peaks(&empty, 0).len(), 1);
        let small = StereoBuffer::silent(8_000, 3).unwrap();
        assert_eq!(compute_peaks(&small, 10).len(), 10);
        assert_eq!(compute_peaks(&small, usize::MAX).len(), MAX_BUCKETS);
    }
}
