# Patched dependencies

- `netdev/`: netdev 0.46.3 (MIT, by shellrow), used by iroh's network monitoring. Patched so it
  builds for iOS: `src/os/darwin/ipv6_addr_flags.rs` reads IPv6 address flags only on macOS
  (`libc::in6_ifreq` doesn't exist for iOS) and returns the default elsewhere. Drop the patch
  once iroh moves to a netdev release that builds for iOS.
- `netwatch/`: netwatch 0.19.3 (MIT/Apache-2.0, by n0), iroh's network monitor. Patched so it builds
  for iOS: the routing-socket constants it needs (`RTM_*`, `RTAX_*`, `RTF_*`, `RTA_IFP`) come from
  `src/rtc.rs`, which re-exports them from `libc` on macOS and defines them with the same values on
  iOS (`libc` only has them for macOS; iOS's kernel uses the same numbers). Drop the patch once iroh
  moves to a netwatch release that builds for iOS.
