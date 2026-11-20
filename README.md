# basalt-plugin-parser-koka

Pure-Rust Koka parser WASM plugin for Basalt.

Implements the parser ABI consumed by Basalt Core:
- `basalt_lang` -> `"koka"`
- `basalt_lexical_profile` -> `.kk` / `.kki` lexical profile
- `basalt_parse` -> 12-byte syntax highlight spans
- `basalt_retrieval_chunks` -> 104-byte v1 retrieval chunks (`module`, `type`, `function`, `property`, `enum_case`)
- `basalt_call_sites` -> 68-byte call-site records

## Build

```sh
cargo build --release --target wasm32-wasip1
```
