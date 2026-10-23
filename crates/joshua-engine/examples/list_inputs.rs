//! Lists audio inputs: `cargo run -p joshua-engine --example list_inputs`
fn main() {
    for d in joshua_engine::input::list_input_devices() {
        println!(
            "audio in: {} · {} ch · {} Hz{}",
            d.name,
            d.channels,
            d.sample_rate,
            if d.is_default { " (default)" } else { "" }
        );
    }
}
