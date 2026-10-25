# Recovery safety

Stop using the affected storage. Installing any application, downloading its installer or saving files to that same drive can overwrite deleted content. When possible, run the app from another system and work from an image on a separate healthy disk.

The source is opened read-only. This does not stop the operating system or another application from modifying a mounted live volume. The OS and WebView can write normal caches and application settings independently of the recovery engine. A read-only handle is not a hardware write blocker or a snapshot. Source fingerprints detect some changes but do not cover every byte; use a stable image for repeatable analysis.

For physical sources, the application checks source/destination media and refuses a destination on the same physical device or a destination it cannot map safely. For a disk-image source, the existing image is treated as an independent copy: the app cannot know which original physical disk produced it. It does not overwrite the source image. The user must still keep the original affected storage untouched and choose healthy output storage.

Permissions are controlled by Windows, macOS or Linux. The app does not bypass access controls or automatically escalate privileges. If raw access is denied, use a legitimately acquired image or the normal OS-approved access procedure. FileVault, BitLocker and other encryption must be unlocked legitimately; this release has no decryption bypass.

Do not repeatedly scan a clicking, disconnecting or otherwise failing disk. The imaging implementation stops at the first read error and does not automatically retry. Irreplaceable data on physically failing hardware may require professional recovery. A partial image remains partial even if some files can be found in it.

Recovery confidence is evidence, not a success percentage. Fragmented, truncated or overwritten content can have valid signatures. A report's `verified-copy` status means the output matches the bytes read by the recovery operation; it is not proof of the original file's correctness. Open recovered documents and media cautiously, and never execute an unknown recovered program.
