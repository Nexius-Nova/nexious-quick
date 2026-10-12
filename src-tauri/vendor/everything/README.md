# Everything SDK

Official Voidtools Everything SDK, downloaded from https://www.voidtools.com/Everything-SDK.zip on 2026-10-12.

The three upstream source/header files are included unchanged (except the IPC header filename's casing). Their MIT copyright and permission notices are retained in each file. They are compiled into the Windows application; no Node.js runtime, HTTP server or external DLL is required.

The user must have Everything 1.4.1 or later running with its local index loaded. Only read/query APIs are called. The query runs in an isolated child process so a stalled IPC response can be timed out without blocking the app or corrupting its index.
