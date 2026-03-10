ALTER TABLE events ADD COLUMN deleted_at DATETIME(6) DEFAULT NULL;
CREATE INDEX idx_events_deleted ON events (deleted_at);
