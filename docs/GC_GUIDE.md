# Garbage collection guide

This document explains how Shai's garbage collection works and why it's safe.

## What GC does

Garbage collection removes old blob payloads from the storage to free disk space. Shai has two modes:

- **Archive mode** (default): Move old blobs to `blobs_archive.redb` for later recovery
- **Delete mode** (`--delete`): Permanently remove old blobs

The metadata (timestamps, summaries, tool names) stays in SQLite even after GC. Only the file content is removed.

## How to run GC

### Dry run (see what would be deleted)

```bash
shai gc --days 30 --dry-run
```

### Archive old blobs

```bash
shai gc --days 30
```

This moves blobs older than 30 days to `blobs_archive.redb`.

### Delete old blobs permanently

```bash
shai gc --days 30 --delete
```

⚠️ **Warning:** This is permanent. Use `--dry-run` first.

## Why it's safe: The Ancestry Walk

Even though current versions of Shai primarily use full snapshots, the storage engine supports delta chains. GC must be careful never to break a chain where a recent change depends on an old base.

Before deleting a change, Shai walks up the `base_change_id` chain. This marks all ancestors as protected. If any descendant is non-expired (recent), the entire chain is kept.

## After GC

If you need to recover a deleted change:

1. Metadata remains intact: You can still see that the file was modified in `shai log`.
2. Content is gone: Attempting to `shai rollback` to a deleted checkpoint will fail with "blob metadata missing".
3. Recovery: If you used default mode, the blob still exists in `.shai/blobs_archive.redb`.

## Troubleshooting GC

### "Everything is protected, nothing deleted"
Most likely your changes are all recent (within the `--days` threshold). Try increasing the threshold.

### "GC is too slow"
If you have >100k changes, the initial scan of the SQLite `changes` table may take a few seconds. This is normal local-first behavior.

## Design philosophy

Shai's GC is conservative:
- ✅ Protects any change that newer data depends on.
- ✅ Never corrupts delta chains.
- ✅ Metadata is preserved even after deletion.
- ✅ Archive mode allows manual recovery.

If unsure, keep the data. Disk is cheap; losing context is expensive.
