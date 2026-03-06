-- Add MAXVALUE catch-all partitions to prevent insert failures when
-- the pre-defined monthly partitions are exhausted (July 2026).
--
-- These are idempotent: MySQL's ADD PARTITION will fail if p_future
-- already exists, but sqlx only runs each migration once.

ALTER TABLE events ADD PARTITION (
    PARTITION p_future VALUES LESS THAN MAXVALUE
);

ALTER TABLE delivery_log ADD PARTITION (
    PARTITION p_future VALUES LESS THAN MAXVALUE
);
