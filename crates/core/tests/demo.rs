//! The app's demo (phase PHS-20261117-f22kxe item 13): the recording bundled with the app
//! reads as the session it was, moved to end a few minutes ago.

use saarthi_core::{Role, demo};

const RECORDING: &str = include_str!("../../../android/app/src/main/assets/demo.json");

#[test]
fn the_bundled_recording_reads_as_its_session() {
    let now = 2_000_000_000_000;
    let d = demo(RECORDING.into(), now).unwrap();
    assert_eq!(d.session.title, "Fix the word count test");
    assert!(d.session.spawned && !d.session.busy);
    assert_eq!(d.node.name, "laptop");
    let first = &d.items[0];
    assert_eq!(first.role, Role::User);
    assert!(first.text.starts_with("The test in test_tally.py fails"), "{}", first.text);
    assert!(d.items.iter().any(|i| i.role == Role::Tool && i.tool == "Edit"));
    // The edit's result carries its diff.
    assert!(d.items.iter().any(|i| i.role == Role::ToolResult && i.diff.as_deref().is_some_and(|d| d.contains("+import re"))));
    assert!(d.items.iter().any(|i| i.role == Role::ToolResult && i.text.contains("OK")));
    let last = d.items.iter().rev().find(|i| i.role == Role::Assistant).unwrap();
    assert!(last.text.contains("The test now passes"), "{}", last.text);
    assert_eq!(d.session.last_reply, last.text);
    let latest = d.items.iter().map(|i| i.time_ms).max().unwrap();
    assert!(latest <= now - 5 * 60_000 && latest > now - 10 * 60_000, "{latest}");
    assert!(!d.usage.is_empty());
    // Nothing of the machine it was recorded on.
    for word in ["/Users/", "/private/", "scratchpad"] {
        assert!(!RECORDING.contains(word), "{word}");
    }
}
