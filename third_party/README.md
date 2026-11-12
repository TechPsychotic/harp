# Patched dependencies

- `netdev/`: netdev 0.46.3 (MIT, by shellrow), used by iroh's network monitoring. Patched so it
  builds for iOS: `src/os/darwin/ipv6_addr_flags.rs` reads IPv6 address flags only on macOS
  (`libc::in6_ifreq` doesn't exist for iOS) and returns the default elsewhere. Drop the patch
  once iroh moves to a netdev release that builds for iOS.
