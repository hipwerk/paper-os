//! Byte-for-byte acceptance test for the typography specimen.

use paperos_specimen::render_specimen;

const ACCEPTED_SPECIMEN: &[u8] = include_bytes!("golden/typography-specimen.pgm");

#[test]
fn typography_specimen_matches_accepted_golden_page() {
    let frame = render_specimen().unwrap();
    let header = format!("P5\n{} {}\n255\n", frame.size().width, frame.size().height);
    let mut actual = Vec::with_capacity(header.len() + frame.pixels().len());
    actual.extend_from_slice(header.as_bytes());
    actual.extend_from_slice(frame.pixels());

    assert_eq!(
        actual, ACCEPTED_SPECIMEN,
        "render changed; inspect the generated PGM before accepting a new golden"
    );
}
