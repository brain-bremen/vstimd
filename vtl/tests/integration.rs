use vtl::{VtlKind, VtlClient, VtlOwner};

fn unique_name() -> String {
    // Use PID + thread-id hash to avoid collisions between parallel test threads.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::thread::current().id().hash(&mut h);
    format!("/vtl_test_{}_{:x}", std::process::id(), h.finish())
}

#[test]
fn create_and_validate_header() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    assert!(owner.is_valid());
    assert_eq!(owner.num_input_banks(), 1);
    assert_eq!(owner.num_output_banks(), 1);
    drop(owner); // shm_unlink
}

#[test]
fn client_attaches_and_reads_header() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    let client = VtlClient::open(&name).expect("open");
    assert!(client.is_valid());
    drop(client);
    drop(owner);
}

#[test]
fn input_state_roundtrip() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    owner.set_input_state(0, 0b1010);
    assert_eq!(owner.input_state(0), 0b1010);
    drop(owner);
}

#[test]
fn rise_latch_set_and_drain() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");

    // Simulate daqd: OR two rising bits.
    owner.set_input_rise(0, 0b0101);
    owner.set_input_rise(0, 0b1010);

    // Peek without clearing — both sets should be visible.
    assert_eq!(owner.peek_input_rise(0), 0b1111);

    // Drain bit 0 only.
    let drained = owner.drain_input_rise(0, 0b0001);
    assert_eq!(drained, 0b0001);
    assert_eq!(owner.peek_input_rise(0), 0b1110);

    // Drain all remaining.
    let drained = owner.drain_input_rise(0, u64::MAX);
    assert_eq!(drained, 0b1110);
    assert_eq!(owner.peek_input_rise(0), 0);

    drop(owner);
}

#[test]
fn fall_latch_set_and_drain() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    owner.set_input_fall(0, 1 << 7);
    assert_eq!(owner.drain_input_fall(0, u64::MAX), 1 << 7);
    assert_eq!(owner.peek_input_fall(0), 0);
    drop(owner);
}

#[test]
fn output_state_and_pulse() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");

    owner.set_output_state(0, 0xFF);
    assert_eq!(owner.output_state(0), 0xFF);

    owner.set_output_pulse(0, 1 << 3);
    assert_eq!(owner.drain_output_pulse(0, u64::MAX), 1 << 3);
    assert_eq!(owner.peek_output_pulse(0), 0);

    drop(owner);
}

#[test]
fn cross_process_latch_via_client() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    let client = VtlClient::open(&name).expect("open");

    // daqd (client) sets a rising edge.
    client.set_input_rise(0, 1 << 5);
    client.set_input_state(0, 1 << 5);

    // vstimd (owner) drains it.
    let edges = owner.drain_input_rise(0, u64::MAX);
    assert_eq!(edges, 1 << 5);
    assert_eq!(owner.input_state(0), 1 << 5);

    drop(client);
    drop(owner);
}

#[test]
fn named_line_write_and_read() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");

    owner.write_named_line(0, "stim_trigger", 0, 3, VtlKind::Input);
    owner.write_named_line(1, "frame_onset",  0, 0, VtlKind::Output);
    owner.set_n_named_lines(2);

    assert_eq!(owner.n_named_lines(), 2);

    let (e0, d0) = owner.named_line(0).unwrap();
    assert_eq!(e0.name_str(), "stim_trigger");
    assert_eq!(e0.bank, 0);
    assert_eq!(e0.bit, 3);
    assert_eq!(d0, VtlKind::Input);

    let (e1, d1) = owner.named_line(1).unwrap();
    assert_eq!(e1.name_str(), "frame_onset");
    assert_eq!(d1, VtlKind::Output);

    // find_named_line
    let found = owner.find_named_line("stim_trigger");
    assert!(found.is_some());
    let (idx, _, _) = found.unwrap();
    assert_eq!(idx, 0);

    drop(owner);
}

#[test]
fn named_line_visible_via_client() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    owner.write_named_line(0, "hello_vtl", 0, 1, VtlKind::Input);
    owner.set_n_named_lines(1);

    let client = VtlClient::open(&name).expect("open");
    assert_eq!(client.n_named_lines(), 1);
    let (e, _) = client.named_line(0).unwrap();
    assert_eq!(e.name_str(), "hello_vtl");

    drop(client);
    drop(owner);
}

#[test]
fn long_name_is_truncated_not_panicked() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    let long = "x".repeat(200);
    owner.write_named_line(0, &long, 0, 0, VtlKind::Input);
    owner.set_n_named_lines(1);
    let (e, _) = owner.named_line(0).unwrap();
    assert_eq!(e.name_str().len(), 55); // max 55 usable bytes (56th is nul)
    drop(owner);
}

#[test]
fn open_nonexistent_returns_error() {
    let result = VtlClient::open("/vtl_nonexistent_xyzzy_123");
    assert!(result.is_err());
}

#[test]
fn multiple_banks_independent() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 4, 4).expect("create");

    for bank in 0..4 {
        owner.set_input_state(bank, 1u64 << bank);
    }
    for bank in 0..4 {
        assert_eq!(owner.input_state(bank), 1u64 << bank);
    }
    drop(owner);
}

// ── set_input_bit / clear_input_bit ──────────────────────────────────────────

#[test]
fn set_and_clear_input_bit_return_edge_direction() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");

    // Low → high: rising edge (returns true).
    assert!(owner.set_input_bit(0, 5), "first set must report rising edge");
    // High → high: no edge (returns false).
    assert!(!owner.set_input_bit(0, 5), "second set must not report rising edge");

    // High → low: falling edge (returns true).
    assert!(owner.clear_input_bit(0, 5), "first clear must report falling edge");
    // Low → low: no edge (returns false).
    assert!(!owner.clear_input_bit(0, 5), "second clear must not report falling edge");
}

