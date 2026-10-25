# Recovery I/O and responsiveness research

Date: 25 October 2026. Purpose: inform the v0.1.1 response to a reported Windows USB scan that appeared stalled, followed by sleep and OS error 1. The report does not establish which operation failed on that device. No access to that Windows computer or reproduction of its fault is claimed.

## Evidence and scope

This review combines bounded static import inspection of user-supplied Recoverit binaries, source inspection of the official CGSecurity repository, and public OS/acquisition documentation. No supplied proprietary program was executed. No proprietary implementation, asset, signature database or binary was copied into Memory Regainer. Open-source comparison files were downloaded only into ignored research scratch storage; no source from those projects was incorporated into the application.

### Recoverit follow-up

Six previously identified files were rechecked; their hashes match the [earlier report](RECOVERIT_ARCHITECTURE_REVIEW.md). One additional file was inspected: `drfoundation.dll`, 5,851,352 bytes, SHA-256 `839f4af0cfca29fdf931ee18bb6d4d228ea766c04b407d7e1395f012b328a8b3`. It is a PE32+ AMD64 library. These hashes identify the supplied copies, not their authenticity.

| Directly observed import evidence | What can be concluded |
| --- | --- |
| `recoverit.exe`: `SetThreadExecutionState` | The executable references a Windows power-management API. Its flags, call sites and lifetime were not reconstructed. |
| `drengcli.dll` and `drengsrv.exe`: I/O completion-port creation/dequeue and thread creation | These modules have asynchronous-dispatch building blocks. They may serve RPC networking; this does not prove asynchronous disk reads. |
| `drstoragedevice.dll`: `DeviceIoControl`, `ReadFile`, `SetFilePointerEx`, events and waits | Storage uses native Windows I/O and synchronization primitives. Exact geometry requests, read alignment, timeout and retry policies remain unknown. |
| Seven selected import tables contain no direct `CancelIo`, `CancelIoEx` or `CancelSynchronousIo` import | No direct reference was observed in this sample. Other modules, dynamic resolution or cooperative cancellation remain possible. |

The earlier architecture observations remain useful: separate scan, storage, recovery, result-management and service modules provide places to isolate responsibilities. None of these names reveal the recovery heuristics or prove correctness, performance or resilience after USB reconnection.

### PhotoRec / TestDisk: source verified

