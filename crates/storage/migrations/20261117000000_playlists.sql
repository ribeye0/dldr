CREATE TABLE playlists (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(name) <= 80 AND (id = '0' OR length(trim(name)) > 0)),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
INSERT INTO playlists(id, name) VALUES ('0', '');

CREATE TABLE playlist_works (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(work_id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    PRIMARY KEY (playlist_id, work_id)
);
CREATE INDEX playlist_works_work ON playlist_works(work_id);
