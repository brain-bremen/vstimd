/// Hardware loopback tests: one GPIO output pin wired to one GPIO input pin.
///
/// # Setup
///
/// 1. Compile with `--features hw-tests`:
///    ```
///    cargo test --features hw-tests -- loopback
///    ```
/// 2. Set these env vars (GPIO line offsets, not 40-pin header numbers):
///    ```
///    export LOOPBACK_CHIP=/dev/gpiochip0
///    export LOOPBACK_OUT=79   # e.g. header pin 29
///    export LOOPBACK_IN=77    # e.g. header pin 26
///    ```
/// 3. Wire the two pins together with a jumper (1 kΩ series resistor recommended).
///
/// Run with `-- --test-threads=1` to avoid competing for the same GPIO lines.
#[cfg(feature = "hw-tests")]
mod hw {
    use std::thread;
    use std::time::Duration;

    use gpio_cdev::{Chip, EventRequestFlags, LineRequestFlags};
    use gpiochip_daqd::bridge;
    use gpiochip_daqd::config::{Edge, InputLine, OutputLine};
    use vtl::VtlOwner;

    fn env_loopback() -> (String, u32, u32) {
        let chip = std::env::var("LOOPBACK_CHIP").unwrap_or_else(|_| "/dev/gpiochip0".into());
        let out: u32 = std::env::var("LOOPBACK_OUT")
            .expect("set LOOPBACK_OUT to GPIO line offset of the output pin")
            .parse()
            .expect("LOOPBACK_OUT must be a u32");
        let inp: u32 = std::env::var("LOOPBACK_IN")
            .expect("set LOOPBACK_IN to GPIO line offset of the input pin")
            .parse()
            .expect("LOOPBACK_IN must be a u32");
        (chip, out, inp)
    }

    fn unique_shm() -> String {
        format!("/gpiochip_lb_{}", std::process::id())
    }

    /// Verify the wire: drive the output pin directly and read the input pin.
    /// Tests GPIO hardware continuity before the VTL layer is involved.
    #[test]
    fn gpio_continuity() {
        let (chip_path, out_line, in_line) = env_loopback();
        let mut chip = Chip::new(&chip_path).expect("open chip");

        let out_h = chip
            .get_line(out_line)
            .unwrap()
            .request(LineRequestFlags::OUTPUT, 0, "lb-test-out")
            .expect("request output");
        let in_h = chip
            .get_line(in_line)
            .unwrap()
            .request(LineRequestFlags::INPUT, 0, "lb-test-in")
            .expect("request input");

        out_h.set_value(0).unwrap();
        thread::sleep(Duration::from_millis(5));
        assert_eq!(in_h.get_value().unwrap(), 0, "expected low");

        out_h.set_value(1).unwrap();
        thread::sleep(Duration::from_millis(5));
        assert_eq!(in_h.get_value().unwrap(), 1, "expected high after driving output");

        out_h.set_value(0).unwrap();
        thread::sleep(Duration::from_millis(5));
        assert_eq!(in_h.get_value().unwrap(), 0, "expected low again");
    }

    /// VTL output_state → poll_outputs_once → GPIO pin: verify pin level matches.
    #[test]
    fn vtl_output_drives_pin() {
        let (chip_path, out_line, in_line) = env_loopback();
        let shm = unique_shm();
        let owner = VtlOwner::create(&shm, 1, 1).expect("create VTL");

        let out_cfg = vec![OutputLine {
            name: "stim_onset".into(),
            vtl_bank: 0,
            vtl_bit: 0,
            gpio_line: out_line,
        }];

        // Use a direct GPIO read to verify pin level — independent of input watcher.
        let mut chip = Chip::new(&chip_path).expect("open chip for read-back");
        let readback = chip
            .get_line(in_line)
            .unwrap()
            .request(LineRequestFlags::INPUT, 0, "lb-readback")
            .expect("request readback input");

        owner.set_output_state(0, 0);
        let vtl_c = vtl::VtlClient::open(&shm).unwrap();
        bridge::poll_outputs_once(&chip_path, &out_cfg, &vtl_c).expect("poll low");
        thread::sleep(Duration::from_millis(5));
        assert_eq!(readback.get_value().unwrap(), 0, "pin should be low");

        owner.set_output_state(0, 1); // set bit 0
        let vtl_c2 = vtl::VtlClient::open(&shm).unwrap();
        bridge::poll_outputs_once(&chip_path, &out_cfg, &vtl_c2).expect("poll high");
        thread::sleep(Duration::from_millis(5));
        assert_eq!(readback.get_value().unwrap(), 1, "pin should be high");
    }

    /// GPIO input edge → VTL input_state + rise/fall latch via spawn_input_watcher.
    ///
    /// Drives the output pin directly and expects the watcher to set the latch.
    #[test]
    fn gpio_input_sets_vtl_latch() {
        let (chip_path, out_line, in_line) = env_loopback();
        let shm = unique_shm();
        let owner = VtlOwner::create(&shm, 1, 1).expect("create VTL");

        let inp = InputLine {
            name: "scanner".into(),
            vtl_bank: 0,
            vtl_bit: 0,
            gpio_line: in_line,
            edge: Edge::Both,
        };
        let client = vtl::VtlClient::open(&shm).expect("client");
        let _watcher = bridge::spawn_input_watcher(chip_path.clone(), inp, client);

        // Give the watcher thread time to open the chip and arm the event handle.
        thread::sleep(Duration::from_millis(100));

        // Drive the output pin high via direct GPIO to produce a rising edge.
        let mut chip = Chip::new(&chip_path).expect("open chip for drive");
        let out_h = chip
            .get_line(out_line)
            .unwrap()
            .request(LineRequestFlags::OUTPUT, 0, "lb-drive")
            .expect("request output drive");

        out_h.set_value(1).unwrap();
        thread::sleep(Duration::from_millis(50));

        let rise = owner.peek_input_rise(0);
        assert_ne!(rise & 1, 0, "rise latch should be set (got {rise:#x})");
        assert_ne!(owner.input_state(0) & 1, 0, "input_state bit 0 should be high");

        // Drive low and check fall latch.
        out_h.set_value(0).unwrap();
        thread::sleep(Duration::from_millis(50));

        let fall = owner.peek_input_fall(0);
        assert_ne!(fall & 1, 0, "fall latch should be set (got {fall:#x})");
        assert_eq!(owner.input_state(0) & 1, 0, "input_state bit 0 should be low");
    }
}
