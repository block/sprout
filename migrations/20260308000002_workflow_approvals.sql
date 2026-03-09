CREATE TABLE IF NOT EXISTS workflow_approvals (
    token           VARCHAR(36)     NOT NULL,
    workflow_id     BINARY(16)      NOT NULL,
    run_id          BINARY(16)      NOT NULL,
    step_id         VARCHAR(64)     NOT NULL,
    step_index      INT             NOT NULL,
    approver_spec   TEXT            NOT NULL,
    status          ENUM('pending','granted','denied','expired')
                    NOT NULL DEFAULT 'pending',
    approver_pubkey VARBINARY(32),
    note            TEXT,
    granted_at      DATETIME(6),
    denied_at       DATETIME(6),
    expires_at      DATETIME(6)     NOT NULL,
    created_at      DATETIME(6)     NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    PRIMARY KEY (token),
    CONSTRAINT fk_wa_workflow
        FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE,
    CONSTRAINT fk_wa_run
        FOREIGN KEY (run_id) REFERENCES workflow_runs(id) ON DELETE CASCADE
);

CREATE INDEX idx_wa_workflow ON workflow_approvals (workflow_id);
CREATE INDEX idx_wa_run      ON workflow_approvals (run_id);
CREATE INDEX idx_wa_status   ON workflow_approvals (status);
