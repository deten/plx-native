//! Shuffle Show / Shuffle Season: the shuffled queue's server requests, and the bookkeeping that
//! lets each Up Next advance reuse it.

use super::*;

fn row(rk: &str, item_id: i64) -> plx_plex::plex::QueueRow {
    plx_plex::plex::QueueRow {
        item_id,
        rk: rk.into(),
        kind: "episode".into(),
        part: format!("/library/parts/{rk}/file.mkv"),
        ..Default::default()
    }
}

fn queue(sid: ServerId, rks: &[&str]) -> ShuffleQueue {
    ShuffleQueue { sid, id: 7, rows: rks.iter().enumerate().map(|(i, rk)| row(rk, 700 + i as i64)).collect() }
}

#[test]
fn the_shuffled_order_names_each_rows_successor_on_its_own_server_only() {
    let sid = ServerId::from_raw(3);
    let other = ServerId::from_raw(4);
    let q = queue(sid, &["30", "10", "40"]);
    assert_eq!(q.index_of(sid, "10"), Some(1));
    assert_eq!(q.next_after(sid, "30").map(|r| r.rk.as_str()), Some("10"));
    assert_eq!(q.next_after(sid, "10").map(|r| r.rk.as_str()), Some("40"));
    assert!(q.next_after(sid, "40").is_none(), "the end of the retained rows");
    assert!(q.index_of(other, "10").is_none(), "the same key on another server is another item");
    assert!(q.index_of(sid, "").is_none());
    let next = q.next_after(sid, "30").and_then(up_next_of).expect("an episode row arms Up Next");
    assert_eq!(next.rk, "10");
}

#[test]
fn a_window_read_from_the_server_extends_the_rows_from_where_it_starts() {
    let sid = ServerId::from_raw(3);
    let mut q = queue(sid, &["30", "10"]);
    q.extend_from(sid, "10", vec![row("10", 701), row("40", 702), row("20", 703)]);
    assert_eq!(q.rows.iter().map(|r| r.rk.as_str()).collect::<Vec<_>>(), ["30", "10", "40", "20"]);
    // A window that does not start at the row asked about is not this queue's continuation.
    q.extend_from(sid, "40", vec![row("99", 900)]);
    assert_eq!(q.rows.len(), 4);
    q.extend_from(sid, "nowhere", vec![row("nowhere", 1)]);
    assert_eq!(q.rows.len(), 4);
}

/// Loopback PMS for the shuffle request: `POST /playQueues` answers a two-row window whose
/// selected item is its first, `GET /playQueues/7` the rest from the center. Every request line
/// comes back from the join handle.
fn shuffle_pms() -> (i32, std::sync::mpsc::Sender<()>, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port() as i32;
    let (done, stop) = std::sync::mpsc::channel::<()>();
    let handle = std::thread::spawn(move || {
        let mut lines = Vec::new();
        while stop.try_recv().is_err() {
            let socket = match listener.accept() {
                Ok((s, _)) => s,
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
            };
            socket.set_nonblocking(false).unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0usize;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(n) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap_or(0);
                }
            }
            let _ = reader.read_exact(&mut vec![0; length]);
            let ep = |rk: &str, id: i64| format!(
                r#"{{"ratingKey":"{rk}","playQueueItemID":{id},"type":"episode","title":"E{rk}","grandparentTitle":"Show","parentIndex":1,"index":{rk},"duration":60000,"Media":[{{"videoCodec":"h264","audioCodec":"aac","Part":[{{"id":{rk},"key":"/library/parts/{rk}/file.mkv"}}]}}]}}"#);
            let body = if line.starts_with("POST /playQueues") {
                format!(r#"{{"MediaContainer":{{"playQueueID":7,"playQueueSelectedItemID":701,"playQueueSelectedItemOffset":0,"playQueueTotalCount":4,"Metadata":[{},{}]}}}}"#,
                    ep("3", 701), ep("1", 702))
            } else if line.starts_with("GET /playQueues/7") {
                format!(r#"{{"MediaContainer":{{"playQueueID":7,"playQueueSelectedItemID":701,"playQueueTotalCount":4,"Metadata":[{},{},{},{}]}}}}"#,
                    ep("3", 701), ep("1", 702), ep("4", 703), ep("2", 704))
            } else {
                r#"{"MediaContainer":{}}"#.to_string()
            };
            lines.push(line);
            let mut socket = socket;
            let _ = write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
        lines
    });
    (port, done, handle)
}

#[test]
fn a_shuffle_asks_for_the_shows_children_shuffled_then_reads_the_rest_of_the_queue() {
    let _serial = plx_base::testlock::serial();
    assert!(plx_net::net::global_init() && crate::curlio::available());
    plx_plex::plex::reset_servers_for_test();
    let (port, done, server) = shuffle_pms();
    let sid = plx_plex::plex::register_for_test("shuffle-machine", "127.0.0.1", port, "token", "shuffle-client");
    let landed = fetch_shuffle(sid, "100");
    done.send(()).unwrap();
    let lines = server.join().unwrap();
    plx_plex::plex::reset_servers_for_test();

    let q = landed.expect("a shuffled queue");
    assert_eq!(q.id, 7);
    assert_eq!(q.sid, sid);
    assert_eq!(q.rows.iter().map(|r| r.rk.as_str()).collect::<Vec<_>>(), ["3", "1", "4", "2"],
        "the POST's window, then the rest read from the server, in the server's order");
    assert_eq!(shuffle_first(&q).map(|u| u.rk), Some("3".to_string()), "playback starts at the selected row");

    let post = lines.iter().find(|l| l.starts_with("POST /playQueues")).expect("a queue POST");
    assert!(post.contains("shuffle=1") && post.contains("continuous=0") && post.contains("type=video"), "{post}");
    assert!(post.contains("shuffle-machine"), "the server's own machine id: {post}");
    assert!(post.contains("%2Flibrary%2Fmetadata%2F100%2Fchildren") || post.contains("/library/metadata/100/children"), "{post}");
    let get = lines.iter().find(|l| l.starts_with("GET /playQueues/7")).expect("a window read");
    assert!(get.contains("window=200") && get.contains("center=701") && get.contains("includeBefore=0"), "{get}");
}
