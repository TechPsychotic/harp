# Security

If you find a security problem in Harp (for example in device sync, shared playlists, listening
together, the update installer or the handling of links), please report it privately rather than in a
public issue: use GitHub's **Report a vulnerability** button on the repository's Security tab.

Please include what's affected, how to reproduce it, and the Harp version. You'll get a reply as soon
as possible, and a fix in the next release.

Releases are signed: the Android APK with Harp's release key, and every release lists SHA-256
checksums of its files in `SHA256SUMS.txt`. Harp's own updater checks those checksums before
installing anything.
