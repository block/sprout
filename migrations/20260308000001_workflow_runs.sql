CREATE TABLE IF NOT EXISTS workflow_runs (
    id                  BINARY(16)      NOT NULL,
    workflow_id         BINARY(16)      NOT NULL,
    status              ENUM('pending','running','waiting_approval','completed','failed','cancelled')
                        NOT NULL DEFAULT 'pending',
    trigger_event_id    VARBINARY(32),
    current_step        INT             NOT NULL DEFAULT 0,
    execution_trace     JSON            NOT NULL,
    started_at          DATETIME(6),
    completed_at        DATETIME(6),
    error_message       TEXT,
    created_at          DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (id),
    CONSTRAINT fk_wr_workflow
        FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
);

CREATE INDEX idx_wr_workflow ON workflow_runs (workflow_id);
CREATE INDEX idx_wr_status   ON workflow_runs (status);
