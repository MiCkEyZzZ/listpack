# listpack

In-memory Listpack implementation for the Zumic.

> ⚠️ **Note:**
> This crate is intended for **internal use** within the Zumic codebase.
> If you need a standalone or public-facing Listpack, please look for
community-maintained crates on [crates.io](https://crates.io/).

## Features

- 🔹 **Compact storage** using varint-encoded lengths
- 🔹 **Bidirectional insertions** (`push_front` and `push_back`)
- 🔹 **Indexed access** (`get`)
- 🔹 **Iterator support** for sequential traversal
- 🔹 **In-place removal** of arbitrary entries
- 🔹 **Benchmarks** via Criterion (see `bench/listpack_benchmarks.rs`)

## License

This package is distributed under the MIT License. A full copy of the license
is available in the [License](./LICENSE) file.
