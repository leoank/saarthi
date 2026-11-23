fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure().compile_protos(
        &[
            "proto/saarthi/event/v1/event.proto",
            "proto/saarthi/node/v1/node.proto",
            "proto/saarthi/control/v1/control.proto",
            "proto/saarthi/shells/v1/shells.proto",
        ],
        &["proto"],
    )?;
    Ok(())
}
