# R2lay
> Short for Rust Relay

A simple TCP relay made in Rust.

## Should support

- [x] CLI
- [x] IPv6 support
- [x] IPv4/IPv6 tunnel
- [x] TCP Proxy Protocol
  * [x] Version 1
  * [x] Version 2

## Usage

If you need logging, just use the env `RUST_LOG=<level>` (for instance: `RUST_LOG=debug`).

```
r2lay 0.0.1
A simple TCP relay made in Rust.

USAGE:
    r2lay [OPTIONS] <proxy-addr> <server-addr>

FLAGS:
    -h, --help       
            Prints help information

    -V, --version    
            Prints version information


OPTIONS:
    -P, --proxy-protocol <proxy-protocol>    
            Enable Proxy Protocol
            
            Add Proxy Protocol header to each connection to the server. [default: disabled]  [possible values: Disabled,
            V1, V2]

ARGS:
    <proxy-addr>     
            The listening TCP address with IP(v4/v6) and port

    <server-addr>    
            Back-end TCP address with IP(v4/v6) and port
```

## License

Copyright 2021 - FuseTim

Available under MIT license terms.