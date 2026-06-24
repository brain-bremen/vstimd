use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub vtl:  VtlConfig,
    pub gpio: GpioConfig,
    #[serde(default)]
    pub outputs: Vec<OutputLine>,
    #[serde(default)]
    pub inputs: Vec<InputLine>,
}

#[derive(Deserialize, Debug)]
pub struct VtlConfig {
    /// POSIX shared memory name, e.g. "/vstimd_vtl".
    pub shm_name: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GpioConfig {
    /// GPIO chip device path, e.g. "/dev/gpiochip0".
    pub chip: String,
}

/// Maps one VTL output bit → one GPIO output pin.
///
/// vstimd writes `output_state`; this daemon drives the pin to match.
#[derive(Deserialize, Debug, Clone)]
pub struct OutputLine {
    /// Must match the name registered in the VTL names table by vstimd.
    pub name: String,
    pub vtl_bank: u8,
    pub vtl_bit: u8,
    /// GPIO line offset within the chip (not the 40-pin header number).
    pub gpio_line: u32,
}

/// Maps one GPIO input pin → one VTL input bit + rise/fall latches.
///
/// This daemon watches for edges and writes `input_state` and latches.
#[derive(Deserialize, Debug, Clone)]
pub struct InputLine {
    pub name: String,
    pub vtl_bank: u8,
    pub vtl_bit: u8,
    pub gpio_line: u32,
    pub edge: Edge,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Rising,
    Falling,
    Both,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let raw = r#"
            [vtl]
            shm_name = "/vstimd_vtl"

            [gpio]
            chip = "/dev/gpiochip0"
        "#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.vtl.shm_name, "/vstimd_vtl");
        assert!(cfg.outputs.is_empty());
        assert!(cfg.inputs.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let raw = r#"
            [vtl]
            shm_name = "/vstimd_vtl"

            [gpio]
            chip = "/dev/gpiochip0"

            [[outputs]]
            name      = "stim_onset"
            vtl_bank  = 0
            vtl_bit   = 0
            gpio_line = 79

            [[inputs]]
            name      = "scanner_trigger"
            vtl_bank  = 0
            vtl_bit   = 0
            gpio_line = 77
            edge      = "rising"
        "#;
        let cfg: Config = toml::from_str(raw).unwrap();
        assert_eq!(cfg.outputs.len(), 1);
        assert_eq!(cfg.outputs[0].gpio_line, 79);
        assert_eq!(cfg.inputs[0].edge, Edge::Rising);
    }

    #[test]
    fn reject_unknown_edge() {
        let raw = r#"
            [vtl]
            shm_name = "/vstimd_vtl"
            [gpio]
            chip = "/dev/gpiochip0"
            [[inputs]]
            name = "x"
            vtl_bank = 0
            vtl_bit = 0
            gpio_line = 1
            edge = "bogus"
        "#;
        assert!(toml::from_str::<Config>(raw).is_err());
    }
}
