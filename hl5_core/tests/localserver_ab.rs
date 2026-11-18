//! The in-process server against the Python one, byte for byte.
//!
//! `tools_re/gen_localserver_fixtures.py` drives the *actual* `hl5_server`
//! code through a scripted conversation and dumps every request/reply pair;
//! this test replays the same requests through `localserver::Session` and
//! asserts the replies are identical. That is the whole acceptance criterion:
//! the Python server is the implementation the real client was debugged
//! against (and the one the S25 logged in to), so "same bytes as it" is what
//! makes the local session trustworthy without a guest run.
//!
//! Regenerate the fixture after changing either side:
//!
//! ```text
//! python3 tools_re/gen_localserver_fixtures.py
//! ```

use hl5_core::localserver::{net_item, Session};
use std::collections::HashMap;

const FIXTURE: &str = include_str!("fixtures/localserver_ab.txt");

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd hex length in fixture: {s}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn drain(s: &mut Session) -> Vec<u8> {
    s.read(1 << 20)
}

/// Every scripted exchange, in fixture order, one Rust session per fixture
/// session — state (the login, the open purchase) carries within a session
/// exactly as it does across one TCP connection.
#[test]
fn replies_match_the_python_server() {
    let mut sessions: HashMap<String, Session> = HashMap::new();
    let mut checked = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("X ")) {
        let mut f = line.split_whitespace();
        let (_, sid, name) = (f.next(), f.next().expect("sid"), f.next().expect("name"));
        let req = unhex(f.next().expect("request"));
        let expect = match f.next().expect("reply") {
            "-" => Vec::new(),
            h => unhex(h),
        };
        let sess = sessions.entry(sid.to_string()).or_default();
        sess.write(&req);
        let got = drain(sess);
        assert_eq!(
            got.len() == 0,
            expect.is_empty(),
            "{sid}/{name}: reply presence differs (got {} bytes)",
            got.len()
        );
        assert_eq!(
            hex(&got),
            hex(&expect),
            "{sid}/{name}: reply bytes differ from the Python server"
        );
        checked += 1;
    }
    assert!(checked >= 10, "fixture looks truncated: only {checked} exchanges");
}

/// The `_NET_ITEM_` builder against `protocol.net_item`.
#[test]
fn net_item_matches_the_python_record() {
    let mut checked = 0;
    for line in FIXTURE.lines().filter(|l| l.starts_with("N ")) {
        let mut f = line.split_whitespace();
        let (_, _name) = (f.next(), f.next().expect("name"));
        let count: i32 = f.next().expect("count").parse().expect("count");
        let kind: u8 = f.next().expect("kind").parse().expect("kind");
        let index: u8 = f.next().expect("index").parse().expect("index");
        let item_name = unhex(f.next().expect("name-hex"));
        let expect = unhex(f.next().expect("record"));
        assert_eq!(hex(&net_item(count, kind, index, &item_name)), hex(&expect));
        checked += 1;
    }
    assert!(checked >= 2, "the fixture must pin both the short and long record");
}

/// TCP gives no message boundaries, and the Python side reads off a socket
/// while the Rust side is handed whatever `send` sizes the guest picks — so
/// a request delivered one byte at a time must produce the same replies as
/// the whole-buffer delivery the fixture recorded.
#[test]
fn split_writes_reassemble() {
    let mut whole = Session::new();
    let mut split = Session::new();
    let mut expect_all = Vec::new();
    for line in FIXTURE.lines().filter(|l| l.starts_with("X s1 ")) {
        let mut f = line.split_whitespace();
        let (_, _, _) = (f.next(), f.next(), f.next());
        let req = unhex(f.next().expect("request"));
        whole.write(&req);
        expect_all.extend(drain(&mut whole));
        for b in &req {
            split.write(std::slice::from_ref(b));
        }
    }
    assert_eq!(hex(&drain(&mut split)), hex(&expect_all));
}

