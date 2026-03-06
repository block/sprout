-- Add trigger_context column to workflow_runs so that the original trigger data
-- is persisted and can be restored when a suspended workflow resumes after approval.
--
-- This fixes the bug where {{trigger.*}} template variables resolved to empty strings
-- in post-approval steps because TriggerContext::default() was used on resume.
--
-- NULL means no trigger context was captured (backwards-compatible with existing rows).
ALTER TABLE workflow_runs
    ADD COLUMN trigger_context JSON DEFAULT NULL AFTER execution_trace;
