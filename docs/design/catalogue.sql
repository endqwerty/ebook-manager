-- A one-library query model for the import experiment.
PRAGMA foreign_keys = ON;

CREATE TABLE work (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL
);

CREATE INDEX shelf_order ON work(title, id);

CREATE TABLE edition (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES work(id)
);

CREATE TABLE file (
    id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE edition_file (
    edition_id TEXT NOT NULL REFERENCES edition(id),
    file_id TEXT NOT NULL REFERENCES file(id),
    PRIMARY KEY (edition_id, file_id)
);

CREATE TABLE file_version (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL REFERENCES file(id),
    sha256 TEXT NOT NULL,
    package_id TEXT,
    UNIQUE (file_id, sha256)
);
CREATE INDEX version_by_digest ON file_version(sha256);
-- A shared package identifier proposes a match for review. It never merges records.
CREATE INDEX version_by_package ON file_version(package_id);

-- The experiment models one installation and one granted root. Paths are relative
-- to that root. Supporting multiple roots requires a root ID in the location key.
CREATE TABLE location (
    path TEXT PRIMARY KEY NOT NULL,
    version_id TEXT NOT NULL REFERENCES file_version(id)
);