/// A stream that is not SmartReq gets dropped, as the Python server drops the
/// connection: the session stops answering and reads report EOF.
#[test]
fn implausible_length_closes() {
    let mut s = Session::new();
    s.write(&[0, 0, 0, 1]);
    assert!(s.is_closed());
    assert!(s.readable(), "EOF must be readable");
    assert!(s.read(16).is_empty());
    // Later bytes are ignored, not parsed as a fresh stream.
    s.write(&[0u8; 256]);
    assert!(s.read(16).is_empty());
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The one place the local session is *allowed* to differ from the Python
/// server: a configured grant turns 6/5's empty record list into a real one.
///
/// Everything else in this file proves the two agree; this proves the single
/// intended divergence reaches the wire in the shape `NETWORK::readNetItem`
/// (0xC8578) reads. It replays the fixture's own recorded `login` and
/// `buy_paid` requests, so the only variable is the grant table — and the
/// login comes first because `Session` gates 6/5 behind an identity, which is
/// the guest's real sequence.
#[test]
fn a_configured_grant_reaches_the_wire() {
    use hl5_core::shop::{GrantTable, ShopGrant};
    use std::sync::Arc;

    /// The item id both `buy_request` and `buy_paid` carry in the fixture:
    /// 엘릭서(20), whose catalogue `a` is 34 and quantity 20.
    const ITEM: i32 = 4;

    let fixture_request = |name: &str| -> Vec<u8> {
        let line = FIXTURE
            .lines()
            .find(|l| l.starts_with(&format!("X s1 {name} ")))
            .unwrap_or_else(|| panic!("fixture has no s1/{name}"));
        unhex(line.split_whitespace().nth(3).expect("request"))
    };

    let grants =
        GrantTable::build(&[ShopGrant { id: ITEM, kind: 18, index: 34, count: Some(20) }], &[], None);
    let mut s = Session::with_grants(Arc::new(grants));

    s.write(&fixture_request("login"));
    drain(&mut s); // the login answer is what the other tests already pin

    s.write(&fixture_request("buy_paid"));
    let reply = drain(&mut s);

    // [u32 4+len][u32 len]["G1000157"][u32 6][u32 5][i32 result][blob msg]
    // [i32 count][record...] — the two length words, then the 16-byte game
    // header, put the payload at 24.
    let be32 = |at: usize| u32::from_be_bytes(reply[at..at + 4].try_into().unwrap());
    assert_eq!((be32(16), be32(20)), (6, 5), "category/subcmd");
    assert_eq!(be32(24), 0, "result must stay 0 — a grant is not an error");
    assert_eq!(be32(28), 0, "message blob stays empty");
    assert_eq!(be32(32), 1, "one record");

    let rec = &reply[36..];
    // +0x00 is the QUANTITY `ProcPacket_Shop` passes as NewBagNetItem's
    // repeat count, not a uid — and it must be positive or readNetItem ends
    // the list right there.
    assert_eq!(be32(36), 20, "quantity at +0x00");
    assert_eq!((rec[4], rec[5]), (18, 34), "kind/index");
    assert_eq!(u32::from_be_bytes(rec[6..10].try_into().unwrap()), 0, "no catalogue → no name");
    // kind 18 > 10, so readNetItem stops after the name: the record is the
    // short form and must NOT carry the 58-byte equipment tail, which would
    // be read as the next record.
    assert_eq!(rec.len(), 10, "short form: 10 bytes plus the (empty) name");

    // And the default stays the deployed server's answer.
    let mut plain = Session::new();
    plain.write(&fixture_request("login"));
    drain(&mut plain);
    plain.write(&fixture_request("buy_paid"));
    let empty = drain(&mut plain);
    assert_eq!(
        u32::from_be_bytes(empty[32..36].try_into().unwrap()),
        0,
        "with no grant table configured the record list must stay empty"
    );
}