The reviewed upstream revision is [`f01b59f76596df457eb5a135a3fc638e91dd0c6b`](https://github.com/cgsecurity/testdisk/tree/f01b59f76596df457eb5a135a3fc638e91dd0c6b). The official project documents [GPL-2.0-or-later licensing](https://www.cgsecurity.org/testdisk_doc/presentation.html); its `COPYING` was inspected. This study is not a claim that PhotoRec is bundled or that Memory Regainer has its format coverage.

- The Windows reader discovers raw length with `IOCTL_DISK_GET_LENGTH_INFO` and sector geometry through disk-specific controls. It also has documented-in-source fallback paths. This confirms that a raw device needs a different sizing/geometry strategy from a regular image file. Its inspected read path uses synchronous seeking and `ReadFile`. [Windows reader source](https://github.com/cgsecurity/testdisk/blob/f01b59f76596df457eb5a135a3fc638e91dd0c6b/src/win32.c#L69-L142), [read path](https://github.com/cgsecurity/testdisk/blob/f01b59f76596df457eb5a135a3fc638e91dd0c6b/src/win32.c#L427-L490).
- QPhotoRec configures a 500 ms UI timer and reports sector position/file counts. The scanned loop also calls `QCoreApplication::processEvents`, checks a stop flag and saves sessions after a disk read returns. This inspected path uses cooperative progress/cancellation; it is not evidence of a worker-thread or immediately cancellable disk-read architecture. A timer cannot update a UI thread while that thread is blocked in I/O. [UI source](https://github.com/cgsecurity/testdisk/blob/f01b59f76596df457eb5a135a3fc638e91dd0c6b/src/qphotorec.cpp#L777-L835), [scan loop](https://github.com/cgsecurity/testdisk/blob/f01b59f76596df457eb5a135a3fc638e91dd0c6b/src/qpsearch.cpp#L283-L306).
- Recovery phases and saved search ranges exist in the inspected implementation. A saved result list alone is insufficient to reproduce a resumable scanner: pending ranges, phase and reconstruction state also matter. This is a design observation, not a proposal to copy its session format or implementation. [Phase handling](https://github.com/cgsecurity/testdisk/blob/f01b59f76596df457eb5a135a3fc638e91dd0c6b/src/qphotorec.cpp#L788-L835).

### GNU ddrescue: documented acquisition architecture

GNU ddrescue prioritizes readable regions, tracks block states in a mapfile and divides acquisition into copying, trimming, sweeping, scraping and retrying. Periodic state saves support continuation after interruption; without that state, restarting is not resuming. Its documentation explicitly warns that a damaged drive can block it until the kernel stops waiting. Direct I/O also needs correct sector alignment. These are reasons to separate damaged-media acquisition from filesystem interpretation and to make missing ranges explicit. Memory Regainer currently stops acquisition on the first read error and has no ddrescue backend or mapfile resume. [GNU ddrescue 1.30 manual](https://www.gnu.org/software/ddrescue/manual/ddrescue_manual.html).

GNU documents ddrescue as [GPL-2.0-or-later](https://www.gnu.org/s/ddrescue/ddrescue.html). A future integration would need an explicit packaging decision, corresponding source/notices, safe source/destination identity checks and preserved mapfiles. The current release does not bundle it.

### OS contracts relevant to the incident

Windows error 1 is `ERROR_INVALID_FUNCTION`; permission denial is error 5. The code alone cannot identify whether a geometry query, regular-file metadata operation, seek, read or device state caused the failure. Preserve the failing operation and original OS code instead of replacing every failure with a permissions message. [Microsoft error-code reference](https://learn.microsoft.com/en-us/windows/win32/debug/system-error-codes--0-499-).

Windows does not treat ordinary disk or CPU activity as a sufficient idle-sleep signal. `ES_CONTINUOUS | ES_SYSTEM_REQUIRED` can protect an active worker from automatic idle sleep; the requirement must be released afterward. It does not prevent manual sleep or lid-close behavior, and it need not keep the display lit. [Microsoft power API](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate).

Windows `CancelIoEx` requests cancellation; it does not wait for completion or guarantee that cancellation wins a race with a completed read. An asynchronous implementation must retain its buffers and `OVERLAPPED` state until completion and inspect the final result. Merely setting an atomic flag cannot cancel a synchronous kernel read already in progress. [Microsoft cancellation contract](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex).

## Independent implementation decisions and validation boundary

The v0.1.1 work targets OS-aware raw readers, bounded read caching, background preparation, meaningful phase reporting, exact errors and operation-scoped idle sleep protection. The proposed Windows elevation uses normal UAC approval; there is no privilege bypass. These changes do not establish the cause of the reported USB failure or guarantee recovery on that device.

The frontend should distinguish an indeterminate metadata/device-opening stage from sequential byte progress, sample rates only within the same stage, and label ETA as an estimate. A responsive elapsed timer must not imply that new bytes are arriving. Cancellation should remain visibly pending until the worker has stopped. Source revalidation and recovery verification are separate work stages.

Deterministic reader/metadata tests, stage and error-path tests, native builds and power-assertion checks provide useful evidence. They do not replace disposable-device Windows testing for UAC, USB disconnect/reconnect, blocked I/O, idle sleep and manual sleep/wake. Exact completed checks are recorded in [VALIDATION.md](VALIDATION.md). Checkpoint resume, persistent result indexing, a dedicated privileged helper and guaranteed low-level I/O interruption remain future engineering work.
