# Directory names and recovery paths

Content scanning identifies files from their internal structures. It cannot infer the original filename or directory tree. These candidates have `relative_path: null` and generated names; recovery places them under `Unknown partition/Recovered by type/<TYPE>`.

FAT/exFAT metadata candidates carry surviving names and a relative path. The desktop recreates those parent directories on the destination, sanitizes unsafe characters, and preserves available basenames. Starting with v0.2.1, ordinary filenames no longer receive an unconditional numeric prefix. Actual collisions receive a suffix before the extension, such as `clip (2).mp4`. The recovery report retains the original metadata path separately. Empty directories are not independent recovery results.

## Corrected in v0.2.1

The FAT walker previously marked a directory's first cluster as visited before validating a deleted directory record. A rejected stale reference could therefore suppress a later valid reference to the same cluster. Admission now validates the record before recording a visit. Non-root live FAT directories also need consistent self/parent headers and a valid allocated first cluster. exFAT likewise records a visit only after validating its directory extent and allocation state.

For a deleted FAT directory, the first cluster must be free and its `.`/`..` records must agree on directory attributes, zero special-record sizes, self cluster and expected parent. An E5 deletion marker in the first byte of those two names is now accepted only when their remaining bytes and these independent checks agree. This is explicitly tentative recovery evidence, not a guarantee of original ancestry. A nonzero outer deleted-directory size can be ignored only after these checks; it never determines how many clusters to read. Live directory size checks remain strict.

Rejected references now identify the record byte offset, first cluster and relevant size or header mismatch. A duplicate reference identifies the path already inspected. Invalid references do not reserve a visit. The cycle guard remains enabled.

Only the confirmed first cluster of a deleted FAT directory is searched when its former chain has been cleared. Orphan directory discovery, arbitrary deleted chain reconstruction and empty-directory restoration remain unsupported. These fixes do not promise that a particular damaged USB drive's names will survive.

## Sources and validation

The [Microsoft FAT specification](https://download.microsoft.com/download/1/6/1/161ba512-40e2-4cc9-843a-923143f3456c/fatgen103.doc) defines directory entry sizes, special records, cluster pointers and deletion markers. [TestDisk's FAT directory implementation](https://github.com/cgsecurity/testdisk/blob/master/src/fat_dir.c) provides an independent comparison for deleted directory interpretation. The E5 special-name handling described above is a conservative heuristic, not a claim that all deletion implementations preserve these records.

Regression fixtures exercise stale references followed by valid ancestry, erased special names, Unicode deleted names, rejection of allocated or mismatched clusters, and output name collisions. Executed results are recorded in [VALIDATION.md](VALIDATION.md). No private source-image bytes or recovered user files are included in this repository.