#[test]
fn concurrent_set_input_bit_no_lost_updates() {
    use std::sync::Arc;
    use std::thread;

    let name = unique_name();
    let owner = Arc::new(VtlOwner::create(&name, 1, 1).expect("create"));

    // Under Miri threads are expensive; use fewer bits to keep the suite fast.
    let n_bits: u8 = if cfg!(miri) { 8 } else { 64 };
    let all_set: u64 = if n_bits == 64 { u64::MAX } else { (1u64 << n_bits) - 1 };

    let handles: Vec<_> = (0..n_bits)
        .map(|bit| {
            let o = Arc::clone(&owner);
            thread::spawn(move || { o.set_input_bit(0, bit); })
        })
        .collect();
    for h in handles { h.join().unwrap(); }

    assert_eq!(owner.input_state(0), all_set, "no bit updates must be lost");
}

#[test]
fn concurrent_clear_input_bit_no_lost_updates() {
    use std::sync::Arc;
    use std::thread;

    let name = unique_name();
    let owner = Arc::new(VtlOwner::create(&name, 1, 1).expect("create"));

    let n_bits: u8 = if cfg!(miri) { 8 } else { 64 };
    let all_set: u64 = if n_bits == 64 { u64::MAX } else { (1u64 << n_bits) - 1 };
    owner.set_input_state(0, all_set);

    let handles: Vec<_> = (0..n_bits)
        .map(|bit| {
            let o = Arc::clone(&owner);
            thread::spawn(move || { o.clear_input_bit(0, bit); })
        })
        .collect();
    for h in handles { h.join().unwrap(); }

    assert_eq!(owner.input_state(0), 0, "no bit clears must be lost");
}

// ── Release / Acquire ordering on n_entries ───────────────────────────────────

#[test]
fn named_line_ordering_write_before_publish() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let name = unique_name();
    let owner = Arc::new(VtlOwner::create(&name, 1, 1).expect("create"));
    let barrier = Arc::new(Barrier::new(2));

    // Writer: populate entries first, then publish with a Release store.
    let o_w = Arc::clone(&owner);
    let b_w = Arc::clone(&barrier);
    let writer = thread::spawn(move || {
        o_w.write_named_line(0, "alpha", 0, 0, VtlKind::Input);
        o_w.write_named_line(1, "beta",  0, 1, VtlKind::Output);
        b_w.wait(); // let reader thread start before the publish
        o_w.set_n_named_lines(2); // Release — all preceding writes visible after this
    });

    // Reader: synchronise, then join writer (guarantees the Release store happened),
    // then Acquire-load n_entries and verify the entry data is visible.
    let o_r = Arc::clone(&owner);
    let b_r = Arc::clone(&barrier);
    thread::spawn(move || {
        b_r.wait();
        writer.join().unwrap();
        // Acquire load on n_entries pairs with the Release store in the writer.
        assert_eq!(o_r.n_named_lines(), 2);
        let (e0, d0) = o_r.named_line(0).unwrap();
        assert_eq!(e0.name_str(), "alpha");
        assert_eq!(d0, VtlKind::Input);
        let (e1, d1) = o_r.named_line(1).unwrap();
        assert_eq!(e1.name_str(), "beta");
        assert_eq!(d1, VtlKind::Output);
    })
    .join()
    .unwrap();
}

// ── Output semaphore ──────────────────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn signal_wakes_wait_output() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let name = unique_name();
    let owner = Arc::new(VtlOwner::create(&name, 1, 1).expect("create"));

    let barrier = Arc::new(Barrier::new(2));
    let b_waiter = Arc::clone(&barrier);
    let client = VtlClient::open(&name).expect("open client");

    let waiter = thread::spawn(move || {
        b_waiter.wait(); // ready to wait
        client.wait_output(); // blocks until signaled
    });

    barrier.wait(); // let the waiter block first
    std::thread::sleep(std::time::Duration::from_millis(20));
    owner.signal_output();

    waiter.join().expect("wait_output returned");
}

#[cfg(unix)]
#[test]
fn try_wait_output_returns_false_when_no_signal() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    assert!(!owner.try_wait_output(), "semaphore should start at 0");
}

#[cfg(unix)]
#[test]
fn try_wait_output_consumes_signal() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");
    owner.signal_output();
    assert!(owner.try_wait_output(),  "first try should succeed");
    assert!(!owner.try_wait_output(), "second try should fail (count now 0)");
}

#[cfg(unix)]
#[test]
fn semaphore_count_absorbs_burst() {
    use std::sync::Arc;
    use std::thread;

    let name = unique_name();
    let owner = Arc::new(VtlOwner::create(&name, 1, 1).expect("create"));
    let client = VtlClient::open(&name).expect("open client");

    // Post 3 times before the consumer wakes.
    owner.signal_output();
    owner.signal_output();
    owner.signal_output();

    // Each wait_output call consumes one count — no signals are lost.
    let consumer = thread::spawn(move || {
        client.wait_output();
        client.wait_output();
        client.wait_output();
        // A fourth call would block — verify count is now 0.
        assert!(!client.try_wait_output());
    });
    consumer.join().expect("consumer drained all signals");
}

#[cfg(unix)]
#[test]
fn client_wait_output_signaled_by_owner() {
    let name = unique_name();
    let owner = VtlOwner::create(&name, 1, 1).expect("create");

    owner.set_output_state(0, 0b1010);
    owner.signal_output();

    let client = VtlClient::open(&name).expect("open client");
    client.wait_output(); // must not block
    assert_eq!(client.output_state(0), 0b1010);
}
