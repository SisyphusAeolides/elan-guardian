use std::fs;
use std::io;
use std::path::Path;

pub fn total_from_proc(path: &Path) -> io::Result<u64> {
    parse_total(&fs::read_to_string(path)?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "elan_i2c IRQ is not registered"))
}

pub fn parse_total(contents: &str) -> Option<u64> {
    let mut found = false;
    let mut total = 0_u64;
    for line in contents.lines().filter(|line| line.contains("elan_i2c")) {
        let (_, counters) = line.split_once(':')?;
        let mut line_found = false;
        for field in counters.split_whitespace() {
            let Ok(value) = field.parse::<u64>() else {
                break;
            };
            total = total.saturating_add(value);
            line_found = true;
        }
        found |= line_found;
    }
    found.then_some(total)
}

#[cfg(test)]
mod tests {
    use super::parse_total;

    #[test]
    fn sums_all_cpu_counters_for_elan_irq() {
        let fixture = " 16: 10 20 0 IR-IO-APIC i801_smbus\n206: 7 8 9 dummy elan_i2c\n";
        assert_eq!(parse_total(fixture), Some(24));
    }

    #[test]
    fn ignores_unrelated_irqs() {
        assert_eq!(parse_total("16: 10 20 IR-IO-APIC i801_smbus\n"), None);
    }
}
