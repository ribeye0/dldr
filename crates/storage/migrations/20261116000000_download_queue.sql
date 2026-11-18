CREATE TABLE download_queue (
    work_id TEXT PRIMARY KEY REFERENCES works(work_id) ON DELETE CASCADE,
    batch_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    account_id TEXT,
    library_root TEXT NOT NULL,
    download_root TEXT NOT NULL,
    unpack_policy TEXT NOT NULL CHECK(unpack_policy IN ('keep_archives','unpack_when_recognized')),
    replace_existing INTEGER NOT NULL CHECK(replace_existing IN (0,1)),
    status TEXT NOT NULL CHECK(status IN ('queued','running','pausing','cancelling','paused','cancelled','failed','completed')),
    error_message TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX download_queue_position ON download_queue(position,work_id);
