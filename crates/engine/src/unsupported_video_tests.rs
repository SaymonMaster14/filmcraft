//! H.264 High 10 and High 4:2:2 (camera formats we have no decoder for yet) must say so: they
//! import with a reason, render the unreadable slate (never black), keep their audio and make an
//! export fail instead of writing a black or slate-baked file. An 8-bit 4:2:0 High control still
//! decodes. Fixtures come from ffmpeg (an external generator only).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use filmcraft_render::offline::{OfflineReason, slate_rgba8};
use serde_json::json;

use crate::Session;
use crate::media_test_util::{frame_rgba, psnr, session_with, tmp_dir};

const W: usize = 160;
const H: usize = 96;

/// A tiny MP4 (H.264 in `pix_fmt` / `profile` + AAC) made by ffmpeg, or `None` when ffmpeg or its
/// libx264 cannot make it (reported, not silent).
fn fixture(ff: &std::path::Path, name: &str, pix_fmt: &str, profile: &str) -> Option<PathBuf> {
    let out = filmcraft_testkit::fixtures_dir("engine/unsupported-video").join(name);
    filmcraft_testkit::fixtures::generate(&out, |tmp| {
        let st = Command::new(ff)
            .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=s=160x96:r=24:d=1", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:d=1"])
            .args(["-c:v", "libx264", "-pix_fmt", pix_fmt, "-profile:v", profile, "-c:a", "aac", "-shortest"])
            .arg(tmp)
            .stdin(Stdio::null())
            .status();
        st.is_ok_and(|s| s.success())
    })
    .or_else(|| {
        eprintln!("SKIPPED: ffmpeg cannot write {name} (libx264 {profile} {pix_fmt})");
        None
    })
}

fn check_unsupported(name: &str, pix_fmt: &str, profile: &str, reason: &str) {
    let ff = filmcraft_testkit::require_ffmpeg!();
    let Some(path) = fixture(&ff, name, pix_fmt, profile) else { return };

    // import says why
    let mut probe = Session::default();
    let r = probe.execute("file.import", json!({"paths": [path.to_string_lossy()]})).unwrap();
    let errors = r["errors"].to_string();
    assert!(errors.contains(reason), "import reports {reason:?}: {r}");

    // the clip renders the unreadable slate, not black
    let (mut s, items, seq) = session_with(&[&path]);
    let (w, h, px) = frame_rgba(&mut s, 3, 1.0);
    assert_eq!((w, h), (W, H));
    let slate = slate_rgba8(w, h, name, OfflineReason::Unreadable);
    assert!(psnr(&px, &slate) > 40.0, "{name} renders the unreadable slate");
    let st = s.media.offline_status(items[0]).expect("offline status");
    assert!(st.unsupported_video && st.reason == OfflineReason::Unreadable && st.error.contains(reason), "{st:?}");

    // its audio is still the file's own
    let src = s.media.full_res_source(&s.project, items[0], &*s.services).unwrap();
    let a = src.audio(0, 2048, 48_000).unwrap();
    assert!(a.channels[0].iter().any(|v| v.abs() > 0.01), "audio plays");

    // an export refuses instead of writing a slate or black picture
    let out = tmp_dir("unsupported-video-export").join("out.mp4");
    let e = s.execute("file.exportMedia", json!({"path": out.to_string_lossy(), "format": "h264", "sequence": seq.0, "wait": true})).unwrap_err().to_string();
    assert!(e.contains(reason), "export fails with the reason: {e}");
    assert!(!out.exists(), "no output file");
}

#[test]
fn h264_high10_is_unsupported_not_black() {
    check_unsupported("h264_high10.mp4", "yuv420p10le", "high10", "H.264 High 10 (4:2:0, 10-bit) is not supported yet");
}

#[test]
fn h264_high422_is_unsupported_not_black() {
    check_unsupported("h264_high422.mp4", "yuv422p", "high422", "H.264 High 4:2:2 (4:2:2, 8-bit) is not supported yet");
}

#[test]
fn h264_high_8bit_420_still_decodes() {
    let ff = filmcraft_testkit::require_ffmpeg!();
    let Some(path) = fixture(&ff, "h264_high8.mp4", "yuv420p", "high") else { return };
    let mut probe = Session::default();
    let r = probe.execute("file.import", json!({"paths": [path.to_string_lossy()]})).unwrap();
    assert_eq!(r["errors"], json!([]), "{r}");
    let (mut s, items, _) = session_with(&[&path]);
    let (w, h, px) = frame_rgba(&mut s, 3, 1.0);
    assert!(psnr(&px, &slate_rgba8(w, h, "h264_high8.mp4", OfflineReason::Unreadable)) < 30.0, "a decodable clip is not the slate");
    assert!(px.chunks(4).any(|p| p[0] > 40 || p[1] > 40 || p[2] > 40), "not black");
    assert!(s.media.offline_status(items[0]).is_none());
}
