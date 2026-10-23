//! Lists MIDI inputs: `cargo run -p joshua-midi --example list_ports`
fn main() {
    match joshua_midi::list_inputs() {
        Ok(ports) if ports.is_empty() => println!("no MIDI input ports"),
        Ok(ports) => ports.iter().for_each(|p| println!("MIDI in: {} ({})", p.name, p.id)),
        Err(e) => println!("MIDI unavailable: {e}"),
    }
}
