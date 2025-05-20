# Python Examples
The example requires the `uv` package manager to be installed. This is most easily accomplised via `brew install uv` on macOS. 

```sh
cargo build ../../target/debug/stepflow-main run --flow=basic.yaml --input=input1.json
cat input1.json | ../../target/debug/stepflow-main run --flow=basic.yaml
```