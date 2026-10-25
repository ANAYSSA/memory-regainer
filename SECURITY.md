# Security policy

This is an early prerelease. Only the current source revision is maintained. Recovery parsers process untrusted binary input; tests and memory-safe Rust reduce risk but do not prove the absence of parser or resource-exhaustion defects.

The current Rust dependency audit reports five allowed warnings, including a glib unsoundness advisory. Target-specific checks exclude those five packages from the Windows/macOS release dependency graphs; they remain selected for Linux. Linux is not packaged or security-qualified in this release. See the dated [dependency audit record](docs/AUDIT.md) for exact versions, advisory links, commands and remaining work. A passing CI audit must not be described as zero findings.

## Reporting

Use the repository's private vulnerability reporting feature when available. If it is unavailable, open a public issue containing only a request for a private reporting channel. Do not publish an exploit, private disk image, sensitive recovered content or credentials in that request. No private reporting mailbox is implied by this document.

Include the exact commit or release, platform, affected parser/operation, reproduction steps and a minimal synthetic input if safe. Explain whether the issue involves a crash, unintended read/write, path handling, preview rendering or incorrect recovery.

## Boundaries

- Source files and devices are opened read-only. There is no partition repair or source-disk write command.
- Recovery destinations are validated, outputs use collision-resistant handling, and existing source files must never be overwritten.
- Raw-device access depends on OS authorization. Starting with v0.1.1, the Windows application manifest requests administrator access through the standard UAC prompt, elevating the whole application. There is no silent elevation or bypass of encryption, TCC, SIP or disk locks.
- Previews do not execute files, scripts or embedded document macros. Do not open recovered executables merely because recovery succeeded.
- A SHA-256 match between read bytes and written bytes checks copy integrity. It does not establish that those bytes are the original pre-deletion content, safe to execute, or a complete valid file.

Do not run destructive experiments on personal storage. Use disposable fixtures. Physical read failures and disconnects require conservative handling; professional recovery may be appropriate for irreplaceable data.
