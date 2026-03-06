-- Migration: Fix workflows table schema
--
-- The initial schema (20260306000001) placed run-state columns on the `workflows`
-- table. Those belong in `workflow_runs`. This migration:
--   1. Converts existing status values to the new definition-only enum.
--   2. Adds the `enabled` flag for soft-disabling without archiving.
--   3. Drops all run-state columns that now live in `workflow_runs`.
--
-- Row conversion:
--   pending    → active   (was never activated, treat as active definition)
--   cancelled  → archived (intentionally stopped)
--   everything else → active (running/waiting_approval/completed/failed were
--                             run states, not definition states)

-- Step 1: Convert existing status values before changing the ENUM definition.
--         MySQL requires the value to be valid in the NEW enum before ALTER,
--         so we update first while the old enum is still in place.
UPDATE workflows SET status = 'active'   WHERE status IN ('pending', 'running', 'waiting_approval', 'completed', 'failed');
UPDATE workflows SET status = 'archived' WHERE status = 'cancelled';

-- Step 2: Add the `enabled` column (default TRUE — all existing rows are enabled).
ALTER TABLE workflows
    ADD COLUMN enabled BOOLEAN NOT NULL DEFAULT TRUE
        AFTER status;

-- Step 3: Change the status ENUM to definition-only values.
ALTER TABLE workflows
    MODIFY COLUMN status ENUM('active', 'disabled', 'archived') NOT NULL DEFAULT 'active';

-- Step 4: Drop run-state columns that belong in workflow_runs.
ALTER TABLE workflows
    DROP COLUMN trigger_event_id,
    DROP COLUMN current_step,
    DROP COLUMN execution_trace,
    DROP COLUMN started_at,
    DROP COLUMN completed_at,
    DROP COLUMN failed_at,
    DROP COLUMN error_message;

-- Step 5: Add a composite index to support the trigger-matching query:
--   WHERE channel_id = ? AND status = 'active' AND enabled = TRUE
CREATE INDEX idx_workflows_channel_active
    ON workflows (channel_id, status, enabled);
