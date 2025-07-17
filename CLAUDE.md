# Code Agent Directions

## Linting

Always lint with the `--fix` option to automatically correct issues where
possible. Use the following command:

```bash
cargo clippy --fix --all-targets --all-features --allow-dirty --tests --examples
```

## Running Tests

When changes are complete, use the following script to run all tests:

```bash
cargo test --all
```

## Check that GUI builds

```bash
cd crates/hotki
dx build
```
