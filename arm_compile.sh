cargo build --target aarch64-unknown-linux-gnu --release
mv target/aarch64-unknown-linux-gnu/release/$(basename $(pwd)) .