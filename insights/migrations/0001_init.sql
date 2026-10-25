-- One row per install and UTC day. Usage counts are deltas, so a second report the same day
-- adds its usage to the first; the snapshot fields (version, system, library…) are replaced.
CREATE TABLE reports (
  instance_id TEXT NOT NULL,
  day TEXT NOT NULL,
  received_at TEXT NOT NULL,
  data TEXT NOT NULL,
  PRIMARY KEY (instance_id, day)
);

CREATE INDEX reports_day_idx ON reports (day);

-- The day each random install id first reported: counts installs after raw reports are purged.
CREATE TABLE instances (
  instance_id TEXT PRIMARY KEY,
  first_seen TEXT NOT NULL
);

CREATE INDEX instances_first_seen_idx ON instances (first_seen);

-- Aggregated history behind the public dashboard. Never purged.
CREATE TABLE daily_summaries (
  day TEXT PRIMARY KEY,
  data TEXT NOT NULL
);
