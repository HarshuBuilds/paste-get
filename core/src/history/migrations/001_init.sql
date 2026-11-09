-- Schema 1 of the clipboard history.
--
-- Never edit this file once released: add 002_….sql instead. The test `migrations_are_pinned`
-- holds its hash. `auto_vacuum = INCREMENTAL` and the per-connection pragmas are set by
-- `schema.rs` before this runs.

-- classifier_version, last_sweep_at.
CREATE TABLE meta (key TEXT PRIMARY KEY, value) WITHOUT ROWID;
INSERT INTO meta (key, value) VALUES ('classifier_version', 1);

-- Devices clips came from. Names survive unpairing.
CREATE TABLE device (
  id       INTEGER PRIMARY KEY,
  key      BLOB    NOT NULL UNIQUE,         -- 32-byte DeviceId
  name     TEXT    NOT NULL,
  platform INTEGER                          -- proto Platform
);

-- One row per unique content.
CREATE TABLE item (
  id         INTEGER PRIMARY KEY,
  sha256     BLOB    NOT NULL UNIQUE,       -- of the exact bytes = ClipOffer.sha256
  format     INTEGER NOT NULL,              -- 1 text/utf-8, 2 PNG, 3 JPEG
  size       INTEGER NOT NULL,              -- bytes of the original
  category   INTEGER NOT NULL,              -- detect::Category: 1 text, 2 link, 3 email, 4 phone,
                                            -- 5 code, 6 json, 7 color, 8 screenshot, 9 image
  host       TEXT,                          -- single-link texts: detect::link().host
  source     INTEGER NOT NULL,              -- 1 copied here, 2 received, 3 screenshot, 4 shared,
                                            -- 5 sent by hand
  origin     INTEGER REFERENCES device(id), -- where first copied (Clip.origin); NULL = this device
  via        INTEGER REFERENCES device(id), -- peer it arrived from; NULL = local
  first_at   INTEGER NOT NULL,              -- unix ms
  last_at    INTEGER NOT NULL,
  times      INTEGER NOT NULL DEFAULT 1,
  pinned_at  INTEGER,                       -- never evicted
  expires_at INTEGER,                       -- bare codes (15 min), "delete in 1 hour"
  text_head  TEXT,                          -- first 4 KiB on a char boundary; whole text when short
  kept       INTEGER NOT NULL,              -- 0 inline, 1 body row, 2 original file,
                                            -- 3 gallery reference, 4 too large,
                                            -- 5 original evicted, 6 gone
  media_id   INTEGER,                       -- MediaStore _ID (phone screenshots)
  width      INTEGER,
  height     INTEGER,
  thumb      INTEGER NOT NULL DEFAULT 0     -- 0 n/a, 1 pending, 2 done, 3 failed
);
CREATE INDEX item_recent ON item(last_at DESC, id DESC);
CREATE INDEX item_cat    ON item(category, last_at DESC);
CREATE INDEX item_pinned ON item(pinned_at)  WHERE pinned_at  IS NOT NULL;
CREATE INDEX item_expiry ON item(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX item_media  ON item(media_id)   WHERE media_id   IS NOT NULL;

-- Full text over 4 KiB, deflated.
CREATE TABLE body (
  item INTEGER PRIMARY KEY REFERENCES item(id) ON DELETE CASCADE,
  data BLOB NOT NULL
);

-- WebP, JPEG or PNG, at most 64 KiB, signature checked.
CREATE TABLE thumb (
  item INTEGER PRIMARY KEY REFERENCES item(id) ON DELETE CASCADE,
  data BLOB NOT NULL
);

-- Clips kept out, as numbers only: the "kept out" counter. `day` is days since 1970 (UTC).
CREATE TABLE skipped (
  day    INTEGER NOT NULL,
  reason INTEGER NOT NULL,
  count  INTEGER NOT NULL,
  PRIMARY KEY (day, reason)
) WITHOUT ROWID;

-- Search on any part of a word, accents ignored. Contentless: the text is not stored twice.
-- A delete hands FTS5 the text that was indexed (the 'delete' command), and `secure-delete`
-- then takes its trigrams out of the index's pages. (`contentless_delete=1` would only mark the
-- row deleted, and leave its trigrams in the file.) So what is indexed never changes:
-- `text_head` can't be updated, and `ocr` stays empty until screenshots are read; whatever is
-- indexed there must be kept to delete it.
CREATE VIRTUAL TABLE item_fts USING fts5(
  text, ocr,
  content='',
  tokenize='trigram remove_diacritics 1'
);
INSERT INTO item_fts(item_fts, rank) VALUES ('secure-delete', 1);

CREATE TRIGGER item_fts_insert AFTER INSERT ON item BEGIN
  INSERT INTO item_fts(rowid, text, ocr) VALUES (new.id, new.text_head, '');
END;
CREATE TRIGGER item_fts_delete AFTER DELETE ON item BEGIN
  INSERT INTO item_fts(item_fts, rowid, text, ocr) VALUES ('delete', old.id, old.text_head, '');
END;
CREATE TRIGGER item_text_head_fixed BEFORE UPDATE OF text_head ON item BEGIN
  SELECT RAISE(ABORT, 'text_head is indexed and never changes');
END;
