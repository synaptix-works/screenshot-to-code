# AXON integration

This fork remains an executable specialist rather than an AXON scheduler.

## Headless flow

1. AXON/House selects a bounded set of supported screenshot-to-code models.
2. Start the existing application normally or with Docker.
3. Invoke the Rust adapter.
4. Collect each generated variant plus `run.json`.
5. Compare variants using visual and engineering gates.
6. Send the selected reconstruction to the HAOMA normalization/implementation worker.
7. Verify the resulting HAOMA route independently.

Example:

```bash
cargo run --release --manifest-path axon-adapter/Cargo.toml -- generate \
  --input ./reference.png \
  --stack react-tailwind \
  --design-system ./contracts/HAOMA.md \
  --model "gemini-3-flash-preview (high thinking)" \
  --model "claude-opus-5-5 (medium effort)" \
  --output ./out/reconstruction
```

If `--model` is omitted, upstream screenshot-to-code model selection remains unchanged.

## Design constraint

Do not move the FastAPI/React application into OpenClaw core. Keep it isolated and replaceable. OpenClaw should treat it as a worker appliance.
