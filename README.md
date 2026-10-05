# xmip-core-contract-rust

A content contract authored in Rust, as a loadable module over the ABI rather than a crate the runtime links. A technology of
[xmip-core-contract](https://github.com/IlleNilsson/xmip-core-contract); ADR-0042
decision 3 (amendment 2026-10-05) admits a contract in Rust or .NET, and ADR-0012 makes
`include/xmip_module.h` in xmip-core-abi the boundary it conforms to.

What it claims today: well-formedness is bytes, every Stream is read to its end
and held (ADR-0042 decision 1). A descriptor a Location binds is kept and
answered by `implies`. A contract with a real standard replaces the one
judgment function and nothing else; the entrypoint, the table and the lifecycle
are done.

## Verification

xgit builds and tests it with cargo like any crate. The runtime's
`ffi::loaded_contract` tests then open the built library through the host's
loader and hold it to the contract table (ADR-0057).
