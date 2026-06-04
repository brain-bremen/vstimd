/// vtl-test-client — CLI tool for inspecting and toggling VTL shared memory.
///
/// Usage:
///   vtl-test-client <shm-path> list
///   vtl-test-client <shm-path> set <name> <0|1>
///   vtl-test-client <shm-path> pulse <name>
///   vtl-test-client <shm-path> watch
use vtl::{Direction, VtlClient};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: vtl-test-client <shm-path> <list|set|pulse|watch> [args...]");
        std::process::exit(1);
    }

    let shm_path = &args[1];
    let cmd      = &args[2];

    let client = VtlClient::open(shm_path).unwrap_or_else(|e| {
        eprintln!("Cannot open '{}': {}", shm_path, e);
        std::process::exit(1);
    });

    match cmd.as_str() {
        "list" => cmd_list(&client),
        "set"  => {
            if args.len() < 5 {
                eprintln!("Usage: vtl-test-client <shm-path> set <name> <0|1>");
                std::process::exit(1);
            }
            let value = match args[4].as_str() {
                "0" => false,
                "1" => true,
                _ => {
                    eprintln!("value must be 0 or 1");
                    std::process::exit(1);
                }
            };
            cmd_set(&client, &args[3], value);
        }
        "pulse" => {
            if args.len() < 4 {
                eprintln!("Usage: vtl-test-client <shm-path> pulse <name>");
                std::process::exit(1);
            }
            cmd_pulse(&client, &args[3]);
        }
        "watch" => cmd_watch(&client),
        other => {
            eprintln!("Unknown command: {other}");
            std::process::exit(1);
        }
    }
}

fn cmd_list(client: &VtlClient) {
    println!("VTL segment  input_banks={}  output_banks={}",
        client.num_input_banks(), client.num_output_banks());
    println!();
    println!("{:<24} {:>5} {:>4}  {:>6}  {:>16}  {:>16}",
        "name", "bank", "bit", "dir", "state", "latches(r/f)");
    println!("{}", "-".repeat(84));

    let n = client.n_named_lines();
    for i in 0..n {
        let Some((entry, dir)) = client.named_line(i) else { continue };
        let b  = entry.bank as usize;
        let mask = 1u64 << entry.bit;
        let state_word = match dir {
            Direction::Input  => client.input_state(b),
            Direction::Output => client.output_state(b),
        };
        let state = if state_word & mask != 0 { 1u8 } else { 0u8 };
        let rise  = (client.peek_input_rise(b) & mask != 0) as u8;
        let fall  = (client.peek_input_fall(b) & mask != 0) as u8;
        let dir_s = match dir { Direction::Input => "in", Direction::Output => "out" };
        println!("{:<24} {:>5} {:>4}  {:>6}  {:>16}  {:>7}/{:>7}",
            entry.name_str(), b, entry.bit, dir_s, state, rise, fall);
    }

    if n == 0 {
        println!("(no named lines registered)");
    }
}

fn cmd_set(client: &VtlClient, line_name: &str, value: bool) {
    let Some((_, entry, dir)) = client.find_named_line(line_name) else {
        eprintln!("Line '{}' not found", line_name);
        std::process::exit(1);
    };
    if dir != Direction::Input {
        eprintln!("Warning: '{}' is an output line — writing input state anyway", line_name);
    }
    let b   = entry.bank as usize;
    let bit = entry.bit;

    // Use atomic fetch_or/fetch_and so a concurrent writer cannot be silently clobbered.
    if value {
        let was_low = client.set_input_bit(b, bit);
        if was_low {
            client.set_input_rise(b, 1u64 << bit);
            println!("↑ rising edge on '{}'  (bank={} bit={})", line_name, b, bit);
        } else {
            println!("  already high: '{}'  (bank={} bit={})", line_name, b, bit);
        }
    } else {
        let was_high = client.clear_input_bit(b, bit);
        if was_high {
            client.set_input_fall(b, 1u64 << bit);
            println!("↓ falling edge on '{}'  (bank={} bit={})", line_name, b, bit);
        } else {
            println!("  already low: '{}'  (bank={} bit={})", line_name, b, bit);
        }
    }
}

fn cmd_pulse(client: &VtlClient, line_name: &str) {
    // Rising then falling in quick succession (useful to test latch capture).
    cmd_set(client, line_name, true);
    std::thread::sleep(std::time::Duration::from_millis(1));
    cmd_set(client, line_name, false);
}

fn cmd_watch(client: &VtlClient) {
    println!("Watching VTL lines (Ctrl-C to stop)…");
    let n = client.n_named_lines();
    loop {
        let mut line = String::new();
        for i in 0..n {
            let Some((entry, _)) = client.named_line(i) else { continue };
            let b    = entry.bank as usize;
            let mask = 1u64 << entry.bit;
            let high = client.input_state(b) & mask != 0;
            if !line.is_empty() { line.push_str("  "); }
            line.push_str(&format!("{}:{}", entry.name_str(), if high { '1' } else { '0' }));
        }
        print!("\r{:<80}", if line.is_empty() { "(no named lines)".to_string() } else { line });
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
