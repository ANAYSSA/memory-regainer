# Privacy

Memory Regainer performs scanning, previewing, hashing and recovery locally. It has no account system, advertising SDK, analytics SDK, telemetry service or file-upload feature. The recovery application does not require an internet connection. The build system downloads dependencies; GitHub downloads and OS WebView components operate under their own policies.

The application reads only the source the user selects for a scan or image operation. Source bytes may contain sensitive information, including files that had been deleted. Previews are bounded and held locally. The program does not execute recovered programs.

Preferences and scan-session metadata are stored locally in the operating system's application-data locations. A saved manifest can contain a source path, filenames, offsets, sizes and recovery notes. Recovery reports can contain destination paths, names, hashes and errors. Recovered files, disk images and exported manifests are written to user-selected locations. Review these files before sharing them; they are not anonymized automatically.

Removing a preference or manifest does not securely erase storage. Operating systems can retain snapshots, swap, backups or crash reports. The application makes no secure-erasure promise.

Do not attach personal disk images or recovered content to public issues. Use synthetic reproduction files and redact paths. See [SECURITY.md](SECURITY.md).
