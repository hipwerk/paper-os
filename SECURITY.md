# Security policy

Please report security issues privately through the repository's GitHub
security-advisory feature once the remote is configured. Do not open a public
issue for credential exposure, unsafe update paths, or remotely exploitable
behavior.

PaperOS is pre-release. Security-sensitive areas include network-fed content,
image/font parsers, deployment over SSH, configuration containing local device
details, and any future OTA mechanism. Keep credentials out of the repository
and treat rendered external content as untrusted input.
