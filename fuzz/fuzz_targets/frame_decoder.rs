// SPDX-License-Identifier: GPL-3.0-or-later

//! Feeds arbitrary bytes to the frame decoder, split at arbitrary points. Whatever decodes must
//! encode back to the same envelope, and the IDs of the clip offers in it go through the copies
//! a device remembers (`SeenClips`), which must stay within its bound.
//! Run with `cargo +nightly fuzz run frame_decoder`.

#![no_main]

use std::time::{Duration, Instant};

use libfuzzer_sys::fuzz_target;
use pasteget_core::clip::OriginClipId;
use pasteget_core::framing::{self, FrameDecoder};
use pasteget_core::identity::DeviceId;
use pasteget_core::proto::envelope::Payload;
use pasteget_core::session::{SEEN_MAX, SeenClips};

fuzz_target!(|data: &[u8]| {
    // The first byte picks how the stream is split into reads.
    let Some((&split, stream)) = data.split_first() else {
        return;
    };
    let piece_len = usize::from(split).max(1);

    let mut decoder = FrameDecoder::new();
    let mut seen = SeenClips::default();
    let start = Instant::now();
    for piece in stream.chunks(piece_len) {
        decoder.push(piece);
        loop {
            let envelope = match decoder.next_envelope() {
                Ok(Some(envelope)) => envelope,
                Ok(None) => break,
                Err(_) => return,
            };
            let frame = framing::encode(&envelope).expect("what decoded fits again");
            let mut again = FrameDecoder::new();
            again.push(&frame);
            assert_eq!(again.next_envelope().expect("round trip"), Some(envelope.clone()));

            if let Some(Payload::ClipOffer(offer)) = envelope.payload {
                let (Ok(origin), Ok(Some(id))) = (
                    DeviceId::try_from(offer.origin_device.as_slice()),
                    OriginClipId::from_offer(&offer.origin_clip_id),
                ) else {
                    continue;
                };
                // At times the input picks, in any order, over twice the time copies are
                // remembered: a copy just taken is never taken again at once.
                let now = start + Duration::from_secs(offer.age_ms % 1200);
                if seen.first_time(origin, id, now) {
                    assert!(!seen.first_time(origin, id, now));
                }
                assert!(seen.len() <= SEEN_MAX);
            }
        }
    }
});
